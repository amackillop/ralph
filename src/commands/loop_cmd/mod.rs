//! Main Ralph loop command.
//!
//! This module runs the iterative AI development loop. Core logic
//! is separated into submodules for maintainability:
//! - `format`: Output formatting and progress display
//! - `git`: Git operations (push, branch, commit)
//! - `worktree`: Git worktree management for parallel builds

mod format;
mod git;
pub(crate) mod worktree;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::agent::{AgentProvider, ClaudeProvider, CursorProvider, Provider};
use crate::config::Config;
use crate::detection::{get_commit_hash, CompletionDetector};
use crate::notifications::{NotificationDetails, NotificationEvent, Notifier};
use crate::sandbox::{DockerSandbox, Sandbox, SandboxError};
use crate::state::{Mode, RalphState};

use format::{
    format_banner, format_completion_detected, format_iteration_header, format_loop_finished,
    format_max_iterations_reached, format_progress, BannerInfo, ProgressInfo,
};
use git::{check_gh_available, create_pull_request, git_push};
use worktree::{
    configure_worktree_identity, copy_plan_to_worktree, create_worktree, enable_worktree_config,
    parse_implementation_plan, worktree_path, BranchSection,
};

/// Check if a branch section has incomplete tasks.
///
/// Returns true if the branch has any unchecked `- [ ]` tasks before the next
/// `## Branch:` header.
fn is_branch_incomplete(plan_content: &str, branch_name: &str) -> bool {
    let header = format!("## Branch: {branch_name}");
    let Some(start) = plan_content.find(&header) else {
        return false;
    };

    // Find where this branch section ends (next ## Branch: or end of file)
    let section_start = start + header.len();
    let section_end = plan_content[section_start..]
        .find("## Branch:")
        .map_or(plan_content.len(), |pos| section_start + pos);

    let section = &plan_content[section_start..section_end];

    // Check for any unchecked tasks
    section.contains("- [ ]")
}

// -----------------------------------------------------------------------------
// Dependency Injection for Testing
// -----------------------------------------------------------------------------

/// Dependencies for the loop that can be injected for testing.
#[cfg(test)]
pub(crate) struct LoopDependencies {
    /// The agent provider to use.
    pub agent: Box<dyn AgentProvider>,
    /// Optional sandbox for isolation.
    pub sandbox: Option<Box<dyn Sandbox>>,
    /// Configuration.
    pub config: Config,
    /// Project directory.
    pub project_dir: PathBuf,
    /// Prompt file path.
    pub prompt_file: PathBuf,
}

/// Result from running the loop core, for test verification.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct LoopResult {
    /// Final iteration reached.
    pub final_iteration: u32,
    /// How the loop terminated.
    pub termination_reason: TerminationReason,
    /// Total errors encountered.
    pub error_count: u32,
}

/// Why the loop terminated.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminationReason {
    /// Max iterations reached.
    MaxIterations,
    /// Completion detected (idle threshold).
    CompletionDetected,
    /// Loop was cancelled externally.
    Cancelled,
    /// Fatal error occurred.
    Error(String),
}

/// Run the loop with injected dependencies (for testing).
///
/// This is an internal function for E2E testing that allows mocking
/// the agent and sandbox while testing the full loop orchestration logic.
#[cfg(test)]
#[allow(clippy::too_many_lines, tail_expr_drop_order)]
pub(crate) async fn run_loop_core(
    deps: LoopDependencies,
    initial_state: RalphState,
) -> Result<LoopResult> {
    let LoopDependencies {
        agent,
        sandbox,
        config,
        project_dir,
        prompt_file,
    } = deps;

    let mut state = initial_state;
    state.active = true;
    state.save(&project_dir)?;

    // Initialize completion detector from persisted state for idle detection
    // continuity across restarts
    let mut detector = CompletionDetector::from_state(
        config.completion.idle_threshold,
        state.last_commit.clone(),
        state.idle_iterations,
    );

    // Create persistent container if sandbox is enabled and reuse is configured
    let persistent_container_name = if let Some(ref sb) = sandbox {
        if config.sandbox.reuse_container {
            match sb.create_persistent(&project_dir).await {
                Ok(name) if !name.is_empty() => Some(name),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let termination_reason;

    // Main loop
    loop {
        // Check for external cancellation (e.g., `ralph cancel`)
        if let Some(loaded) = RalphState::load(&project_dir)? {
            if !loaded.active {
                state.active = false;
                state.save(&project_dir)?;
                termination_reason = TerminationReason::Cancelled;
                break;
            }
        }

        // Check max iterations
        if is_max_iterations_reached(&state) {
            state.active = false;
            state.save(&project_dir)?;
            termination_reason = TerminationReason::MaxIterations;
            break;
        }

        // Read prompt
        let mut prompt = std::fs::read_to_string(&prompt_file)
            .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))?;

        // Append validation errors from previous iteration if present
        if let Some(ref last_error) = state.last_error {
            if last_error.starts_with("Validation error:") {
                let error_details = last_error
                    .strip_prefix("Validation error:")
                    .unwrap_or(last_error);

                prompt.push_str("\n\n");
                prompt.push_str("## ⚠️ VALIDATION ERROR FROM PREVIOUS ITERATION\n");
                prompt.push_str("The following validation error occurred. Please fix it:\n\n");
                prompt.push_str("```\n");
                prompt.push_str(error_details.trim());
                prompt.push_str("\n```\n");
                prompt.push_str(
                    "\nFix the issues above and ensure validation passes before proceeding.\n",
                );
            }
        }

        // Run agent
        let output_result = if let Some(ref sb) = sandbox {
            sb.run(&project_dir, &prompt, persistent_container_name.as_deref())
                .await
        } else {
            agent.invoke(&project_dir, &prompt).await
        };

        // Handle agent execution result
        let _output = match output_result {
            Ok(out) => out,
            Err(e) => {
                let error_msg = e.to_string();

                // Check if this is a recoverable error
                let is_timeout = error_msg.contains("timed out");
                let is_rate_limit = error_msg.contains("resource_exhausted")
                    || error_msg.contains("rate limit")
                    || error_msg.contains("Rate limit");

                if is_timeout || is_rate_limit {
                    state.error_count += 1;
                    state.consecutive_errors += 1;
                    state.last_error = Some(error_msg);
                    state.last_iteration_at = Some(chrono::Utc::now());
                    state.iteration += 1;
                    state.save(&project_dir)?;

                    // Circuit breaker
                    if config.monitoring.max_consecutive_errors > 0
                        && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                    {
                        if let (Some(container_name), Some(sb)) =
                            (&persistent_container_name, &sandbox)
                        {
                            let _ = sb.remove_persistent(container_name).await;
                        }
                        termination_reason =
                            TerminationReason::Error("Circuit breaker triggered".to_string());
                        break;
                    }
                    continue;
                }

                // Non-recoverable error
                if let (Some(container_name), Some(sb)) = (&persistent_container_name, &sandbox) {
                    let _ = sb.remove_persistent(container_name).await;
                }
                return Err(e).context("Agent execution failed");
            }
        };

        // Validate code if enabled
        if config.validation.enabled {
            match validate_code(&project_dir, &config.validation.command).await {
                Ok(()) => {
                    if let Some(ref last_error) = state.last_error {
                        if last_error.starts_with("Validation error:") {
                            state.last_error = None;
                        }
                    }
                }
                Err(full_error) => {
                    state.error_count += 1;
                    state.consecutive_errors += 1;
                    state.last_error = Some(format!("Validation error:{full_error}"));
                    state.last_iteration_at = Some(chrono::Utc::now());
                    state.iteration += 1;
                    state.save(&project_dir)?;

                    // Circuit breaker
                    if config.monitoring.max_consecutive_errors > 0
                        && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                    {
                        if let (Some(container_name), Some(sb)) =
                            (&persistent_container_name, &sandbox)
                        {
                            let _ = sb.remove_persistent(container_name).await;
                        }
                        termination_reason =
                            TerminationReason::Error("Circuit breaker triggered".to_string());
                        break;
                    }
                    continue;
                }
            }
        }

        // Successful iteration
        state.consecutive_errors = 0;
        state.last_iteration_at = Some(chrono::Utc::now());

        // Check for cancellation again (agent may have been cancelled externally during execution)
        if let Some(loaded) = RalphState::load(&project_dir)? {
            if !loaded.active {
                state.active = false;
                state.save(&project_dir)?;
                termination_reason = TerminationReason::Cancelled;
                break;
            }
        }

        state.save(&project_dir)?;

        // Check for completion (idle detection - no real git in tests, so always idle)
        // In real usage, this compares git commit hashes
        // check_completion updates detector's internal state
        let is_complete = detector.check_completion(None);

        // Sync detector state to RalphState for persistence across restarts
        state.last_commit = detector.last_commit().map(String::from);
        state.idle_iterations = detector.idle_count();

        if is_complete {
            state.active = false;
            state.save(&project_dir)?;
            termination_reason = TerminationReason::CompletionDetected;
            break;
        }

        // Increment iteration
        state.iteration += 1;
        state.save(&project_dir)?;
    }

    // Cleanup
    if let (Some(container_name), Some(sb)) = (persistent_container_name, &sandbox) {
        let _ = sb.remove_persistent(&container_name).await;
    }

    Ok(LoopResult {
        final_iteration: state.iteration,
        termination_reason,
        error_count: state.error_count,
    })
}

// -----------------------------------------------------------------------------
// Branch Build Types
// -----------------------------------------------------------------------------

/// Result of building a single branch.
#[derive(Debug, Clone)]
pub(crate) struct BranchResult {
    /// Branch name.
    pub branch: String,
    /// Whether the build succeeded.
    pub success: bool,
    /// Final iteration count.
    pub iterations: u32,
    /// Error message if failed.
    pub error: Option<String>,
    /// PR URL if created.
    pub pr_url: Option<String>,
}

impl BranchResult {
    fn success(branch: &str, iterations: u32, pr_url: Option<String>) -> Self {
        Self {
            branch: branch.to_string(),
            success: true,
            iterations,
            error: None,
            pr_url,
        }
    }

    fn failure(branch: &str, iterations: u32, error: String) -> Self {
        Self {
            branch: branch.to_string(),
            success: false,
            iterations,
            error: Some(error),
            pr_url: None,
        }
    }
}

// -----------------------------------------------------------------------------
// Branch Build Execution
// -----------------------------------------------------------------------------

/// Execute builds for all branches in parallel or sequential mode.
#[allow(tail_expr_drop_order)]
async fn execute_branch_builds(
    branches: Vec<BranchSection>,
    config: &Config,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    provider_override: Option<&str>,
    sequential: bool,
) -> Result<Vec<BranchResult>> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Enable worktree config extension
    enable_worktree_config(&cwd).await?;

    // Check if gh CLI is available for PR creation
    let gh_available = config.git.auto_pr && check_gh_available().await;
    if config.git.auto_pr && !gh_available {
        warn!("gh CLI not available or not authenticated. PRs will not be created automatically.");
    }

    // Prepare worktrees for all branches
    info!("Preparing {} worktrees...", branches.len());
    for branch in &branches {
        // Create worktree
        if let Err(e) = create_worktree(&cwd, &branch.name).await {
            // Branch may already exist, try to continue
            warn!("Failed to create worktree for '{}': {}", branch.name, e);
            continue;
        }

        // Configure identity if specified
        if let Some(ref wt_config) = config.git.worktree {
            if let Err(e) = configure_worktree_identity(&cwd, &branch.name, wt_config).await {
                warn!("Failed to configure identity for '{}': {}", branch.name, e);
            }
        }

        // Copy implementation plan
        if let Err(e) = copy_plan_to_worktree(&cwd, &branch.name) {
            warn!("Failed to copy plan to '{}': {}", branch.name, e);
        }
    }

    // Execute builds
    let results = if sequential {
        execute_sequential(
            &cwd,
            branches,
            config,
            max_iterations,
            no_sandbox,
            provider_override,
            gh_available,
        )
        .await
    } else {
        execute_parallel(
            &cwd,
            branches,
            config,
            max_iterations,
            no_sandbox,
            provider_override,
            gh_available,
        )
        .await
    };

    results
}

