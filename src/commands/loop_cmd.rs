//! Main Ralph loop command.
//!
//! This module runs the iterative AI development loop. Core logic
//! is separated into testable functions where possible.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use clap::ValueEnum;
use colored::Colorize;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::agent::{AgentProvider, ClaudeProvider, CursorProvider, Provider};
use crate::config::Config;
use crate::detection::CompletionDetector;
use crate::notifications::{NotificationDetails, NotificationEvent, Notifier};
use crate::sandbox::SandboxRunner;
use crate::state::{Mode, RalphState};

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the main Ralph loop with the specified configuration.
#[allow(tail_expr_drop_order, clippy::too_many_lines)] // Drop order doesn't matter for async operations
pub(crate) async fn run(
    mode: LoopMode,
    max_iterations: Option<u32>,
    completion_promise: Option<String>,
    no_sandbox: bool,
    custom_prompt: Option<String>,
    provider_override: Option<String>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Load configuration
    let config = Config::load(&cwd).context("Failed to load ralph.toml")?;

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
    let mut state = prepare_state(state, max_iterations, completion_promise.clone());
    state.save(&cwd)?;

    // Get agent provider: CLI override takes precedence over config
    let provider = resolve_provider(&config, provider_override.as_deref())?;

    // Print startup banner
    let banner = BannerInfo::new(&state, &prompt_file, no_sandbox, &config, provider);
    print!("{}", format_banner(&banner));

    // Clean up orphaned containers if sandbox is enabled
    if banner.sandbox_enabled {
        if let Err(e) = SandboxRunner::cleanup_orphaned_containers().await {
            warn!(
                "Failed to cleanup orphaned containers: {}. Continuing anyway.",
                e
            );
        }
    }

    // Create the agent provider (for non-sandbox mode)
    let agent: Box<dyn AgentProvider> = match provider {
        Provider::Cursor => Box::new(CursorProvider::new(config.agent.cursor.clone())),
        Provider::Claude => Box::new(ClaudeProvider::new(config.agent.claude.clone())),
    };

    // Create sandbox runner if sandbox is enabled
    let sandbox_runner = if banner.sandbox_enabled {
        Some(SandboxRunner::new(
            config.clone(),
            provider,
            config.agent.clone(),
        ))
    } else {
        None
    };

    // Create persistent container if reuse is enabled
    let persistent_container_name = if banner.sandbox_enabled && config.sandbox.reuse_container {
        match sandbox_runner.as_ref() {
            Some(runner) => match runner.create_persistent_container(&cwd).await {
                Ok(name) => {
                    info!("Created persistent container: {}", name);
                    Some(name)
                }
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

    let detector = CompletionDetector::new(completion_promise.as_deref());

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
        let output_result = if let Some(ref sandbox) = sandbox_runner {
            sandbox
                .run(&cwd, &prompt, persistent_container_name.as_deref())
                .await
        } else {
            // Non-sandbox mode: apply timeout from config
            let timeout_duration = std::time::Duration::from_secs(
                u64::from(config.sandbox.resources.timeout_minutes) * 60,
            );
            tokio::time::timeout(timeout_duration, agent.invoke(&cwd, &prompt))
                .await
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Agent execution timed out after {} minutes",
                        config.sandbox.resources.timeout_minutes
                    ))
                })
        };

        // Handle agent execution result (including timeouts)
        let output = match output_result {
            Ok(out) => out,
            Err(e) => {
                let error_msg = e.to_string();

                // Check if this is a recoverable error (timeout, rate limit, etc.)
                let is_timeout = error_msg.contains("timed out");
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
                            let backoff_seconds = match state.error_count {
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
                    state.last_error = Some(format!("Agent {error_type}: {error_msg}"));
                    state.last_iteration_at = Some(chrono::Utc::now());
                    state.iteration += 1;
                    state.save(&cwd)?;

                    // Show progress if enabled
                    if config.monitoring.show_progress {
                        let progress = ProgressInfo::new(&state, &cwd).await;
                        print!("{}", format_progress(&progress));
                    }

                    // Continue to next iteration
                    continue;
                }

                // For other errors, fail the loop (but cleanup container first)
                if let Some(container_name) = &persistent_container_name {
                    let _ = SandboxRunner::remove_persistent_container(container_name).await;
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

                    // Continue to next iteration (let agent fix it)
                    if config.monitoring.show_progress {
                        let progress = ProgressInfo::new(&state, &cwd).await;
                        print!("{}", format_progress(&progress));
                    }
                    continue;
                }
            }
        }

        // Update last iteration timestamp
        state.last_iteration_at = Some(chrono::Utc::now());
        state.save(&cwd)?;

        // Check for completion
        if detector.is_complete(&output, &cwd)? {
            println!(
                "{}",
                format_completion_detected(state.completion_promise.as_deref())
            );
            state.active = false;
            state.save(&cwd)?;

            // Log loop end
            tracing::info!(
                event = "loop_end",
                total_iterations = state.iteration,
                reason = "completion_detected",
            );

            // Send completion notification
            let details = NotificationDetails::complete(
                state.iteration,
                state.iteration,
                "completion_detected",
            );
            notifier.notify(NotificationEvent::Complete, &details).await;

            break;
        }

        // Get commit hash after agent execution (may have created commits)
        let commit_hash = get_current_commit_hash(&cwd).await.ok();

        // Git operations
        if config.git.auto_push {
            if let Err(e) = git_push(&cwd).await {
                warn!("Git push failed: {e}");
                state.error_count += 1;
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
    if let Some(container_name) = persistent_container_name {
        info!("Cleaning up persistent container: {}", container_name);
        if let Err(e) = SandboxRunner::remove_persistent_container(&container_name).await {
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
// Internal types
// -----------------------------------------------------------------------------

/// Banner information for display at loop start.
#[derive(Debug, Clone)]
struct BannerInfo {
    provider: String,
    mode: String,
    prompt_file: String,
    iteration: u32,
    max_iterations: Option<u32>,
    promise: Option<String>,
    sandbox_enabled: bool,
}

impl BannerInfo {
    fn new(
        state: &RalphState,
        prompt_file: &Path,
        no_sandbox: bool,
        config: &Config,
        provider: Provider,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            mode: format!("{:?}", state.mode),
            prompt_file: prompt_file.display().to_string(),
            iteration: state.iteration,
            max_iterations: state.max_iterations,
            promise: state.completion_promise.clone(),
            sandbox_enabled: !no_sandbox && config.sandbox.enabled,
        }
    }
}

/// Progress information for display during loop execution.
#[derive(Debug, Clone)]
struct ProgressInfo {
    iteration: u32,
    mode: String,
    elapsed_time: String,
    avg_iteration_duration: Option<String>,
    successful_commits: u32,
    errors: u32,
    last_commit_message: Option<String>,
}

impl ProgressInfo {
    async fn new(state: &RalphState, cwd: &Path) -> Self {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(state.started_at);
        let elapsed_time = format_duration_for_progress(&elapsed);

        let avg_iteration_duration = if state.iteration > 1 {
            let total_seconds = elapsed.num_seconds();
            let avg_seconds = total_seconds / i64::from(state.iteration);
            Some(format_duration_for_progress(&Duration::seconds(
                avg_seconds,
            )))
        } else {
            None
        };

        let successful_commits = count_successful_commits(cwd, state.started_at).await;
        let last_commit_message = get_last_commit_message(cwd).await;

        Self {
            iteration: state.iteration,
            mode: format!("{:?}", state.mode),
            elapsed_time,
            avg_iteration_duration,
            successful_commits,
            errors: state.error_count,
            last_commit_message,
        }
    }
}

/// Formats a duration for progress display (simpler format).
fn format_duration_for_progress(duration: &Duration) -> String {
    let total_seconds = duration.num_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
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
fn prepare_state(
    mut state: RalphState,
    max_iterations: Option<u32>,
    completion_promise: Option<String>,
) -> RalphState {
    state.max_iterations = max_iterations;
    state.completion_promise = completion_promise;
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
/// CLI override takes precedence over config.
fn resolve_provider(config: &Config, provider_override: Option<&str>) -> Result<Provider> {
    match provider_override {
        Some(p) => {
            debug!("Using CLI provider override: {}", p);
            p.parse()
        }
        None => config.agent.get_provider(),
    }
}

// -----------------------------------------------------------------------------
// Formatting functions
// -----------------------------------------------------------------------------

/// Formats the startup banner for display.
fn format_banner(info: &BannerInfo) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(&mut out, "\n{}", "━".repeat(50).dimmed()).unwrap();
    writeln!(&mut out, "{}", "   🔄 Ralph Loop Starting".yellow().bold()).unwrap();
    writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();

    writeln!(&mut out, "  Agent:      {}", info.provider.cyan().bold()).unwrap();
    writeln!(&mut out, "  Mode:       {}", info.mode.cyan()).unwrap();
    writeln!(&mut out, "  Prompt:     {}", info.prompt_file.cyan()).unwrap();
    writeln!(
        &mut out,
        "  Iteration:  {}",
        info.iteration.to_string().cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Max:        {}",
        info.max_iterations
            .map_or_else(|| "unlimited".to_string(), |n| n.to_string())
            .cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  Promise:    {}",
        info.promise.as_deref().unwrap_or("none").cyan()
    )
    .unwrap();

    let sandbox_status = if info.sandbox_enabled {
        "enabled".green()
    } else {
        "disabled".red()
    };
    writeln!(&mut out, "  Sandbox:    {sandbox_status}").unwrap();

    writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();
    writeln!(
        &mut out,
        "\n  {} to stop\n",
        "Ctrl+C or 'ralph cancel'".dimmed()
    )
    .unwrap();

    out
}

/// Formats the iteration header line.
fn format_iteration_header(iteration: u32) -> String {
    format!(
        "\n{} Iteration {} {}",
        "━".repeat(20).dimmed(),
        iteration.to_string().cyan().bold(),
        "━".repeat(20).dimmed()
    )
}

/// Formats progress display for real-time loop monitoring.
fn format_progress(info: &ProgressInfo) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(&mut out, "\n{}", "━".repeat(50).dimmed()).unwrap();
    writeln!(
        &mut out,
        "{} Iteration {} {}",
        "━".repeat(15).dimmed(),
        info.iteration.to_string().cyan().bold(),
        "━".repeat(15).dimmed()
    )
    .unwrap();
    writeln!(&mut out, "  Mode:      {}", info.mode.cyan()).unwrap();
    writeln!(&mut out, "  Started:   {} ago", info.elapsed_time.cyan()).unwrap();

    if let Some(ref avg) = info.avg_iteration_duration {
        writeln!(&mut out, "  Duration:    ~{}/iteration avg", avg.cyan()).unwrap();
    }

    writeln!(
        &mut out,
        "  Commits:   {} successful",
        info.successful_commits.to_string().green()
    )
    .unwrap();

    if info.errors > 0 {
        writeln!(
            &mut out,
            "  Errors:    {} (recovered)",
            info.errors.to_string().yellow()
        )
        .unwrap();
    }

    if let Some(ref commit_msg) = info.last_commit_message {
        writeln!(&mut out, "\n  Current task: {}", commit_msg.dimmed()).unwrap();
        writeln!(&mut out, "  Last commit:  \"{}\"", commit_msg.cyan()).unwrap();
    }

    writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();

    out
}

/// Formats the max iterations reached message.
fn format_max_iterations_reached(max: u32) -> String {
    format!("\n{} Max iterations ({}) reached.", "🛑".red(), max)
}

/// Formats the completion detected message.
fn format_completion_detected(promise: Option<&str>) -> String {
    format!(
        "\n{} Completion detected: {}",
        "✅".green(),
        promise.unwrap_or("task complete")
    )
}

/// Formats the loop finished message.
fn format_loop_finished(total_iterations: u32) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(&mut out, "\n{} Ralph loop finished.", "🎉".green()).unwrap();
    writeln!(
        &mut out,
        "  Total iterations: {}",
        total_iterations.to_string().cyan()
    )
    .unwrap();
    out
}

// -----------------------------------------------------------------------------
// Validation
// -----------------------------------------------------------------------------

/// Validates code by running the configured validation command.
/// Returns the full error message if validation fails.
async fn validate_code(cwd: &Path, command: &str) -> Result<(), String> {
    debug!("Validating code with command: {}", command);

    // Parse command into program and args
    let mut parts = command.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| "Validation command cannot be empty".to_string())?;
    let args: Vec<&str> = parts.collect();

    let output = tokio::process::Command::new(program)
        .current_dir(cwd)
        .args(&args)
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
// Git operations
// -----------------------------------------------------------------------------

async fn git_push(cwd: &Path) -> Result<()> {
    debug!("Pushing to git...");

    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["push"])
        .output()
        .await
        .context("Failed to run git push")?;

    if !output.status.success() {
        // Try to create upstream branch
        let branch = get_current_branch(cwd).await?;
        tokio::process::Command::new("git")
            .current_dir(cwd)
            .args(["push", "-u", "origin", &branch])
            .output()
            .await
            .context("Failed to push with upstream")?;
    }

    info!("Git push complete");
    Ok(())
}

async fn get_current_branch(cwd: &Path) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["branch", "--show-current"])
        .output()
        .await
        .context("Failed to get current branch")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the current git commit hash (short format).
