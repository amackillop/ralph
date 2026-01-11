//! Agent providers for different AI CLI tools
//!
//! This module provides a unified interface for invoking different AI agent CLIs:
//! - Cursor: `agent -p "prompt"`
//! - Claude: `claude -p --dangerously-skip-permissions`
//!
//! The provider is selected via `[agent].provider` in ralph.toml.

mod claude;
mod cursor;

pub use claude::ClaudeProvider;
pub use cursor::CursorProvider;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Trait for AI agent CLI providers
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Provider name for display
    fn name(&self) -> &'static str;

    /// Invoke the agent with a prompt and return the output
    async fn invoke(&self, project_dir: &Path, prompt: &str) -> Result<String>;
}

/// Supported agent providers
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Provider {
    #[default]
    Cursor,
    Claude,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Cursor => write!(f, "cursor"),
            Provider::Claude => write!(f, "claude"),
        }
    }
}

impl std::str::FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cursor" => Ok(Provider::Cursor),
            "claude" => Ok(Provider::Claude),
            _ => anyhow::bail!("Unknown agent provider: '{}'. Supported: cursor, claude", s),
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