/// Execute branch builds sequentially.
async fn execute_sequential(
    project_dir: &Path,
    branches: Vec<BranchSection>,
    config: &Config,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    provider_override: Option<&str>,
    gh_available: bool,
) -> Result<Vec<BranchResult>> {
    let mut results = Vec::with_capacity(branches.len());

    for branch in branches {
        info!("Building branch '{}' sequentially...", branch.name);
        let result = build_single_branch(
            project_dir,
            &branch,
            config,
            max_iterations,
            no_sandbox,
            provider_override,
            gh_available,
        )
        .await;
        results.push(result);
    }

    Ok(results)
}

/// Execute branch builds in parallel using tokio `JoinSet`.
#[allow(tail_expr_drop_order)]
async fn execute_parallel(
    project_dir: &Path,
    branches: Vec<BranchSection>,
    config: &Config,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    provider_override: Option<&str>,
    gh_available: bool,
) -> Result<Vec<BranchResult>> {
    use tokio::task::JoinSet;

    let mut join_set = JoinSet::new();

    for branch in branches {
        let project_dir = project_dir.to_path_buf();
        let config = config.clone();
        let provider_override = provider_override.map(String::from);

        join_set.spawn(async move {
            info!("Building branch '{}' in parallel...", branch.name);
            build_single_branch(
                &project_dir,
                &branch,
                &config,
                max_iterations,
                no_sandbox,
                provider_override.as_deref(),
                gh_available,
            )
            .await
        });
    }

    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(branch_result) => results.push(branch_result),
            Err(e) => {
                warn!("Branch task panicked: {}", e);
            }
        }
    }

    Ok(results)
}

/// Build a single branch in its worktree.
async fn build_single_branch(
    project_dir: &Path,
    branch: &BranchSection,
    config: &Config,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    provider_override: Option<&str>,
    gh_available: bool,
) -> BranchResult {
    let wt_path = worktree_path(project_dir, &branch.name);

    // Run the loop in the worktree directory
    match run_branch_loop(
        &wt_path,
        config,
        max_iterations,
        no_sandbox,
        provider_override,
    )
    .await
    {
        Ok(iterations) => {
            // Try to create PR if enabled
            let pr_url = if gh_available {
                match create_pull_request(
                    &wt_path,
                    &branch.name,
                    &config.git.pr_base,
                    &format!("{}: {}", branch.name, branch.goal),
                    &format!(
                        "## Summary\n\n{}\n\n## Branch\n\n`{}`\n\n---\n\n🤖 Generated by Ralph",
                        branch.goal, branch.name
                    ),
                )
                .await
                {
                    Ok(url) => Some(url),
                    Err(e) => {
                        warn!("Failed to create PR for '{}': {}", branch.name, e);
                        None
                    }
                }
            } else {
                None
            };

            BranchResult::success(&branch.name, iterations, pr_url)
        }
        Err(e) => BranchResult::failure(&branch.name, 0, e.to_string()),
    }
}