async fn get_current_commit_hash(cwd: &Path) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .await
        .context("Failed to get current commit hash")?;

    if !output.status.success() {
        bail!("Git rev-parse failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the last commit message (first line only).
async fn get_last_commit_message(cwd: &Path) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "-1", "--pretty=%s"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

/// Count successful commits since loop started (commits with timestamps after `started_at`).
async fn count_successful_commits(cwd: &Path, started_at: DateTime<Utc>) -> u32 {
    // Get all commits since started_at using ISO format
    let since_str = started_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let output = match tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "--since", &since_str, "--pretty=format:%H"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };

    let commits = String::from_utf8_lossy(&output.stdout);
    let count = commits.lines().filter(|l| !l.is_empty()).count();
    // Truncate to u32::MAX if count exceeds u32 range (unlikely in practice)
    u32::try_from(count.min(u32::MAX as usize)).unwrap_or(u32::MAX)
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
            completion_promise: None,
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            last_error: None,
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
        let prepared = prepare_state(state, Some(10), Some("DONE".to_string()));

        assert!(prepared.active);
        assert_eq!(prepared.max_iterations, Some(10));
        assert_eq!(prepared.completion_promise, Some("DONE".to_string()));
    }

    #[test]
    fn test_prepare_state_unlimited() {
        let state = make_state(1, Some(5));
        let prepared = prepare_state(state, None, None);

        assert!(prepared.active);
        assert_eq!(prepared.max_iterations, None);
        assert_eq!(prepared.completion_promise, None);
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
    fn test_banner_info_creation() {
        let state = RalphState {
            active: true,
            mode: Mode::Plan,
            iteration: 5,
            max_iterations: Some(20),
            completion_promise: Some("COMPLETE".to_string()),
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            last_error: None,
        };
        let config = Config::default();
        let prompt = PathBuf::from("/project/PROMPT_plan.md");

        let banner = BannerInfo::new(&state, &prompt, false, &config, Provider::Cursor);

        assert_eq!(banner.provider, "cursor");
        assert_eq!(banner.mode, "Plan");
        assert_eq!(banner.iteration, 5);
        assert_eq!(banner.max_iterations, Some(20));
        assert_eq!(banner.promise, Some("COMPLETE".to_string()));
    }

    #[test]
    fn test_banner_info_sandbox_disabled_by_flag() {
        let state = RalphState::default();
        let mut config = Config::default();
        config.sandbox.enabled = true;
        let prompt = PathBuf::from("/project/PROMPT.md");

        let banner = BannerInfo::new(&state, &prompt, true, &config, Provider::Cursor);
        assert!(!banner.sandbox_enabled);
    }

    #[test]
    fn test_banner_info_sandbox_disabled_by_config() {
        let state = RalphState::default();
        let mut config = Config::default();
        config.sandbox.enabled = false;
        let prompt = PathBuf::from("/project/PROMPT.md");

        let banner = BannerInfo::new(&state, &prompt, false, &config, Provider::Cursor);
        assert!(!banner.sandbox_enabled);
    }

    #[test]
    fn test_format_banner() {
        let banner = BannerInfo {
            provider: "cursor".to_string(),
            mode: "Build".to_string(),
            prompt_file: "/project/PROMPT.md".to_string(),
            iteration: 3,
            max_iterations: Some(10),
            promise: Some("DONE".to_string()),
            sandbox_enabled: true,
        };

        let output = format_banner(&banner);
        assert!(output.contains("Ralph Loop Starting"));
        assert!(output.contains("cursor"));
        assert!(output.contains("Build"));
        assert!(output.contains("PROMPT.md"));
        assert!(output.contains("10"));
        assert!(output.contains("DONE"));
        assert!(output.contains("enabled"));
    }

    #[test]
    fn test_format_banner_unlimited() {
        let banner = BannerInfo {
            provider: "claude".to_string(),
            mode: "Plan".to_string(),
            prompt_file: "/project/PROMPT_plan.md".to_string(),
            iteration: 1,
            max_iterations: None,
            promise: None,
            sandbox_enabled: false,
        };

        let output = format_banner(&banner);
        assert!(output.contains("unlimited"));
        assert!(output.contains("none"));
        assert!(output.contains("disabled"));
    }

    #[test]
    fn test_format_iteration_header() {
        let output = format_iteration_header(5);
        assert!(output.contains("Iteration"));
        assert!(output.contains('5'));
    }

    #[test]
    fn test_format_max_iterations_reached() {
        let output = format_max_iterations_reached(10);
        assert!(output.contains("Max iterations"));
        assert!(output.contains("10"));
    }

    #[test]
    fn test_format_completion_detected() {
        let output = format_completion_detected(Some("ALL TESTS PASS"));
        assert!(output.contains("Completion detected"));
        assert!(output.contains("ALL TESTS PASS"));

        let output_none = format_completion_detected(None);
        assert!(output_none.contains("task complete"));
    }

    #[test]
    fn test_format_loop_finished() {
        let output = format_loop_finished(7);
        assert!(output.contains("loop finished"));
        assert!(output.contains('7'));
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

    #[tokio::test]
    async fn test_get_current_commit_hash() {
        use std::process::Command;

        // This test requires an actual git repository with at least one commit.
        // It's skipped if git is not available or if we're not in a git repo.
        // We test against the current repository rather than creating test commits.

        let cwd = std::env::current_dir().unwrap();

        // Check if we're in a git repo and if git is available
        let Ok(git_output) = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&cwd)
            .output()
        else {
            // Git not available - skip test
            return;
        };

        if !git_output.status.success() {
            // Not in a git repo - skip test
            return;
        }

        // Test getting commit hash from current repo
        // This uses the actual repository state, not a test commit
        if let Ok(hash) = get_current_commit_hash(&cwd).await {
            assert!(!hash.is_empty());
            // Short hash is typically 7 characters, but can vary
            assert!(hash.len() >= 7);
        }
        // If getting commit hash fails (no commits, etc.), that's acceptable in test environments
    }

    #[test]
    fn test_format_duration_for_progress_seconds() {
        let duration = Duration::seconds(45);
        let formatted = format_duration_for_progress(&duration);
        assert_eq!(formatted, "45s");
    }

    #[test]
    fn test_format_duration_for_progress_minutes() {
        let duration = Duration::seconds(125);
        let formatted = format_duration_for_progress(&duration);
        assert_eq!(formatted, "2m 5s");
    }

    #[test]
    fn test_format_duration_for_progress_hours() {
        let duration = Duration::seconds(3665);
        let formatted = format_duration_for_progress(&duration);
        assert_eq!(formatted, "1h 1m 5s");
    }

    #[test]
    fn test_format_progress() {
        let info = ProgressInfo {
            iteration: 15,
            mode: "Build".to_string(),
            elapsed_time: "2h 0m 0s".to_string(),
            avg_iteration_duration: Some("8m".to_string()),
            successful_commits: 12,
            errors: 2,
            last_commit_message: Some("Add JWT token validation".to_string()),
        };

        let output = format_progress(&info);
        // Strip ANSI color codes for testing (colors may not be available in all environments)
        let stripped = strip_ansi_codes(&output);
        assert!(stripped.contains("Iteration 15"));
        assert!(stripped.contains("Build"));
        assert!(stripped.contains("2h 0m 0s"));
        assert!(stripped.contains("12 successful"));
        assert!(stripped.contains("2 (recovered)"));
        assert!(stripped.contains("Add JWT token validation"));
    }

    #[test]
    fn test_format_progress_no_errors() {
        let info = ProgressInfo {
            iteration: 5,
            mode: "Plan".to_string(),
            elapsed_time: "30m 0s".to_string(),
            avg_iteration_duration: Some("6m".to_string()),
            successful_commits: 3,
            errors: 0,
            last_commit_message: None,
        };

        let output = format_progress(&info);
        // Strip ANSI color codes for testing (colors may not be available in all environments)
        let stripped = strip_ansi_codes(&output);
        assert!(stripped.contains("Iteration 5"));
        assert!(stripped.contains("Plan"));
        assert!(stripped.contains("3 successful"));
        assert!(!stripped.contains("Errors")); // Should not show errors line when 0
    }

    /// Strip ANSI color codes from a string for testing.
    /// This allows tests to work in environments where colors are disabled.
    fn strip_ansi_codes(s: &str) -> String {
        // Robust ANSI code stripper - removes all escape sequences
        // Handles formats like: \x1b[0m, \x1b[31m, \x1b[1;31m, \x1b[38;5;123m, etc.
        use std::fmt::Write;
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Skip ANSI escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                                  // Consume all characters until we hit a letter (the terminator: m, H, etc.)
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_alphabetic() {
                            chars.next(); // Consume the terminator
                            break;
                        } else if c.is_ascii_digit() || c == ';' || c == ':' || c == '?' {
                            chars.next(); // Continue consuming parameters
                        } else {
                            // Unexpected character, stop here
                            break;
                        }
                    }
                } else {
                    // \x1b not followed by '[', treat as regular character
                    result.write_char(ch).unwrap();
                }
            } else {
                result.write_char(ch).unwrap();
            }
        }
        result
    }
}
