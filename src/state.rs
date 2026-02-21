//! Ralph loop state management.
//!
//! Persists loop state to `.ralph/state.toml` including iteration count,
//! mode, and timing information.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const STATE_FILE: &str = ".ralph/state.toml";

/// Loop execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    /// Planning mode - generates implementation plans from specs.
    Plan,
    /// Build mode - implements features according to the plan.
    Build,
}

/// Persistent state for a Ralph loop.
///
/// Stored in `.ralph/state.toml` and tracks the current iteration,
/// mode, limits, and timing information across loop restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RalphState {
    /// Whether the loop is currently active.
    pub active: bool,
    /// Current execution mode (Plan or Build).
    pub mode: Mode,
    /// Current iteration number (1-indexed).
    pub iteration: u32,
    /// Maximum iterations before auto-stop (None = unlimited).
    pub max_iterations: Option<u32>,
    /// When the loop was started.
    pub started_at: DateTime<Utc>,
    /// When the last iteration completed.
    pub last_iteration_at: Option<DateTime<Utc>>,
    /// Total number of recoverable errors encountered (cumulative, never resets).
    #[serde(default)]
    pub error_count: u32,
    /// Consecutive errors without a successful iteration (resets on success).
    /// Used for exponential backoff and circuit breaker logic.
    #[serde(default)]
    pub consecutive_errors: u32,
    /// Last error message encountered (if any).
    #[serde(default)]
    pub last_error: Option<String>,
    /// Last known git commit hash for idle detection.
    /// Persisted so idle detection survives restarts.
    #[serde(default)]
    pub last_commit: Option<String>,
    /// Consecutive iterations without git changes.
    /// Persisted so idle detection continues correctly after restart.
    #[serde(default)]
    pub idle_iterations: u32,
}

impl Default for RalphState {
    fn default() -> Self {
        Self {
            active: false,
            mode: Mode::Build,
            iteration: 1,
            max_iterations: None,
            started_at: Utc::now(),
            last_iteration_at: None,
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
            last_commit: None,
            idle_iterations: 0,
        }
    }
}

impl RalphState {
    /// Load state from file if it exists
    pub fn load(project_dir: &Path) -> Result<Option<Self>> {
        let state_path = project_dir.join(STATE_FILE);

        if !state_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&state_path)
            .with_context(|| format!("Failed to read state file: {}", state_path.display()))?;

        let state: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse state file: {}", state_path.display()))?;