/// Run the loop for a single branch (simplified version of the main loop).
#[allow(clippy::too_many_lines, tail_expr_drop_order)]
async fn run_branch_loop(
    wt_path: &Path,
    config: &Config,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    provider_override: Option<&str>,
) -> Result<u32> {
    use crate::detection::{get_commit_hash, CompletionDetector};

    // Determine prompt file
    let prompt_file = wt_path.join("PROMPT_build.md");
    if !prompt_file.exists() {
        bail!(
            "Prompt file not found in worktree: {}",
            prompt_file.display()
        );
    }

    // Load or create state for this worktree
    let mut state = RalphState::load_or_create(wt_path, Mode::Build)?;
    state.max_iterations = max_iterations;
    state.active = true;
    state.save(wt_path)?;

    // Get agent provider
    let provider = resolve_provider(config, provider_override)?;

    // Create agent
    let agent: Box<dyn AgentProvider> = match provider {
        Provider::Cursor => Box::new(CursorProvider::new(config.agent.cursor.clone())),
        Provider::Claude => Box::new(ClaudeProvider::new(config.agent.claude.clone())),
    };

    // Create sandbox if enabled
    let sandbox: Option<Box<dyn Sandbox>> = if !no_sandbox && config.sandbox.enabled {
        Some(Box::new(DockerSandbox::new(
            config.clone(),
            provider,
            config.agent.clone(),
        )))
    } else {
        None
    };

    // Initialize completion detector
    let mut detector = CompletionDetector::from_state(
        config.completion.idle_threshold,
        state.last_commit.clone(),
        state.idle_iterations,
    );

    // Main loop for this branch
    loop {
        // Check for cancellation
        if let Some(loaded) = RalphState::load(wt_path)? {
            if !loaded.active {
                state.active = false;
                state.save(wt_path)?;
                break;
            }
        }

        // Check max iterations
        if state
            .max_iterations
            .is_some_and(|max| state.iteration > max)
        {
            state.active = false;
            state.save(wt_path)?;
            break;
        }

        // Record commit hash at start
        let start_commit = get_commit_hash(wt_path).await;
        detector.record_commit(start_commit);

        // Read prompt
        let mut prompt = std::fs::read_to_string(&prompt_file)
            .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))?;

        // Append validation errors if present
        if let Some(ref last_error) = state.last_error {
            if last_error.starts_with("Validation error:") {
                let error_details = last_error
                    .strip_prefix("Validation error:")
                    .unwrap_or(last_error);
                prompt.push_str("\n\n## ⚠️ VALIDATION ERROR FROM PREVIOUS ITERATION\n");
                prompt.push_str("The following validation error occurred. Please fix it:\n\n```\n");
                prompt.push_str(error_details.trim());
                prompt.push_str("\n```\n\nFix the issues above and ensure validation passes.\n");
            }
        }

        // Run agent
        let output_result = if let Some(ref sb) = sandbox {
            sb.run(wt_path, &prompt, None).await
        } else {
            let timeout_mins = resolve_timeout(config, provider);
            let timeout_duration = std::time::Duration::from_secs(u64::from(timeout_mins) * 60);
            tokio::time::timeout(timeout_duration, agent.invoke(wt_path, &prompt))
                .await
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Agent execution timed out after {timeout_mins} minutes"
                    ))
                })
        };

        // Handle result
        match output_result {
            Ok(_) => {
                state.consecutive_errors = 0;
            }
            Err(e) => {
                let error_msg = e.to_string();
                let is_recoverable = error_msg.contains("timed out")
                    || error_msg.contains("rate limit")
                    || error_msg.contains("resource_exhausted");

                if is_recoverable {
                    state.error_count += 1;
                    state.consecutive_errors += 1;
                    state.last_error = Some(error_msg);
                    state.iteration += 1;
                    state.save(wt_path)?;

                    if config.monitoring.max_consecutive_errors > 0
                        && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                    {
                        state.active = false;
                        state.save(wt_path)?;
                        bail!("Circuit breaker triggered");
                    }
                    continue;
                }
                return Err(e);
            }
        }

        // Validate if enabled
        if config.validation.enabled {
            if let Err(validation_error) = validate_code(wt_path, &config.validation.command).await
            {
                state.error_count += 1;
                state.consecutive_errors += 1;
                state.last_error = Some(format!("Validation error:{validation_error}"));
                state.iteration += 1;
                state.save(wt_path)?;

                if config.monitoring.max_consecutive_errors > 0
                    && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                {
                    state.active = false;
                    state.save(wt_path)?;
                    bail!("Circuit breaker triggered");
                }
                continue;
            }
            // Clear validation error on success
            if state
                .last_error
                .as_ref()
                .is_some_and(|e| e.starts_with("Validation error:"))
            {
                state.last_error = None;
            }
        }

        state.last_iteration_at = Some(chrono::Utc::now());
        state.save(wt_path)?;

        // Check completion (idle detection)
        let current_commit = get_commit_hash(wt_path).await;
        let is_complete = detector.check_completion(current_commit.as_deref());

        state.last_commit = detector.last_commit().map(String::from);
        state.idle_iterations = detector.idle_count();

        if is_complete {
            state.active = false;
            state.save(wt_path)?;
            break;
        }

        // Git push if enabled
        if config.git.auto_push {
            if let Err(e) = git_push(wt_path, &config.git.protected_branches).await {
                warn!("Git push failed in worktree: {}", e);
            }
        }

        state.iteration += 1;
        state.save(wt_path)?;
    }

    Ok(state.iteration)
}

