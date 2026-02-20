//! Formatting functions for loop output display.
//!
//! This module provides pure formatting functions that return strings,
//! following the principle of separating formatting from printing.

use chrono::{Duration, Utc};
use colored::Colorize;
use std::fmt::Write;
use std::path::Path;

use crate::agent::Provider;
use crate::config::Config;
use crate::state::RalphState;

use super::git::{count_successful_commits, get_last_commit_message};

/// Banner information for display at loop start.
#[derive(Debug, Clone)]
pub(crate) struct BannerInfo {
    pub provider: String,
    pub mode: String,
    pub prompt_file: String,
    pub iteration: u32,
    pub max_iterations: Option<u32>,
    pub sandbox_enabled: bool,
}

impl BannerInfo {
    pub fn new(
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
            sandbox_enabled: !no_sandbox && config.sandbox.enabled,
        }
    }
}

/// Progress information for display during loop execution.
#[derive(Debug, Clone)]
pub(crate) struct ProgressInfo {
    pub iteration: u32,
    pub mode: String,
    pub elapsed_time: String,
    pub avg_iteration_duration: Option<String>,
    pub successful_commits: u32,
    pub errors: u32,
    pub last_commit_message: Option<String>,
}

impl ProgressInfo {
    pub async fn new(state: &RalphState, cwd: &Path) -> Self {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(state.started_at);
        let elapsed_time = format_duration(&elapsed);

        let avg_iteration_duration = if state.iteration > 1 {
            let total_seconds = elapsed.num_seconds();
            let avg_seconds = total_seconds / i64::from(state.iteration);
            Some(format_duration(&Duration::seconds(avg_seconds)))
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

/// Formats a duration for display (e.g., "2h 5m 30s").
pub(crate) fn format_duration(duration: &Duration) -> String {
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

/// Formats the startup banner for display.
pub(crate) fn format_banner(info: &BannerInfo) -> String {
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
pub(crate) fn format_iteration_header(iteration: u32) -> String {
    format!(
        "\n{} Iteration {} {}",
        "━".repeat(20).dimmed(),
        iteration.to_string().cyan().bold(),
        "━".repeat(20).dimmed()
    )
}

/// Formats progress display for real-time loop monitoring.
pub(crate) fn format_progress(info: &ProgressInfo) -> String {
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
pub(crate) fn format_max_iterations_reached(max: u32) -> String {
    format!("\n{} Max iterations ({}) reached.", "🛑".red(), max)
}

/// Formats the completion detected message.
pub(crate) fn format_completion_detected(idle_count: u32) -> String {
    format!(
        "\n{} Agent idle for {} iterations - task complete.",
        "✅".green(),
        idle_count
    )
}

/// Formats the loop finished message.
pub(crate) fn format_loop_finished(total_iterations: u32) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Mode;

    /// Strip ANSI color codes from a string for testing.
    /// This allows tests to work in environments where colors are disabled.
    fn strip_ansi_codes(s: &str) -> String {
        // Robust ANSI code stripper - removes all escape sequences
        // Handles formats like: \x1b[0m, \x1b[31m, \x1b[1;31m, \x1b[38;5;123m, etc.
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

    #[test]
    fn test_banner_info_creation() {
        let state = RalphState {
            active: true,
            mode: Mode::Plan,
            iteration: 5,
            max_iterations: Some(20),
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
        };
        let config = Config::default();
        let prompt = std::path::PathBuf::from("/project/PROMPT_plan.md");

        let banner = BannerInfo::new(&state, &prompt, false, &config, Provider::Cursor);

        assert_eq!(banner.provider, "cursor");
        assert_eq!(banner.mode, "Plan");
        assert_eq!(banner.iteration, 5);
        assert_eq!(banner.max_iterations, Some(20));
    }

    #[test]
    fn test_banner_info_sandbox_disabled_by_flag() {
        let state = RalphState::default();
        let mut config = Config::default();
        config.sandbox.enabled = true;
        let prompt = std::path::PathBuf::from("/project/PROMPT.md");

        let banner = BannerInfo::new(&state, &prompt, true, &config, Provider::Cursor);
        assert!(!banner.sandbox_enabled);
    }

    #[test]
    fn test_banner_info_sandbox_disabled_by_config() {
        let state = RalphState::default();
        let mut config = Config::default();
        config.sandbox.enabled = false;
        let prompt = std::path::PathBuf::from("/project/PROMPT.md");

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
            sandbox_enabled: true,
        };

        let output = format_banner(&banner);
        assert!(output.contains("Ralph Loop Starting"));
        assert!(output.contains("cursor"));
        assert!(output.contains("Build"));
        assert!(output.contains("PROMPT.md"));
        assert!(output.contains("10"));
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
            sandbox_enabled: false,
        };

        let output = format_banner(&banner);
        assert!(output.contains("unlimited"));
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
        let output = format_completion_detected(2);
        assert!(output.contains("idle"));
        assert!(output.contains('2'));
        assert!(output.contains("task complete"));
    }

    #[test]
    fn test_format_loop_finished() {
        let output = format_loop_finished(7);
        assert!(output.contains("loop finished"));
        assert!(output.contains('7'));
    }

    #[test]
    fn test_format_duration_seconds() {
        let duration = Duration::seconds(45);
        let formatted = format_duration(&duration);
        assert_eq!(formatted, "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let duration = Duration::seconds(125);
        let formatted = format_duration(&duration);
        assert_eq!(formatted, "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        let duration = Duration::seconds(3665);
        let formatted = format_duration(&duration);
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
}
