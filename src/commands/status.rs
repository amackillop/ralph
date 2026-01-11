//! Show current Ralph loop status.
//!
//! Separates display formatting from state loading for testability.
//! Formatting is pure. IO happens only at the top level.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fmt::Write;

use crate::state::RalphState;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the status command, displaying current loop state.
pub(crate) fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    let status = RalphState::load(&cwd)?.map(|s| StatusDisplay::from(&s));
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
}

impl From<&RalphState> for StatusDisplay {
    fn from(state: &RalphState) -> Self {
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
        }
    }
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Formats status for terminal output (plain text, testable).
#[allow(dead_code)]
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
        if let Some(ref last) = s.last_iteration_at {
            writeln!(&mut out, "  Last iter:  {last}").unwrap();
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

        if let Some(ref last) = s.last_iteration_at {
            writeln!(&mut out, "  Last iter:  {}", last.cyan()).unwrap();
        }

        writeln!(&mut out, "{}", "━".repeat(50).dimmed()).unwrap();
    } else {
        writeln!(&mut out, "\n{} No active Ralph loop found.", "ℹ".blue()).unwrap();
        writeln!(&mut out, "  Run {} to start one.", "ralph loop".green()).unwrap();
    }
    out
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
        };

        let status = StatusDisplay::from(&state);
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
        };

        let output = format_status_colored(Some(&status));
        assert!(output.contains("Ralph Loop Status"));
        assert!(output.contains("━"));
    }
}