/// Format a summary of branch build results.
fn format_branch_summary(results: &[BranchResult]) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    writeln!(
        out,
        "\n═══════════════════════════════════════════════════════════════"
    )
    .unwrap();
    writeln!(out, "  Branch Build Summary").unwrap();
    writeln!(
        out,
        "═══════════════════════════════════════════════════════════════"
    )
    .unwrap();

    let succeeded: Vec<_> = results.iter().filter(|r| r.success).collect();
    let failed: Vec<_> = results.iter().filter(|r| !r.success).collect();

    writeln!(
        out,
        "\n  ✅ Succeeded: {}  ❌ Failed: {}",
        succeeded.len(),
        failed.len()
    )
    .unwrap();

    if !succeeded.is_empty() {
        writeln!(out, "\n  Successful branches:").unwrap();
        for r in &succeeded {
            let pr_info = r
                .pr_url
                .as_ref()
                .map(|url| format!(" → {url}"))
                .unwrap_or_default();
            writeln!(
                out,
                "    ✓ {} ({} iterations){pr_info}",
                r.branch, r.iterations
            )
            .unwrap();
        }
    }

    if !failed.is_empty() {
        writeln!(out, "\n  Failed branches:").unwrap();
        for r in &failed {
            let error = r.error.as_deref().unwrap_or("Unknown error");
            writeln!(out, "    ✗ {}: {}", r.branch, error).unwrap();
        }
    }

    writeln!(
        out,
        "\n═══════════════════════════════════════════════════════════════"
    )
    .unwrap();
    out
}

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the main Ralph loop with the specified configuration.
#[allow(tail_expr_drop_order, clippy::too_many_lines)] // Drop order doesn't matter for async operations
pub(crate) async fn run(
    mode: LoopMode,
    max_iterations: Option<u32>,
    no_sandbox: bool,
    custom_prompt: Option<String>,
    provider_override: Option<String>,
    sequential: bool,
) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Load configuration
    let config = Config::load(&cwd).context("Failed to load ralph.toml")?;

    // Check for branch build mode: build mode + IMPLEMENTATION_PLAN.md with branches
    if mode == LoopMode::Build && custom_prompt.is_none() {
        let plan_path = cwd.join("IMPLEMENTATION_PLAN.md");
        if plan_path.exists() {
            let plan_content = std::fs::read_to_string(&plan_path)
                .context("Failed to read IMPLEMENTATION_PLAN.md")?;
            let branches = parse_implementation_plan(&plan_content);

            // Filter to only incomplete branches (those with unchecked tasks)
            let incomplete_branches: Vec<_> = branches
                .into_iter()
                .filter(|b| {
                    // Check if branch section has unchecked tasks
                    // A branch is complete if all its tasks are checked [x]
                    is_branch_incomplete(&plan_content, &b.name)
                })
                .collect();

            if !incomplete_branches.is_empty() {
                info!(
                    "Found {} incomplete branches in IMPLEMENTATION_PLAN.md",
                    incomplete_branches.len()
                );
                let mode_str = if sequential { "sequential" } else { "parallel" };
                println!(
                    "Building {} branches in {} mode...\n",
                    incomplete_branches.len(),
                    mode_str
                );

                let results = execute_branch_builds(
                    incomplete_branches,
                    &config,
                    max_iterations,
                    no_sandbox,
                    provider_override.as_deref(),
                    sequential,
                )
                .await?;

                print!("{}", format_branch_summary(&results));

                // Return error if any branch failed
                let failed_count = results.iter().filter(|r| !r.success).count();
                if failed_count > 0 {
                    bail!("{failed_count} branch(es) failed");
                }

                return Ok(());
            }
        }
    }

    // Determine prompt file
    let prompt_file = determine_prompt_file(&cwd, mode, custom_prompt.as_deref());

    if !prompt_file.exists() {
        bail!(
            "Prompt file not found: {}\nRun 'ralph init' to create default files.",
            prompt_file.display()
        );
    }

    // Load or create state
    let state = RalphState::load_or_create(&cwd, mode.into())?;
    let mut state = prepare_state(state, max_iterations);
    state.save(&cwd)?;

    // Get agent provider: CLI override takes precedence over config
    let provider = resolve_provider(&config, provider_override.as_deref())?;

    // Print startup banner
    let banner = BannerInfo::new(&state, &prompt_file, no_sandbox, &config, provider);
    print!("{}", format_banner(&banner));

    // Create the agent provider (for non-sandbox mode)
    let agent: Box<dyn AgentProvider> = match provider {
        Provider::Cursor => Box::new(CursorProvider::new(config.agent.cursor.clone())),
        Provider::Claude => Box::new(ClaudeProvider::new(config.agent.claude.clone())),
    };

    // Create sandbox if enabled
    let sandbox: Option<Box<dyn Sandbox>> = if banner.sandbox_enabled {
        Some(Box::new(DockerSandbox::new(
            config.clone(),
            provider,
            config.agent.clone(),
        )))
    } else {
        None
    };

    // Clean up orphaned containers if sandbox is enabled
    if let Some(ref sb) = sandbox {
        if let Err(e) = sb.cleanup_orphaned().await {
            warn!(
                "Failed to cleanup orphaned containers: {}. Continuing anyway.",
                e
            );
        }
    }

    // Create persistent container if reuse is enabled
    let persistent_container_name = if banner.sandbox_enabled && config.sandbox.reuse_container {
        match sandbox.as_ref() {
            Some(sb) => match sb.create_persistent(&cwd).await {
                Ok(name) if !name.is_empty() => {
                    info!("Created persistent container: {}", name);
                    Some(name)
                }
                Ok(_) => None, // Empty string means no persistence support
                Err(e) => {
                    warn!(
                        "Failed to create persistent container: {}. Falling back to per-iteration containers.",
                        e
                    );
                    None
                }
            },
            None => None,
        }
    } else {
        None
    };

    // Initialize completion detector from persisted state for idle detection
    // continuity across restarts
    let mut detector = CompletionDetector::from_state(
        config.completion.idle_threshold,
        state.last_commit.clone(),
        state.idle_iterations,
    );

    // Initialize notifier
    let notifier = Notifier::new(config.monitoring.notifications.clone());

    // Log loop start
    tracing::info!(
        event = "loop_start",
        mode = ?state.mode,
        provider = %provider,
        max_iterations = state.max_iterations,
    );

    // Main loop
    loop {
        // Check for external cancellation (e.g., `ralph cancel`)
        if let Some(loaded) = RalphState::load(&cwd)? {
            if !loaded.active {
                info!("Loop cancelled externally");
                state.active = false;
                state.save(&cwd)?;

                tracing::info!(
                    event = "loop_end",
                    total_iterations = state.iteration,
                    reason = "cancelled",
                );

                let details =
                    NotificationDetails::complete(state.iteration, state.iteration, "cancelled");
                notifier.notify(NotificationEvent::Complete, &details).await;

                break;
            }
        }

        // Check max iterations
        if is_max_iterations_reached(&state) {
            println!(
                "{}",
                format_max_iterations_reached(state.max_iterations.unwrap())
            );
            state.active = false;
            state.save(&cwd)?;

            // Log loop end
            tracing::info!(
                event = "loop_end",
                total_iterations = state.iteration,
                reason = "max_iterations_reached",
            );

            // Send completion notification
            let details = NotificationDetails::complete(
                state.iteration,
                state.iteration,
                "max_iterations_reached",
            );
            notifier.notify(NotificationEvent::Complete, &details).await;

            break;
        }

        println!("{}", format_iteration_header(state.iteration));

        // Log iteration start
        tracing::info!(event = "iteration_start", iteration = state.iteration,);

        // Record commit hash at start of iteration (for idle detection)
        let start_commit = get_commit_hash(&cwd).await;
        detector.record_commit(start_commit);

        // Read prompt
        let mut prompt = std::fs::read_to_string(&prompt_file)
            .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))?;

        // Append validation errors from previous iteration if present
        if let Some(ref last_error) = state.last_error {
            if last_error.starts_with("Validation error:") {
                debug!("Appending validation error to prompt for agent visibility");

                let error_details = last_error
                    .strip_prefix("Validation error:")
                    .unwrap_or(last_error);

                prompt.push_str("\n\n");
                prompt.push_str("## ⚠️ VALIDATION ERROR FROM PREVIOUS ITERATION\n");
                prompt.push_str("The following validation error occurred. Please fix it:\n\n");
                prompt.push_str("```\n");
                prompt.push_str(error_details.trim());
                prompt.push_str("\n```\n");
                prompt.push_str(
                    "\nFix the issues above and ensure validation passes before proceeding.\n",
                );
            }
        }

        // Run agent (in sandbox if enabled, otherwise directly)
        info!(
            "Running {} agent iteration {}",
            agent.name(),
            state.iteration
        );
        let output_result = if let Some(ref sb) = sandbox {
            sb.run(&cwd, &prompt, persistent_container_name.as_deref())
                .await
        } else {
            // Non-sandbox mode: apply timeout (provider-specific > global)
            let timeout_mins = resolve_timeout(&config, provider);
            let timeout_duration = std::time::Duration::from_secs(u64::from(timeout_mins) * 60);
            tokio::time::timeout(timeout_duration, agent.invoke(&cwd, &prompt))
                .await
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Agent execution timed out after {timeout_mins} minutes"
                    ))
                })
        };

        // Handle agent execution result (including timeouts)
        let _output = match output_result {
            Ok(out) => out,
            Err(e) => {
                let error_msg = e.to_string();

                // Check if this is a recoverable error (timeout, rate limit, etc.)
                // Use typed error checking for sandbox errors
                let is_timeout = e
                    .downcast_ref::<SandboxError>()
                    .is_some_and(SandboxError::is_timeout)
                    || error_msg.contains("timed out"); // Fallback for non-sandbox timeouts
                let is_rate_limit = error_msg.contains("resource_exhausted")
                    || error_msg.contains("rate limit")
                    || error_msg.contains("Rate limit")
                    || error_msg.contains("429")
                    || error_msg.contains("quota")
                    || error_msg.contains("Quota");

                // Log error
                let error_context = serde_json::json!({
                    "iteration": state.iteration,
                    "provider": provider.to_string(),
                    "timeout": is_timeout,
                    "rate_limit": is_rate_limit,
                });
                tracing::error!(
                    event = "error",
                    iteration = state.iteration,
                    error = %e,
                    ?error_context,
                );

                // Send error notification
                let error_details = NotificationDetails::error(
                    Some(state.iteration),
                    &error_msg,
                    Some(error_context),
                );
                notifier
                    .notify(NotificationEvent::Error, &error_details)
                    .await;

                // For recoverable errors (timeout, rate limit), continue to next iteration
                if is_timeout || is_rate_limit {
                    let error_type = if is_rate_limit {
                        "rate limit"
                    } else {
                        "timeout"
                    };

                    // Check if this is a consecutive rate limit error (likely hard cap)
                    let consecutive_rate_limits = if is_rate_limit {
                        // Count consecutive rate limit errors in recent iterations
                        // Check if last error was also a rate limit
                        state.last_error.as_ref().is_some_and(|e| {
                            e.contains("rate limit") || e.contains("resource_exhausted")
                        })
                    } else {
                        false
                    };

                    if is_rate_limit {
                        if consecutive_rate_limits {
                            // Likely hit a hard cap (daily/hourly quota)
                            // Use exponential backoff: 30s, 1m, 2m, 5m, 10m
                            let backoff_seconds = match state.consecutive_errors {
                                0..=1 => 30,
                                2 => 60,
                                3 => 120,
                                4 => 300,
                                _ => 600, // 10 minutes for 5+ consecutive errors
                            };

                            warn!(
                                "Rate limit error (likely daily/hourly quota). Waiting {} seconds before retry...",
                                backoff_seconds
                            );
                            warn!(
                                "If this persists, you may have hit a hard quota limit. Consider:\n\
                                 - Waiting several hours before retrying\n\
                                 - Switching to Claude provider: ralph loop build --provider claude\n\
                                 - Reducing iteration frequency"
                            );

                            tokio::time::sleep(std::time::Duration::from_secs(backoff_seconds))
                                .await;
                        } else {
                            // First rate limit error - short delay
                            info!(
                                "Waiting 30 seconds before retry to allow rate limit to reset..."
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        }
                    }

                    state.error_count += 1;
                    state.consecutive_errors += 1;
                    state.last_error = Some(format!("Agent {error_type}: {error_msg}"));
                    state.last_iteration_at = Some(chrono::Utc::now());
                    state.iteration += 1;
                    state.save(&cwd)?;

                    // Circuit breaker: stop if too many consecutive errors
                    if config.monitoring.max_consecutive_errors > 0
                        && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                    {
                        // Clean up persistent container before bailing
                        if let (Some(container_name), Some(sb)) =
                            (&persistent_container_name, &sandbox)
                        {
                            let _ = sb.remove_persistent(container_name).await;
                        }
                        bail!(
                            "Circuit breaker triggered: {} consecutive errors (limit: {}). \
                             Increase monitoring.max_consecutive_errors in ralph.toml to continue.",
                            state.consecutive_errors,
                            config.monitoring.max_consecutive_errors
                        );
                    }

                    // Show progress if enabled
                    if config.monitoring.show_progress {
                        let progress = ProgressInfo::new(&state, &cwd).await;
                        print!("{}", format_progress(&progress));
                    }

                    // Continue to next iteration
                    continue;
                }

                // For other errors, fail the loop (but cleanup container first)
                if let (Some(container_name), Some(sb)) = (&persistent_container_name, &sandbox) {
                    let _ = sb.remove_persistent(container_name).await;
                }
                return Err(e).context("Agent execution failed");
            }
        };

        // Validate code compiles before proceeding (if enabled)
        if config.validation.enabled {
            match validate_code(&cwd, &config.validation.command).await {
                Ok(()) => {
                    // Clear validation error if validation now passes (agent fixed it)
                    if let Some(ref last_error) = state.last_error {
                        if last_error.starts_with("Validation error:") {
                            debug!("Validation passed - clearing previous validation error");
                            state.last_error = None;
                        }
                    }
                }
                Err(full_error) => {
                    warn!("Code validation failed. Agent should fix this in next iteration.");

                    // Truncate for logging/notifications (full error goes in state)
                    let error_summary: String =
                        full_error.lines().take(5).collect::<Vec<_>>().join("\n");

                    // Store full error in state for next iteration's prompt
                    state.error_count += 1;
                    state.consecutive_errors += 1;
                    state.last_error = Some(format!("Validation error:{full_error}"));
                    state.last_iteration_at = Some(chrono::Utc::now());
                    state.iteration += 1;
                    state.save(&cwd)?;

                    // Log validation error
                    let validation_error_context = serde_json::json!({
                        "iteration": state.iteration - 1,
                        "error": error_summary.clone(),
                    });
                    tracing::error!(
                        event = "error",
                        iteration = state.iteration - 1,
                        error = %format!("Code validation failed"),
                        ?validation_error_context,
                    );

                    // Send error notification
                    let error_details = NotificationDetails::error(
                        Some(state.iteration - 1),
                        &format!("Code validation failed: {error_summary}"),
                        Some(validation_error_context),
                    );
                    notifier
                        .notify(NotificationEvent::Error, &error_details)
                        .await;

                    // Circuit breaker: stop if too many consecutive errors
                    if config.monitoring.max_consecutive_errors > 0
                        && state.consecutive_errors >= config.monitoring.max_consecutive_errors
                    {
                        // Clean up persistent container before bailing
                        if let (Some(container_name), Some(sb)) =
                            (&persistent_container_name, &sandbox)
                        {
                            let _ = sb.remove_persistent(container_name).await;
                        }
                        bail!(
                            "Circuit breaker triggered: {} consecutive validation errors (limit: {}). \
                             Increase monitoring.max_consecutive_errors in ralph.toml to continue.",
                            state.consecutive_errors,
                            config.monitoring.max_consecutive_errors
                        );
                    }

                    // Continue to next iteration (let agent fix it)
                    if config.monitoring.show_progress {
                        let progress = ProgressInfo::new(&state, &cwd).await;
                        print!("{}", format_progress(&progress));
                    }
                    continue;
                }
            }
        }

        // Successful iteration - reset consecutive errors counter
        state.consecutive_errors = 0;
        state.last_iteration_at = Some(chrono::Utc::now());

        // Check for cancellation again (loop may have been cancelled during agent execution)
        if let Some(loaded) = RalphState::load(&cwd)? {
            if !loaded.active {
                info!("Loop cancelled externally during iteration");
                state.active = false;
                state.save(&cwd)?;

                tracing::info!(
                    event = "loop_end",
                    total_iterations = state.iteration,
                    reason = "cancelled",
                );

                let details =
                    NotificationDetails::complete(state.iteration, state.iteration, "cancelled");
                notifier.notify(NotificationEvent::Complete, &details).await;

                break;
            }
        }

        state.save(&cwd)?;

        // Get commit hash after agent execution (may have created commits)
        let current_commit = get_commit_hash(&cwd).await;

        // Check for completion: validation passed + agent idle (no new commits)
        // check_completion updates detector's internal state (last_commit, idle_count)
        let is_complete = detector.check_completion(current_commit.as_deref());

        // Sync detector state to RalphState for persistence across restarts
        state.last_commit = detector.last_commit().map(String::from);
        state.idle_iterations = detector.idle_count();

        if is_complete {
            println!("{}", format_completion_detected(detector.idle_count()));
            state.active = false;
            state.save(&cwd)?;

            // Log loop end
            tracing::info!(
                event = "loop_end",
                total_iterations = state.iteration,
                reason = "agent_idle",
                idle_iterations = detector.idle_count(),
            );

            // Send completion notification
            let details =
                NotificationDetails::complete(state.iteration, state.iteration, "agent_idle");
            notifier.notify(NotificationEvent::Complete, &details).await;

            break;
        }

        let commit_hash = current_commit;

        // Git operations
        if config.git.auto_push {
            if let Err(e) = git_push(&cwd, &config.git.protected_branches).await {
                warn!("Git push failed: {e}");
                state.error_count += 1;
                // Note: Git push failures don't increment consecutive_errors because
                // the iteration itself succeeded. The agent produced valid code.
                state.last_error = Some(format!("Git push failed: {e}"));
                state.save(&cwd)?;
                // Log git push error
                let git_error_context = serde_json::json!({
                    "iteration": state.iteration,
                });
                tracing::error!(
                    event = "error",
                    iteration = state.iteration,
                    error = %format!("Git push failed: {e}"),
                    ?git_error_context,
                );

                // Send error notification for git push failure
                let error_details = NotificationDetails::error(
                    Some(state.iteration),
                    &format!("Git push failed: {e}"),
                    Some(git_error_context),
                );
                notifier
                    .notify(NotificationEvent::Error, &error_details)
                    .await;
            }
        }

        // Log iteration complete
        tracing::info!(
            event = "iteration_complete",
            iteration = state.iteration,
            commit = ?commit_hash,
        );

        // Show progress display between iterations (if enabled)
        if config.monitoring.show_progress {
            let progress = ProgressInfo::new(&state, &cwd).await;
            print!("{}", format_progress(&progress));
        }

        // Increment iteration
        state.iteration += 1;
        state.save(&cwd)?;
    }

    // Log loop end if not already logged
    if state.active {
        tracing::info!(
            event = "loop_end",
            total_iterations = state.iteration,
            reason = "max_iterations_reached",
        );
    }

    // Clean up persistent container if it was created
    if let (Some(container_name), Some(sb)) = (persistent_container_name, &sandbox) {
        info!("Cleaning up persistent container: {}", container_name);
        if let Err(e) = sb.remove_persistent(&container_name).await {
            warn!(
                "Failed to remove persistent container {}: {}",
                container_name, e
            );
        }
    }

    print!("{}", format_loop_finished(state.iteration));

    Ok(())
}

