//! Agent providers for different AI CLI tools
//!
//! This module provides a unified interface for invoking different AI agent CLIs:
//! - Cursor: `agent -p "prompt"`
//! - Claude: `claude -p --dangerously-skip-permissions`
//!
//! The provider is selected via `[agent].provider` in ralph.toml.

mod claude;
mod cursor;
#[cfg(test)]
pub(crate) mod mock;

pub(crate) use claude::ClaudeProvider;
pub(crate) use cursor::CursorProvider;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Trait for AI agent CLI providers.
#[async_trait]
pub(crate) trait AgentProvider: Send + Sync {
    /// Returns the provider name for display.
    fn name(&self) -> &'static str;

    /// Invokes the agent with a prompt and returns the output.
    async fn invoke(&self, project_dir: &Path, prompt: &str) -> Result<String>;
}

/// Supported agent providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Provider {
    /// Cursor CLI agent.
    #[default]
    Cursor,
    /// Claude Code CLI agent.
    Claude,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cursor => write!(f, "cursor"),
            Self::Claude => write!(f, "claude"),
        }
    }
}

impl std::str::FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cursor" => Ok(Self::Cursor),
            "claude" => Ok(Self::Claude),
            _ => anyhow::bail!("Unknown agent provider: '{s}'. Supported: cursor, claude"),
        }
    }
}

/// Creates a mock executable script for testing.
/// Handles the "Text file busy" (ETXTBSY) race condition that can occur
/// when creating and immediately executing scripts, especially in release mode.
#[cfg(test)]
pub(crate) fn create_mock_executable(path: &std::path::Path, script: &[u8]) {
    use std::io::Write;

    // Write script content
    {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(script).unwrap();
        file.sync_all().unwrap();
    } // file is dropped (closed) here

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Sync parent directory to ensure inode metadata is flushed.
    // This prevents ETXTBSY errors when tests run in parallel under high load.
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_display() {
        assert_eq!(format!("{}", Provider::Cursor), "cursor");
        assert_eq!(format!("{}", Provider::Claude), "claude");
    }

    #[test]
    fn test_provider_from_str() {
        assert_eq!("cursor".parse::<Provider>().unwrap(), Provider::Cursor);
        assert_eq!("claude".parse::<Provider>().unwrap(), Provider::Claude);
        assert_eq!("Claude".parse::<Provider>().unwrap(), Provider::Claude);
        assert!("unknown".parse::<Provider>().is_err());
    }
}
