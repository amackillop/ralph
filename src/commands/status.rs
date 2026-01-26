//! Show current Ralph loop status.
//!
//! Separates display formatting from state loading for testability.
//! Formatting is pure. IO happens only at the top level.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use colored::Colorize;
use std::fmt::Write;
use std::path::Path;

use crate::state::RalphState;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the status command, displaying current loop state.
pub(crate) fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    let state = RalphState::load(&cwd)?;
    let status = state.as_ref().map(|s| {
        let recent_commits = get_recent_commits(&cwd).unwrap_or_default();
        StatusDisplay::from_state(s, &recent_commits)
    });
    print!("{}", format_status_colored(status.as_ref()));

    Ok(())
}

// -----------------------------------------------------------------------------
// Internal types
// -----------------------------------------------------------------------------

/// Formatted status output for display.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusDisplay {
    active: bool,
    mode: String,
    iteration: u32,
    max_iterations: Option<u32>,
    promise: Option<String>,
    started_at: String,
    last_iteration_at: Option<String>,
    elapsed_time: String,
    avg_iteration_duration: Option<String>,
    estimated_remaining: Option<String>,
    error_count: u32,
    last_error: Option<String>,
    recent_commits: Vec<String>,
}

impl StatusDisplay {
    fn from_state(state: &RalphState, recent_commits: &[String]) -> Self {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(state.started_at);
        let elapsed_time = format_duration(&elapsed);

        let avg_iteration_duration = if state.iteration > 1 {
            let total_duration = elapsed;
            let avg_seconds = total_duration.num_seconds() / i64::from(state.iteration);
            Some(format_duration(&Duration::seconds(avg_seconds)))
        } else {
            None
        };

        let estimated_remaining = if let (Some(max_iter), Some(_avg_dur)) =
            (state.max_iterations, &avg_iteration_duration)
        {
            if state.iteration < max_iter {
                let remaining_iterations = max_iter - state.iteration;
                // Parse average duration to estimate remaining time
                // For simplicity, use elapsed time per iteration
                if state.iteration > 1 {
                    let avg_seconds = elapsed.num_seconds() / i64::from(state.iteration);
                    let remaining_seconds = avg_seconds * i64::from(remaining_iterations);
                    Some(format_duration(&Duration::seconds(remaining_seconds)))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Self {
            active: state.active,
            mode: format!("{:?}", state.mode),
            iteration: state.iteration,
            max_iterations: state.max_iterations,
            promise: state.completion_promise.clone(),
            started_at: state.started_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            last_iteration_at: state
                .last_iteration_at
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
            elapsed_time,
            avg_iteration_duration,
            estimated_remaining,
            error_count: state.error_count,
            last_error: state.last_error.clone(),
            recent_commits: recent_commits.to_vec(),
        }
    }
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Formats status for terminal output (plain text, testable).
#[cfg(test)]
fn format_status(status: Option<&StatusDisplay>) -> String {
    let mut out = String::new();
    if let Some(s) = status {
        writeln!(
            &mut out,
            "  Status:     {}",
            if s.active { "active" } else { "inactive" }
        )
        .unwrap();
        writeln!(&mut out, "  Mode:       {}", s.mode).unwrap();
        writeln!(&mut out, "  Iteration:  {}", s.iteration).unwrap();
        writeln!(
            &mut out,
            "  Max:        {}",
            s.max_iterations
                .map_or_else(|| "unlimited".to_string(), |n| n.to_string())
        )
        .unwrap();
        writeln!(
            &mut out,
            "  Promise:    {}",
            s.promise.as_deref().unwrap_or("none")
        )
        .unwrap();
        writeln!(&mut out, "  Started:    {}", s.started_at).unwrap();
        writeln!(&mut out, "  Elapsed:    {}", s.elapsed_time).unwrap();
        if let Some(ref last) = s.last_iteration_at {
            writeln!(&mut out, "  Last iter:  {last}").unwrap();
        }
        if let Some(ref avg) = s.avg_iteration_duration {
            writeln!(&mut out, "  Avg/iter:   {avg}").unwrap();
        }
        if let Some(ref remaining) = s.estimated_remaining {
            writeln!(&mut out, "  Est. left:  {remaining}").unwrap();
        }
        if s.error_count > 0 {
            writeln!(&mut out, "  Errors:    {}", s.error_count).unwrap();
            if let Some(ref last_error) = s.last_error {
                let display_error = if last_error.len() > 80 {
                    format!("{}...", &last_error[..77])
                } else {
                    last_error.clone()
                };
                writeln!(&mut out, "  Last error: {display_error}").unwrap();
            }
        }
        if !s.recent_commits.is_empty() {
            writeln!(&mut out, "\n  Recent commits:").unwrap();
            for commit in s.recent_commits.iter().take(5) {
                writeln!(&mut out, "    {commit}").unwrap();
            }
        }
    } else {
        writeln!(&mut out, "No active Ralph loop found.").unwrap();
        writeln!(&mut out, "Run 'ralph loop' to start one.").unwrap();
    }
    out
}

/// Formats status with colors for terminal display.
fn format_status_colored(status: Option<&StatusDisplay>) -> String {
    let mut out = String::new();
    if let Some(s) = status {
        writeln!(&mut out, "\n{}", "━".repeat(50).dimmed()).unwrap();
        writeln!(&mut out, "{}", "   🔄 Ralph Loop Status".yellow().bold()).unwrap();
        writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();

        let active_str = if s.active {
            "active".green().bold().to_string()
        } else {
            "inactive".red().to_string()
        };
        writeln!(&mut out, "  Status:     {active_str}").unwrap();
        writeln!(&mut out, "  Mode:       {}", s.mode.cyan()).unwrap();
        writeln!(&mut out, "  Iteration:  {}", s.iteration.to_string().cyan()).unwrap();
        writeln!(
            &mut out,
            "  Max:        {}",
            s.max_iterations
                .map_or_else(|| "unlimited".to_string(), |n| n.to_string())
                .cyan()
        )
        .unwrap();
        writeln!(
            &mut out,
            "  Promise:    {}",
            s.promise.as_deref().unwrap_or("none").cyan()
        )
        .unwrap();
        writeln!(&mut out, "  Started:    {}", s.started_at.cyan()).unwrap();
        writeln!(&mut out, "  Elapsed:    {}", s.elapsed_time.cyan()).unwrap();

        if let Some(ref last) = s.last_iteration_at {
            writeln!(&mut out, "  Last iter:  {}", last.cyan()).unwrap();
        }

        if let Some(ref avg) = s.avg_iteration_duration {
            writeln!(&mut out, "  Avg/iter:   {}", avg.cyan()).unwrap();
        }

        if let Some(ref remaining) = s.estimated_remaining {
            writeln!(&mut out, "  Est. left:  {}", remaining.cyan()).unwrap();
        }

        if s.error_count > 0 {
            writeln!(
                &mut out,
                "  Errors:     {} (recovered)",
                s.error_count.to_string().yellow()
            )
            .unwrap();
            if let Some(ref last_error) = s.last_error {
                let display_error = if last_error.len() > 80 {
                    format!("{}...", &last_error[..77])
                } else {
                    last_error.clone()
                };
                writeln!(&mut out, "  Last error: {}", display_error.yellow()).unwrap();
            }
        }

        if !s.recent_commits.is_empty() {
            writeln!(&mut out, "\n  Recent commits:").unwrap();
            for commit in s.recent_commits.iter().take(5) {
                writeln!(&mut out, "    {}", commit.dimmed()).unwrap();
            }
        }

        writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();
    } else {
        writeln!(&mut out, "\n{} No active Ralph loop found.", "ℹ".blue()).unwrap();
        writeln!(&mut out, "  Run {} to start one.", "ralph loop".green()).unwrap();
    }
    out
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Formats a duration into a human-readable string.
fn format_duration(duration: &Duration) -> String {
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

/// Gets recent commit messages from git log.
fn get_recent_commits(cwd: &Path) -> Result<Vec<String>> {
    use std::process::Command;

    let output = Command::new("git")
        .current_dir(cwd)
        .args(["log", "--oneline", "-n", "5"])
        .output()
        .context("Failed to get git log")?;

    if !output.status.success() {
        return Ok(Vec::new()); // Not a git repo or no commits, return empty
    }

    let log_output = String::from_utf8_lossy(&output.stdout);
    Ok(parse_commits(&log_output))
}

/// Parses git log output into commit summary lines.
fn parse_commits(log_output: &str) -> Vec<String> {
    log_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Mode;
    use chrono::Utc;

    #[test]
    fn test_status_display_from_state() {
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 5,
            max_iterations: Some(10),
            completion_promise: Some("DONE".to_string()),
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
        };

        let status = StatusDisplay::from_state(&state, &[]);
        assert!(status.active);
        assert_eq!(status.mode, "Build");
        assert_eq!(status.iteration, 5);
        assert_eq!(status.max_iterations, Some(10));
        assert_eq!(status.promise, Some("DONE".to_string()));
    }

    #[test]
    fn test_format_status_with_state() {
        let status = StatusDisplay {
            active: true,
            mode: "Build".to_string(),
            iteration: 3,
            max_iterations: Some(20),
            promise: Some("COMPLETE".to_string()),
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: Some("2024-01-01 12:05:00 UTC".to_string()),
            elapsed_time: "15m".to_string(),
            avg_iteration_duration: Some("5m".to_string()),
            estimated_remaining: Some("85m".to_string()),
            error_count: 0,
            last_error: None,
            recent_commits: Vec::new(),
        };

        let output = format_status(Some(&status));
        assert!(output.contains("active"));
        assert!(output.contains("Build"));
        assert!(output.contains('3'));
        assert!(output.contains("20"));
        assert!(output.contains("COMPLETE"));
        assert!(output.contains("12:05:00"));
    }

    #[test]
    fn test_format_status_none() {
        let output = format_status(None);
        assert!(output.contains("No active Ralph loop"));
    }

    #[test]
    fn test_format_status_unlimited_iterations() {
        let status = StatusDisplay {
            active: false,
            mode: "Plan".to_string(),
            iteration: 1,
            max_iterations: None,
            promise: None,
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: None,
            elapsed_time: "2m".to_string(),
            avg_iteration_duration: None,
            estimated_remaining: None,
            error_count: 0,
            last_error: None,
            recent_commits: Vec::new(),
        };

        let output = format_status(Some(&status));
        assert!(output.contains("unlimited"));
        assert!(output.contains("none"));
        assert!(output.contains("inactive"));
    }

    #[test]
    fn test_format_status_colored_has_banner() {
        let status = StatusDisplay {
            active: true,
            mode: "Build".to_string(),
            iteration: 1,
            max_iterations: None,
            promise: None,
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: None,
            elapsed_time: "5m 30s".to_string(),
            avg_iteration_duration: None,
            estimated_remaining: None,
            error_count: 0,
            last_error: None,
            recent_commits: Vec::new(),
        };

        let output = format_status_colored(Some(&status));
        assert!(output.contains("Ralph Loop Status"));
        assert!(output.contains("━"));
    }

    #[test]
    fn test_format_duration_seconds() {
        let duration = Duration::seconds(45);
        assert_eq!(format_duration(&duration), "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let duration = Duration::seconds(125);
        assert_eq!(format_duration(&duration), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        let duration = Duration::seconds(3665);
        assert_eq!(format_duration(&duration), "1h 1m 5s");
    }

    #[test]
    fn test_parse_commits_single() {
        let log = "abc1234 Fix bug";
        let commits = parse_commits(log);
        assert_eq!(commits, vec!["abc1234 Fix bug"]);
    }

    #[test]
    fn test_parse_commits_multiple() {
        let log = "abc1234 Fix bug\ndef5678 Add feature\nghi9012 Update docs";
        let commits = parse_commits(log);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0], "abc1234 Fix bug");
    }

    #[test]
    fn test_parse_commits_empty() {
        let commits = parse_commits("");
        assert!(commits.is_empty());
    }

    #[test]
    fn test_status_display_with_timing() {
        use chrono::{Duration, Utc};
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 5,
            max_iterations: Some(10),
            completion_promise: Some("DONE".to_string()),
            started_at: Utc::now() - Duration::minutes(30),
            last_iteration_at: Some(Utc::now() - Duration::minutes(2)),
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
        };

        let status = StatusDisplay::from_state(&state, &[]);
        assert!(status.active);
        assert_eq!(status.iteration, 5);
        assert!(status.elapsed_time.contains('m') || status.elapsed_time.contains('h'));
        assert!(status.avg_iteration_duration.is_some());
        assert!(status.estimated_remaining.is_some());
    }

    #[test]
    fn test_status_display_with_errors() {
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 10,
            max_iterations: Some(20),
            completion_promise: None,
            started_at: Utc::now() - Duration::hours(1),
            last_iteration_at: Some(Utc::now() - Duration::minutes(5)),
            error_count: 3,
            consecutive_errors: 2,
            last_error: Some("Git push failed: connection timeout".to_string()),
        };

        let status = StatusDisplay::from_state(&state, &[]);
        assert_eq!(status.error_count, 3);
        assert_eq!(
            status.last_error,
            Some("Git push failed: connection timeout".to_string())
        );
    }

    #[test]
    fn test_format_status_with_errors() {
        let status = StatusDisplay {
            active: true,
            mode: "Build".to_string(),
            iteration: 5,
            max_iterations: Some(10),
            promise: None,
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: None,
            elapsed_time: "30m".to_string(),
            avg_iteration_duration: None,
            estimated_remaining: None,
            error_count: 2,
            last_error: Some("Agent execution timed out".to_string()),
            recent_commits: Vec::new(),
        };

        let output = format_status(Some(&status));
        assert!(output.contains("Errors:"));
        assert!(output.contains('2'));
        assert!(output.contains("Last error:"));
        assert!(output.contains("Agent execution timed out"));
    }

    #[test]
    fn test_format_status_colored_with_errors() {
        let status = StatusDisplay {
            active: true,
            mode: "Build".to_string(),
            iteration: 5,
            max_iterations: Some(10),
            promise: None,
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: None,
            elapsed_time: "30m".to_string(),
            avg_iteration_duration: None,
            estimated_remaining: None,
            error_count: 1,
            last_error: Some("Test error message".to_string()),
            recent_commits: Vec::new(),
        };

        let output = format_status_colored(Some(&status));
        assert!(output.contains("Errors:"));
        assert!(output.contains('1'));
        assert!(output.contains("Last error:"));
        assert!(output.contains("Test error message"));
    }

    #[test]
    fn test_format_status_no_errors() {
        let status = StatusDisplay {
            active: true,
            mode: "Build".to_string(),
            iteration: 5,
            max_iterations: Some(10),
            promise: None,
            started_at: "2024-01-01 12:00:00 UTC".to_string(),
            last_iteration_at: None,
            elapsed_time: "30m".to_string(),
            avg_iteration_duration: None,
            estimated_remaining: None,
            error_count: 0,
            last_error: None,
            recent_commits: Vec::new(),
        };

        let output = format_status(Some(&status));
        assert!(!output.contains("Errors:"));
        assert!(!output.contains("Last error:"));
    }
}