// -----------------------------------------------------------------------------
// Public types
// -----------------------------------------------------------------------------

/// Loop execution mode for the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum LoopMode {
    /// Planning mode - generates implementation plans.
    Plan,
    /// Build mode - implements features.
    Build,
}

impl From<LoopMode> for Mode {
    fn from(mode: LoopMode) -> Self {
        match mode {
            LoopMode::Plan => Self::Plan,
            LoopMode::Build => Self::Build,
        }
    }
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Determines the prompt file path based on mode and custom override.
fn determine_prompt_file(cwd: &Path, mode: LoopMode, custom_prompt: Option<&str>) -> PathBuf {
    match custom_prompt {
        Some(p) => PathBuf::from(p),
        None => match mode {
            LoopMode::Plan => cwd.join("PROMPT_plan.md"),
            LoopMode::Build => cwd.join("PROMPT_build.md"),
        },
    }
}

/// Prepares state with CLI options.
fn prepare_state(mut state: RalphState, max_iterations: Option<u32>) -> RalphState {
    state.max_iterations = max_iterations;
    state.active = true;
    state
}

/// Checks if max iterations has been reached.
fn is_max_iterations_reached(state: &RalphState) -> bool {
    state
        .max_iterations
        .is_some_and(|max| state.iteration > max)
}

/// Resolves the agent provider to use.
/// Priority: CLI flag > `RALPH_PROVIDER` env var > config file.
fn resolve_provider(config: &Config, provider_override: Option<&str>) -> Result<Provider> {
    let env_provider = std::env::var("RALPH_PROVIDER").ok();
    resolve_provider_with_env(config, provider_override, env_provider.as_deref())
}

/// Resolves the timeout for the given provider.
/// Priority: provider-specific timeout > global sandbox timeout.
fn resolve_timeout(config: &Config, provider: Provider) -> u32 {
    config
        .agent
        .get_provider_timeout(provider)
        .unwrap_or(config.sandbox.resources.timeout_minutes)
}

/// Internal helper for provider resolution with explicit env var value.
/// Enables testing without modifying actual environment.
fn resolve_provider_with_env(
    config: &Config,
    provider_override: Option<&str>,
    env_provider: Option<&str>,
) -> Result<Provider> {
    // 1. CLI flag takes highest precedence
    if let Some(p) = provider_override {
        debug!("Using CLI provider override: {}", p);
        return p.parse();
    }

    // 2. RALPH_PROVIDER env var takes precedence over config
    if let Some(env_val) = env_provider {
        if !env_val.is_empty() {
            debug!("Using RALPH_PROVIDER env var: {}", env_val);
            return env_val.parse();
        }
    }

    // 3. Fall back to config file
    config.agent.get_provider()
}

// -----------------------------------------------------------------------------
// Validation
// -----------------------------------------------------------------------------

/// Validates code by running the configured validation command.
/// Returns the full error message if validation fails.
async fn validate_code(cwd: &Path, command: &str) -> Result<(), String> {
    debug!("Validating code with command: {}", command);

    // Parse command using shell-words to handle quoted arguments properly
    // e.g., `sh -c "cmd1 && cmd2"` becomes ["sh", "-c", "cmd1 && cmd2"]
    let parts = shell_words::split(command)
        .map_err(|e| format!("Failed to parse validation command: {e}"))?;

    let (program, args) = parts
        .split_first()
        .ok_or_else(|| "Validation command cannot be empty".to_string())?;

    let output = tokio::process::Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run validation command: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let error_msg = if stderr.is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        // Return full error message (not truncated)
        let full_error = format!("Validation failed ({command}):\n{error_msg}");
        return Err(full_error);
    }

