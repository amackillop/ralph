use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const STATE_FILE: &str = ".cursor/ralph-state.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Plan,
    Build,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphState {
    pub active: bool,
    pub mode: Mode,
    pub iteration: u32,
    pub max_iterations: Option<u32>,
    pub completion_promise: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_iteration_at: Option<DateTime<Utc>>,
}

impl Default for RalphState {
    fn default() -> Self {
        Self {
            active: false,
            mode: Mode::Build,
            iteration: 1,
            max_iterations: None,
            completion_promise: None,
            started_at: Utc::now(),
            last_iteration_at: None,
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

        // Ensure .cursor directory exists
        if let Some(parent) = state_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize state")?;

        fs::write(&state_path, content)
            .with_context(|| format!("Failed to write state file: {}", state_path.display()))?;

        Ok(())
    }

    /// Delete state file
    #[allow(dead_code)]
    pub fn delete(project_dir: &Path) -> Result<bool> {
        let state_path = project_dir.join(STATE_FILE);

        if state_path.exists() {
            fs::remove_file(&state_path).with_context(|| {
                format!("Failed to delete state file: {}", state_path.display())
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_state_roundtrip() {
        let dir = tempdir().unwrap();
        let state = RalphState {
            active: true,
            mode: Mode::Build,
            iteration: 5,
            max_iterations: Some(20),
            completion_promise: Some("DONE".to_string()),
            started_at: Utc::now(),
            last_iteration_at: Some(Utc::now()),
        };

        state.save(dir.path()).unwrap();
        let loaded = RalphState::load(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.active, state.active);
        assert_eq!(loaded.mode, state.mode);
        assert_eq!(loaded.iteration, state.iteration);
        assert_eq!(loaded.max_iterations, state.max_iterations);
        assert_eq!(loaded.completion_promise, state.completion_promise);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempdir().unwrap();
        let result = RalphState::load(dir.path()).unwrap();
        assert!(result.is_none());
    }
}
