//! Main Ralph loop command.
//!
//! This module runs the iterative AI development loop. Core logic
//! is separated into testable functions where possible.

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use colored::Colorize;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::agent::{AgentProvider, ClaudeProvider, CursorProvider, Provider};
use crate::config::Config;
use crate::detection::CompletionDetector;
use crate::state::{Mode, RalphState};

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the main Ralph loop with the specified configuration.
pub(crate) async fn run(
    mode: LoopMode,
    max_iterations: u32,
    completion_promise: Option<String>,
    no_sandbox: bool,
    custom_prompt: Option<String>,
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

    // Get the configured agent provider
    let provider = config.agent.get_provider()?;

    // Print startup banner
    let banner = BannerInfo::new(&state, &prompt_file, no_sandbox, &config, provider);
    print!("{}", format_banner(&banner));

    // Create the agent provider
    let agent: Box<dyn AgentProvider> = match provider {
        Provider::Cursor => Box::new(CursorProvider::new(config.agent.cursor.clone())),
        Provider::Claude => Box::new(ClaudeProvider::new(config.agent.claude.clone())),
    };

    let detector = CompletionDetector::new(completion_promise.as_deref());

    // Warn about sandbox (not yet implemented for multi-provider)
    if banner.sandbox_enabled {
        warn!(
            "Docker sandbox is not yet implemented for the provider system. Running without sandbox."
        );
    }

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
            break;
        }

        println!("{}", format_iteration_header(state.iteration));

        // Read prompt
        let prompt = std::fs::read_to_string(&prompt_file)
            .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))?;

        // Run agent
        info!(
            "Running {} agent iteration {}",
            agent.name(),
            state.iteration
        );
        let output = agent.invoke(&cwd, &prompt).await?;

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
            break;
        }

        // Git operations
        if config.git.auto_push
            && let Err(e) = git_push(&cwd).await
        {
            warn!("Git push failed: {e}");
        }

        // Increment iteration
        state.iteration += 1;
        state.save(&cwd)?;
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

/// Outcome of a single loop iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum IterationOutcome {
    /// Continue to next iteration.
    Continue,
    /// Max iterations reached, stop loop.
    MaxIterationsReached,
    /// Completion promise detected, stop loop.
    CompletionDetected,
}

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
    max_iterations: u32,
    completion_promise: Option<String>,
) -> RalphState {
    state.max_iterations = if max_iterations > 0 {
        Some(max_iterations)
    } else {
        None
    };
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
        let prepared = prepare_state(state, 10, Some("DONE".to_string()));

        assert!(prepared.active);
        assert_eq!(prepared.max_iterations, Some(10));
        assert_eq!(prepared.completion_promise, Some("DONE".to_string()));
    }

    #[test]
    fn test_prepare_state_unlimited() {
        let state = make_state(1, Some(5));
        let prepared = prepare_state(state, 0, None);

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
}