    info!("Code validation passed: {}", command);
    Ok(())
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_state(iteration: u32, max: Option<u32>) -> RalphState {
        RalphState {
            active: false,
            mode: Mode::Build,
            iteration,
            max_iterations: max,
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
            last_commit: None,
            idle_iterations: 0,
        }
    }

    #[test]
    fn test_loop_mode_conversion() {
        assert_eq!(Mode::from(LoopMode::Plan), Mode::Plan);
        assert_eq!(Mode::from(LoopMode::Build), Mode::Build);
    }

    #[test]
    fn test_determine_prompt_file_default_plan() {
        let cwd = PathBuf::from("/project");
        let path = determine_prompt_file(&cwd, LoopMode::Plan, None);
        assert_eq!(path, PathBuf::from("/project/PROMPT_plan.md"));
    }

    #[test]
    fn test_determine_prompt_file_default_build() {
        let cwd = PathBuf::from("/project");
        let path = determine_prompt_file(&cwd, LoopMode::Build, None);
        assert_eq!(path, PathBuf::from("/project/PROMPT_build.md"));
    }

    #[test]
    fn test_determine_prompt_file_custom() {
        let cwd = PathBuf::from("/project");
        let path = determine_prompt_file(&cwd, LoopMode::Build, Some("/custom/prompt.md"));
        assert_eq!(path, PathBuf::from("/custom/prompt.md"));
    }

    #[test]
    fn test_prepare_state_with_max() {
        let state = make_state(1, None);
        let prepared = prepare_state(state, Some(10));

        assert!(prepared.active);
        assert_eq!(prepared.max_iterations, Some(10));
    }

    #[test]
    fn test_prepare_state_unlimited() {
        let state = make_state(1, Some(5));
        let prepared = prepare_state(state, None);

        assert!(prepared.active);
        assert_eq!(prepared.max_iterations, None);
    }

    #[test]
    fn test_is_max_iterations_reached_under() {
        let state = make_state(3, Some(10));
        assert!(!is_max_iterations_reached(&state));
    }

    #[test]
    fn test_is_max_iterations_reached_at() {
        let state = make_state(10, Some(10));
        assert!(!is_max_iterations_reached(&state));
    }

    #[test]
    fn test_is_max_iterations_reached_over() {
        let state = make_state(11, Some(10));
        assert!(is_max_iterations_reached(&state));
    }

    #[test]
    fn test_is_max_iterations_reached_unlimited() {
        let state = make_state(1000, None);
        assert!(!is_max_iterations_reached(&state));
    }

    #[test]
    fn test_resolve_provider_config_default() {
        let config = Config::default();
        let provider = resolve_provider(&config, None).unwrap();
        assert_eq!(provider, Provider::Cursor);
    }

    #[test]
    fn test_resolve_provider_cli_override_claude() {
        let config = Config::default();
        let provider = resolve_provider(&config, Some("claude")).unwrap();
        assert_eq!(provider, Provider::Claude);
    }

    #[test]
    fn test_resolve_provider_cli_override_cursor() {
        // Config set to claude but CLI overrides to cursor
        let mut config = Config::default();
        config.agent.provider = "claude".to_string();
        let provider = resolve_provider(&config, Some("cursor")).unwrap();
        assert_eq!(provider, Provider::Cursor);
    }

