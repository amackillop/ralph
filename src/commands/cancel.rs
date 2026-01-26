//! Cancel an active Ralph loop.
//!
//! Core logic is pure: takes state, returns updated state.
//! Formatting is pure: returns strings. IO happens only at the top level.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fmt::Write;

use crate::state::RalphState;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the cancel command, deactivating any active loop.
pub(crate) fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    let state = RalphState::load(&cwd)?;
    let (result, updated_state) = cancel_loop(state);

    // Save if we have updated state
    if let Some(s) = updated_state {
        if matches!(result, CancelResult::Cancelled { .. }) {
            s.save(&cwd)?;
        }
    }

    print!("{}", format_result(&result));
    Ok(())
}

// -----------------------------------------------------------------------------
// Internal types
// -----------------------------------------------------------------------------

/// Result of a cancel operation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CancelResult {
    /// Loop was successfully cancelled.
    Cancelled { iteration: u32 },
    /// No active loop was found.
    NoActiveLoop,
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Pure cancel logic: if state is active, deactivate it.
fn cancel_loop(state: Option<RalphState>) -> (CancelResult, Option<RalphState>) {
    match state {
        Some(mut s) if s.active => {
            let iteration = s.iteration;
            s.active = false;
            (CancelResult::Cancelled { iteration }, Some(s))
        }
        Some(s) => (CancelResult::NoActiveLoop, Some(s)),
        None => (CancelResult::NoActiveLoop, None),
    }
}

/// Formats the cancel result as a displayable string.
fn format_result(result: &CancelResult) -> String {
    let mut out = String::new();
    match result {
        CancelResult::Cancelled { iteration } => {
            writeln!(
                &mut out,
                "\n{} Ralph loop cancelled (was at iteration {}).",
                "✓".green(),
                iteration.to_string().cyan()
            )
            .unwrap();
        }
        CancelResult::NoActiveLoop => {
            writeln!(&mut out, "\n{} No active Ralph loop found.", "ℹ".blue()).unwrap();
        }
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

    fn make_state(active: bool, iteration: u32) -> RalphState {
        RalphState {
            active,
            mode: Mode::Build,
            iteration,
            max_iterations: None,
            completion_promise: None,
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
        }
    }

    #[test]
    fn test_cancel_active_loop() {
        let state = make_state(true, 5);
        let (result, updated) = cancel_loop(Some(state));

        assert_eq!(result, CancelResult::Cancelled { iteration: 5 });
        assert!(!updated.unwrap().active);
    }

    #[test]
    fn test_cancel_inactive_loop() {
        let state = make_state(false, 3);
        let (result, _) = cancel_loop(Some(state));

        assert_eq!(result, CancelResult::NoActiveLoop);
    }

    #[test]
    fn test_cancel_no_state() {
        let (result, updated) = cancel_loop(None);

        assert_eq!(result, CancelResult::NoActiveLoop);
        assert!(updated.is_none());
    }

    #[test]
    fn test_cancel_preserves_other_fields() {
        let state = RalphState {
            active: true,
            mode: Mode::Plan,
            iteration: 10,
            max_iterations: Some(50),
            completion_promise: Some("DONE".to_string()),
            started_at: Utc::now(),
            last_iteration_at: Some(Utc::now()),
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
        };

        let (_, updated) = cancel_loop(Some(state.clone()));
        let updated = updated.unwrap();

        assert!(!updated.active);
        assert_eq!(updated.mode, Mode::Plan);
        assert_eq!(updated.iteration, 10);
        assert_eq!(updated.max_iterations, Some(50));
        assert_eq!(updated.completion_promise, Some("DONE".to_string()));
    }

    #[test]
    fn test_format_result_cancelled() {
        let result = CancelResult::Cancelled { iteration: 5 };
        let output = format_result(&result);
        assert!(output.contains("cancelled"));
        assert!(output.contains('5'));
    }

    #[test]
    fn test_format_result_no_loop() {
        let result = CancelResult::NoActiveLoop;
        let output = format_result(&result);
        assert!(output.contains("No active Ralph loop"));
    }
}