        Ok(Some(state))
    }

    /// Load existing state or create new one
    pub fn load_or_create(project_dir: &Path, mode: Mode) -> Result<Self> {
        match Self::load(project_dir)? {
            Some(state) if state.active => Ok(state),
            _ => Ok(Self {
                mode,
                ..Default::default()
            }),
        }
    }

    /// Save state to file
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let state_path = project_dir.join(STATE_FILE);

        // Ensure .ralph directory exists
        if let Some(parent) = state_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize state")?;

        fs::write(&state_path, content)
            .with_context(|| format!("Failed to write state file: {}", state_path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_state(active: bool, mode: Mode) -> RalphState {
        RalphState {
            active,
            mode,
            iteration: 5,
            max_iterations: Some(20),
            started_at: Utc::now(),
            last_iteration_at: Some(Utc::now()),
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
            last_commit: None,
            idle_iterations: 0,
        }
    }

    #[test]
    fn test_state_roundtrip() {
        let dir = tempdir().unwrap();
        let state = make_state(true, Mode::Build);

        state.save(dir.path()).unwrap();
        let loaded = RalphState::load(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.active, state.active);
        assert_eq!(loaded.mode, state.mode);
        assert_eq!(loaded.iteration, state.iteration);
        assert_eq!(loaded.max_iterations, state.max_iterations);
        assert_eq!(loaded.error_count, state.error_count);
        assert_eq!(loaded.consecutive_errors, state.consecutive_errors);
        assert_eq!(loaded.last_error, state.last_error);
        assert_eq!(loaded.last_commit, state.last_commit);
        assert_eq!(loaded.idle_iterations, state.idle_iterations);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempdir().unwrap();
        let result = RalphState::load(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_or_create_with_active_state() {
        let dir = tempdir().unwrap();
        let state = make_state(true, Mode::Build);
        state.save(dir.path()).unwrap();

        // Should return existing active state, ignoring requested mode
        let loaded = RalphState::load_or_create(dir.path(), Mode::Plan).unwrap();
        assert!(loaded.active);
        assert_eq!(loaded.mode, Mode::Build); // Original mode preserved
        assert_eq!(loaded.iteration, 5);
    }

    #[test]
    fn test_load_or_create_with_inactive_state() {
        let dir = tempdir().unwrap();
        let state = make_state(false, Mode::Build);
        state.save(dir.path()).unwrap();

        // Should create new state with requested mode
        let loaded = RalphState::load_or_create(dir.path(), Mode::Plan).unwrap();
        assert!(!loaded.active);
        assert_eq!(loaded.mode, Mode::Plan);
        assert_eq!(loaded.iteration, 1); // Default iteration
    }

    #[test]
    fn test_load_or_create_no_state() {
        let dir = tempdir().unwrap();

        let loaded = RalphState::load_or_create(dir.path(), Mode::Plan).unwrap();
        assert!(!loaded.active);
        assert_eq!(loaded.mode, Mode::Plan);
        assert_eq!(loaded.iteration, 1);
    }

    #[test]
    fn test_default_state() {
        let state = RalphState::default();
        assert!(!state.active);
        assert_eq!(state.mode, Mode::Build);
        assert_eq!(state.iteration, 1);
        assert!(state.max_iterations.is_none());
        assert!(state.last_iteration_at.is_none());
        assert_eq!(state.error_count, 0);
        assert_eq!(state.consecutive_errors, 0);
        assert!(state.last_error.is_none());
        assert!(state.last_commit.is_none());
        assert_eq!(state.idle_iterations, 0);
    }

    #[test]
    fn test_mode_serialization() {
        let state = RalphState {
            mode: Mode::Plan,
            ..Default::default()
        };
        let serialized = toml::to_string(&state).unwrap();
        assert!(serialized.contains("mode = \"plan\""));

        let state = RalphState {
            mode: Mode::Build,
            ..Default::default()
        };
        let serialized = toml::to_string(&state).unwrap();
        assert!(serialized.contains("mode = \"build\""));
    }

    #[test]
    fn test_save_creates_directory() {
        let dir = tempdir().unwrap();
        // Directory doesn't have .ralph yet
        let state = make_state(true, Mode::Build);
        state.save(dir.path()).unwrap();

        // Should have created .ralph directory
        assert!(dir.path().join(".ralph").exists());
        assert!(dir.path().join(".ralph/state.toml").exists());
    }

    #[test]
    fn test_state_with_errors() {
        let dir = tempdir().unwrap();
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 5,
            max_iterations: Some(10),
            started_at: Utc::now(),
            last_iteration_at: Some(Utc::now()),
            error_count: 3,
            consecutive_errors: 2,
            last_error: Some("Test error".to_string()),
            last_commit: None,
            idle_iterations: 0,
        };

        state.save(dir.path()).unwrap();
        let loaded = RalphState::load(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.error_count, 3);
        assert_eq!(loaded.consecutive_errors, 2);
        assert_eq!(loaded.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_state_backward_compatibility() {
        // Test that old state files without error/idle fields can still be loaded
        // Note: DateTime values must be quoted as strings for TOML deserialization
        let old_state_toml = r#"
active = true
mode = "build"
iteration = 5
max_iterations = 10
started_at = "2024-01-01T12:00:00Z"
last_iteration_at = "2024-01-01T12:05:00Z"
"#;

        let state: RalphState = toml::from_str(old_state_toml).unwrap();
        assert_eq!(state.error_count, 0); // Should default to 0
        assert_eq!(state.consecutive_errors, 0); // Should default to 0
        assert!(state.last_error.is_none()); // Should default to None
        assert!(state.last_commit.is_none()); // Should default to None
        assert_eq!(state.idle_iterations, 0); // Should default to 0
    }

    #[test]
    fn test_state_with_idle_detection() {
        let dir = tempdir().unwrap();
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 5,
            max_iterations: Some(10),
            started_at: Utc::now(),
            last_iteration_at: Some(Utc::now()),
            error_count: 0,
            consecutive_errors: 0,
            last_error: None,
            last_commit: Some("abc123def456".to_string()),
            idle_iterations: 1,
        };

        state.save(dir.path()).unwrap();
        let loaded = RalphState::load(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.last_commit, Some("abc123def456".to_string()));
        assert_eq!(loaded.idle_iterations, 1);
    }
}