    #[test]
    fn test_resolve_provider_invalid() {
        let config = Config::default();
        let result = resolve_provider(&config, Some("invalid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_provider_env_var_overrides_config() {
        let config = Config::default(); // defaults to cursor
        let provider = resolve_provider_with_env(&config, None, Some("claude")).unwrap();
        assert_eq!(provider, Provider::Claude);
    }

    #[test]
    fn test_resolve_provider_cli_overrides_env_var() {
        let config = Config::default();
        // CLI flag "cursor" should win over env var "claude"
        let provider = resolve_provider_with_env(&config, Some("cursor"), Some("claude")).unwrap();
        assert_eq!(provider, Provider::Cursor);
    }

    #[test]
    fn test_resolve_provider_empty_env_var_falls_back() {
        let config = Config::default(); // defaults to cursor
                                        // Empty env var should fall back to config
        let provider = resolve_provider_with_env(&config, None, Some("")).unwrap();
        assert_eq!(provider, Provider::Cursor);
    }

    #[test]
    fn test_resolve_provider_none_env_var_falls_back() {
        let config = Config::default(); // defaults to cursor
                                        // None env var should fall back to config
        let provider = resolve_provider_with_env(&config, None, None).unwrap();
        assert_eq!(provider, Provider::Cursor);
    }

    #[test]
    fn test_resolve_provider_invalid_env_var() {
        let config = Config::default();
        let result = resolve_provider_with_env(&config, None, Some("invalid_provider"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_code_simple_command() {
        // Simple command without quotes should work
        let cwd = std::env::current_dir().unwrap();
        let result = validate_code(&cwd, "true").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_code_quoted_args() {
        // Quoted arguments should be parsed correctly
        // sh -c "echo hello" should be parsed as ["sh", "-c", "echo hello"]
        let cwd = std::env::current_dir().unwrap();
        let result = validate_code(&cwd, "sh -c \"exit 0\"").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_code_quoted_args_complex() {
        // Complex quoted arguments with && should work
        // This was broken with split_whitespace()
        let cwd = std::env::current_dir().unwrap();
        let result = validate_code(&cwd, "sh -c \"true && true\"").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_code_empty_command() {
        let cwd = std::env::current_dir().unwrap();
        let result = validate_code(&cwd, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_validate_code_unmatched_quote() {
        // Unmatched quote should fail parsing
        let cwd = std::env::current_dir().unwrap();
        let result = validate_code(&cwd, "sh -c \"unclosed").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parse"));
    }

    #[test]
    fn test_resolve_timeout_uses_provider_specific() {
        // Provider timeout should override global
        let toml = r"
[agent.cursor]
timeout_minutes = 120

[sandbox.resources]
timeout_minutes = 60
";
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(resolve_timeout(&config, Provider::Cursor), 120);
    }

    #[test]
    fn test_resolve_timeout_falls_back_to_global() {
        // No provider timeout - should use global
        let toml = r"
[sandbox.resources]
timeout_minutes = 45
";
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(resolve_timeout(&config, Provider::Cursor), 45);
        assert_eq!(resolve_timeout(&config, Provider::Claude), 45);
    }

    #[test]
    fn test_resolve_timeout_different_providers() {
        // Different timeouts for different providers
        let toml = r"
[agent.cursor]
timeout_minutes = 30

[agent.claude]
timeout_minutes = 180

[sandbox.resources]
timeout_minutes = 60
";
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(resolve_timeout(&config, Provider::Cursor), 30);
        assert_eq!(resolve_timeout(&config, Provider::Claude), 180);
    }

    #[test]
    fn test_resolve_timeout_default_config() {
        // Default config should use sandbox.resources.timeout_minutes (60)
        let config = Config::default();
        assert_eq!(resolve_timeout(&config, Provider::Cursor), 60);
        assert_eq!(resolve_timeout(&config, Provider::Claude), 60);
    }

    // -------------------------------------------------------------------------
    // Branch Build Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_branch_incomplete_with_unchecked_tasks() {
        let plan = r"
## Branch: feature-a
Goal: Add feature A
Base: master

- [ ] Task 1
- [ ] Task 2
";
        assert!(is_branch_incomplete(plan, "feature-a"));
    }

    #[test]
    fn test_is_branch_incomplete_with_all_checked_tasks() {
        let plan = r"
## Branch: feature-a
Goal: Add feature A
Base: master

- [x] Task 1
- [x] Task 2
";
        assert!(!is_branch_incomplete(plan, "feature-a"));
    }

    #[test]
    fn test_is_branch_incomplete_with_mixed_tasks() {
        let plan = r"
## Branch: feature-a
Goal: Add feature A
Base: master

- [x] Task 1
- [ ] Task 2
";
        assert!(is_branch_incomplete(plan, "feature-a"));
    }

    #[test]
    fn test_is_branch_incomplete_branch_not_found() {
        let plan = r"
## Branch: other-branch
Goal: Other
Base: master
";
        assert!(!is_branch_incomplete(plan, "feature-a"));
    }

    #[test]
    fn test_is_branch_incomplete_scoped_to_branch() {
        // Ensure we only check tasks within the branch's section
        let plan = r"
## Branch: feature-a
Goal: Add feature A
Base: master

- [x] Task 1
- [x] Task 2

## Branch: feature-b
Goal: Add feature B
Base: master

- [ ] Task 3
";
        // feature-a should be complete (all tasks checked)
        assert!(!is_branch_incomplete(plan, "feature-a"));
        // feature-b should be incomplete
        assert!(is_branch_incomplete(plan, "feature-b"));
    }

    #[test]
    fn test_branch_result_success() {
        let result = BranchResult::success(
            "test-branch",
            5,
            Some("http://example.com/pr/1".to_string()),
        );
        assert!(result.success);
        assert_eq!(result.branch, "test-branch");
        assert_eq!(result.iterations, 5);
        assert!(result.error.is_none());
        assert_eq!(result.pr_url, Some("http://example.com/pr/1".to_string()));
    }

    #[test]
    fn test_branch_result_failure() {
        let result = BranchResult::failure("test-branch", 3, "Something went wrong".to_string());
        assert!(!result.success);
        assert_eq!(result.branch, "test-branch");
        assert_eq!(result.iterations, 3);
        assert_eq!(result.error, Some("Something went wrong".to_string()));
        assert!(result.pr_url.is_none());
    }

    #[test]
    fn test_format_branch_summary_all_success() {
        let results = vec![
            BranchResult::success("branch-a", 5, Some("http://pr/1".to_string())),
            BranchResult::success("branch-b", 3, None),
        ];
        let summary = format_branch_summary(&results);
        assert!(summary.contains("Succeeded: 2"));
        assert!(summary.contains("Failed: 0"));
        assert!(summary.contains("branch-a"));
        assert!(summary.contains("branch-b"));
        assert!(summary.contains("http://pr/1"));
    }

    #[test]
    fn test_format_branch_summary_mixed() {
        let results = vec![
            BranchResult::success("branch-a", 5, None),
            BranchResult::failure("branch-b", 3, "Build failed".to_string()),
        ];
        let summary = format_branch_summary(&results);
        assert!(summary.contains("Succeeded: 1"));
        assert!(summary.contains("Failed: 1"));
        assert!(summary.contains("branch-a"));
        assert!(summary.contains("branch-b"));
        assert!(summary.contains("Build failed"));
    }

    // -------------------------------------------------------------------------
    // E2E Loop Tests
    // -------------------------------------------------------------------------

    mod e2e {
        use super::*;
        use crate::agent::mock::{MockAgentProvider, MockResponse};
        use crate::sandbox::NoopSandbox;
        use tempfile::tempdir;

        /// Create a test project directory with required files.
        fn setup_test_project(prompt_content: &str) -> (tempfile::TempDir, PathBuf) {
            let dir = tempdir().unwrap();
            let project_dir = dir.path().to_path_buf();

            // Create prompt file
            let prompt_file = project_dir.join("PROMPT_build.md");
            std::fs::write(&prompt_file, prompt_content).unwrap();

            (dir, project_dir)
        }

        /// Create a minimal config for testing.
        fn test_config() -> Config {
            let mut config = Config::default();
            config.validation.enabled = false; // Disable validation for basic tests
            config.sandbox.enabled = false;
            config.git.auto_push = false;
            config.monitoring.show_progress = false;
            config
        }

        /// Create initial state for testing.
        fn test_state(max_iterations: Option<u32>) -> RalphState {
            RalphState {
                active: false,
                mode: Mode::Build,
                iteration: 1,
                max_iterations,
                started_at: Utc::now(),
                last_iteration_at: None,
                error_count: 0,
                consecutive_errors: 0,
                last_error: None,
                last_commit: None,
                idle_iterations: 0,
            }
        }

        #[tokio::test]
        async fn test_e2e_loop_max_iterations() {
            // Test: Loop stops after max iterations (before idle detection triggers)
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            let agent = MockAgentProvider::always_succeed("Agent output");

            // Use high idle_threshold so max_iterations triggers first
            let mut config = test_config();
            config.completion.idle_threshold = 10;

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(3)); // Max 3 iterations

            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(result.termination_reason, TerminationReason::MaxIterations);
            assert_eq!(result.final_iteration, 4); // Stopped after iteration > max (4 > 3)
            assert_eq!(result.error_count, 0);
            assert_eq!(agent.invocation_count(), 3); // Ran exactly 3 times
        }

        #[tokio::test]
        async fn test_e2e_loop_idle_detection() {
            // Test: Loop stops when agent is idle (no commits) for idle_threshold iterations
            // Default idle_threshold is 2, so after 2 idle iterations, loop should complete
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // Agent produces output but no commits (test has no git repo)
            let agent = MockAgentProvider::always_succeed("Working...");

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config: test_config(),
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(10)); // High max to ensure idle detection triggers first

            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(
                result.termination_reason,
                TerminationReason::CompletionDetected
            );
            // With idle_threshold=2, completes after 2 idle iterations
            assert_eq!(result.final_iteration, 2);
            assert_eq!(agent.invocation_count(), 2);
        }

        #[tokio::test]
        async fn test_e2e_loop_error_recovery() {
            // Test: Loop continues after recoverable errors (timeout/rate limit)
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // Agent: timeout, then two successes (need 2 for idle detection)
            let agent = MockAgentProvider::new(vec![
                MockResponse::Timeout,
                MockResponse::Success("Working...".to_string()),
                MockResponse::Success("Still working...".to_string()),
            ]);

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config: test_config(),
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(10));

            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(
                result.termination_reason,
                TerminationReason::CompletionDetected
            );
            assert_eq!(result.error_count, 1); // One timeout error
            assert_eq!(agent.invocation_count(), 3);
        }

        #[tokio::test]
        async fn test_e2e_loop_validation_error_recovery() {
            // Test: Validation errors are appended to prompt for next iteration
            let (_dir, project_dir) = setup_test_project("Initial prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // Track prompts received by the agent
            let agent = MockAgentProvider::new(vec![
                MockResponse::Success("Output 1".to_string()),
                MockResponse::Success("<promise>DONE</promise>".to_string()),
            ]);

            let mut config = test_config();
            config.validation.enabled = true;
            config.validation.command = "false".to_string(); // Always fails first time

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(5));

            // This will fail validation and increment iteration
            // Note: Since validation always fails, this tests the error accumulation
            let result = run_loop_core(deps, state).await;

            // The loop should continue but accumulate errors
            // Since validation always fails, it will hit max iterations
            assert!(result.is_ok());
            let result = result.unwrap();
            // Validation fails, so errors accumulate
            assert!(result.error_count > 0);
        }

        #[tokio::test]
        async fn test_e2e_loop_state_persistence() {
            // Test: State is persisted correctly across iterations
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            let agent = MockAgentProvider::always_succeed("Output");

            let deps = LoopDependencies {
                agent: Box::new(agent),
                sandbox: None,
                config: test_config(),
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(2));

            let _result = run_loop_core(deps, state).await.unwrap();

            // Verify state was saved
            let loaded_state = RalphState::load(&project_dir).unwrap().unwrap();
            assert!(!loaded_state.active); // Should be inactive after completion
            assert!(loaded_state.iteration > 1); // Should have advanced
        }

        #[tokio::test]
        async fn test_e2e_loop_with_noop_sandbox() {
            // Test: Loop works with NoopSandbox
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            let agent = MockAgentProvider::always_succeed("Sandbox output");
            let sandbox = NoopSandbox::new();

            let mut config = test_config();
            config.sandbox.enabled = true;
            config.sandbox.reuse_container = false;
            config.completion.idle_threshold = 10; // High threshold so max_iterations wins

            let deps = LoopDependencies {
                agent: Box::new(agent),
                sandbox: Some(Box::new(sandbox)),
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(2));

            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(result.termination_reason, TerminationReason::MaxIterations);
        }

        #[tokio::test]
        async fn test_e2e_loop_circuit_breaker() {
            // Test: Circuit breaker stops loop after max consecutive errors
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // Agent always times out
            let agent = MockAgentProvider::new(vec![MockResponse::Timeout]);

            let mut config = test_config();
            config.monitoring.max_consecutive_errors = 3;

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(100)); // High limit, circuit breaker should trigger first

            let result = run_loop_core(deps, state).await.unwrap();

            assert!(matches!(
                result.termination_reason,
                TerminationReason::Error(_)
            ));
            assert_eq!(result.error_count, 3); // Exactly 3 errors before circuit breaker
        }

        #[tokio::test]
        async fn test_e2e_loop_rate_limit_recovery() {
            // Test: Rate limit errors are recoverable
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // After rate limit, need 2 successful iterations for idle detection
            let agent = MockAgentProvider::new(vec![
                MockResponse::RateLimit,
                MockResponse::Success("Working...".to_string()),
                MockResponse::Success("Still working...".to_string()),
            ]);

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config: test_config(),
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(10));

            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(
                result.termination_reason,
                TerminationReason::CompletionDetected
            );
            assert_eq!(result.error_count, 1); // One rate limit error
        }

        #[tokio::test]
        async fn test_e2e_loop_external_cancellation() {
            // Test: Loop stops when state.active is set to false externally during execution
            let (_dir, project_dir) = setup_test_project("Test prompt");
            let prompt_file = project_dir.join("PROMPT_build.md");

            // Agent that cancels the loop after first invocation
            let agent = MockAgentProvider::new(vec![
                // First call succeeds but cancels the loop (simulates `ralph cancel`)
                MockResponse::SuccessAndCancel("Working...".to_string(), project_dir.clone()),
                // Second call would succeed, but loop should have stopped
                MockResponse::Success("Should not reach here".to_string()),
            ]);

            // High thresholds so cancellation is the only way to stop
            let mut config = test_config();
            config.completion.idle_threshold = 100;

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(100)); // High max iterations

            // Run the loop - first iteration runs, then cancellation is detected
            let result = run_loop_core(deps, state).await.unwrap();

            assert_eq!(result.termination_reason, TerminationReason::Cancelled);
            // Only one iteration should have run before cancellation was detected
            assert_eq!(agent.invocation_count(), 1);
        }

        #[tokio::test]
        async fn test_e2e_loop_plan_mode() {
            // Test: Plan mode uses correct prompt file and state mode
            let dir = tempdir().unwrap();
            let project_dir = dir.path().to_path_buf();

            // Create plan mode prompt file (not build mode)
            let prompt_file = project_dir.join("PROMPT_plan.md");
            std::fs::write(&prompt_file, "Plan mode prompt content").unwrap();

            let agent = MockAgentProvider::always_succeed("Generated plan output");

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config: test_config(),
                project_dir: project_dir.clone(),
                prompt_file,
            };

            // Create state in Plan mode (not Build)
            let state = RalphState {
                active: false,
                mode: Mode::Plan, // Plan mode
                iteration: 1,
                max_iterations: Some(2),
                started_at: Utc::now(),
                last_iteration_at: None,
                error_count: 0,
                consecutive_errors: 0,
                last_error: None,
                last_commit: None,
                idle_iterations: 0,
            };

            let result = run_loop_core(deps, state).await.unwrap();

            // Plan mode should complete via idle detection (no git commits in test)
            assert_eq!(
                result.termination_reason,
                TerminationReason::CompletionDetected
            );
            assert_eq!(agent.invocation_count(), 2);

            // Verify saved state reflects Plan mode
            let loaded = RalphState::load(&project_dir).unwrap().unwrap();
            assert_eq!(loaded.mode, Mode::Plan);
        }

        #[tokio::test]
        async fn test_e2e_loop_state_persistence_across_restart() {
            // Test: State persists correctly and loop can resume from saved state
            // Simulates restart by running loop partially, then loading state and running again
            let dir = tempdir().unwrap();
            let project_dir = dir.path().to_path_buf();
            let prompt_file = project_dir.join("PROMPT_build.md");
            std::fs::write(&prompt_file, "Test prompt").unwrap();

            // First run: 2 iterations, then stops due to max iterations
            let agent1 = MockAgentProvider::always_succeed("First run output");
            let mut config = test_config();
            config.completion.idle_threshold = 100; // High to prevent idle completion

            let deps1 = LoopDependencies {
                agent: Box::new(agent1.clone()),
                sandbox: None,
                config: config.clone(),
                project_dir: project_dir.clone(),
                prompt_file: prompt_file.clone(),
            };

            let state1 = RalphState {
                active: false,
                mode: Mode::Build,
                iteration: 1,
                max_iterations: Some(2),
                started_at: Utc::now(),
                last_iteration_at: None,
                error_count: 0,
                consecutive_errors: 0,
                last_error: None,
                last_commit: None,
                idle_iterations: 0,
            };

            let result1 = run_loop_core(deps1, state1).await.unwrap();
            assert_eq!(result1.termination_reason, TerminationReason::MaxIterations);
            assert_eq!(agent1.invocation_count(), 2);

            // Simulate restart: load state from disk
            let loaded_state = RalphState::load(&project_dir).unwrap().unwrap();
            assert!(!loaded_state.active); // Should be inactive after completion
            assert!(loaded_state.iteration > 1); // Should have advanced

            // Verify state fields persisted correctly
            assert_eq!(loaded_state.mode, Mode::Build);
            assert_eq!(loaded_state.error_count, 0);

            // Create new state for "resumed" session, starting from where we left off
            // max_iterations is iteration (not iteration + 1) so only 1 iteration runs
            // (loop runs while iteration <= max, so if both are 3, runs once then 4 > 3)
            let resumed_state = RalphState {
                active: false,
                mode: loaded_state.mode,
                iteration: loaded_state.iteration,
                max_iterations: Some(loaded_state.iteration), // Allow exactly 1 more iteration
                started_at: Utc::now(),
                last_iteration_at: None,
                error_count: loaded_state.error_count,
                consecutive_errors: 0, // Reset on restart
                last_error: None,
                last_commit: loaded_state.last_commit.clone(),
                idle_iterations: loaded_state.idle_iterations,
            };

            // Second run: continues from saved state
            let agent2 = MockAgentProvider::always_succeed("Second run output");
            let deps2 = LoopDependencies {
                agent: Box::new(agent2.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let result2 = run_loop_core(deps2, resumed_state).await.unwrap();

            // Should run 1 more iteration and hit max
            assert_eq!(result2.termination_reason, TerminationReason::MaxIterations);
            assert_eq!(agent2.invocation_count(), 1);
            // Final iteration should be original + 1
            assert_eq!(result2.final_iteration, result1.final_iteration + 1);
        }

        #[tokio::test]
        async fn test_e2e_loop_validation_error_in_prompt() {
            // Test: Validation errors are included in subsequent iteration prompts
            // This verifies the error feedback mechanism works
            let dir = tempdir().unwrap();
            let project_dir = dir.path().to_path_buf();

            let prompt_file = project_dir.join("PROMPT_build.md");
            std::fs::write(&prompt_file, "Initial prompt").unwrap();

            // Agent always succeeds
            let agent = MockAgentProvider::always_succeed("Output");

            let mut config = test_config();
            config.validation.enabled = true;
            // Use 'false' command which always exits non-zero
            // shell_words::split doesn't interpret shell operators like &&
            config.validation.command = "false".to_string();
            config.completion.idle_threshold = 100; // High to force max iterations

            let deps = LoopDependencies {
                agent: Box::new(agent.clone()),
                sandbox: None,
                config,
                project_dir: project_dir.clone(),
                prompt_file,
            };

            let state = test_state(Some(3));

            let result = run_loop_core(deps, state).await.unwrap();

            // Should hit max iterations (validation errors are recoverable)
            assert_eq!(result.termination_reason, TerminationReason::MaxIterations);
            // Validation failed each iteration
            assert_eq!(result.error_count, 3);
            assert_eq!(agent.invocation_count(), 3);
        }
    }
}
