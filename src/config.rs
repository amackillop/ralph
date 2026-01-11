//! Configuration file parsing for `ralph.toml`.
//!
//! Handles loading and parsing of project configuration including agent settings,
//! sandbox configuration, git options, and completion detection.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::agent::Provider;

const CONFIG_FILE: &str = "ralph.toml";

/// Top-level Ralph configuration loaded from `ralph.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Config {
    /// Agent provider configuration.
    #[serde(default)]
    pub agent: AgentConfig,
    /// Docker sandbox configuration.
    #[serde(default)]
    pub sandbox: SandboxConfig,
    /// Git integration settings.
    #[serde(default)]
    pub git: GitConfig,
    /// Completion detection settings.
    #[serde(default)]
    pub completion: CompletionConfig,
}

/// Agent configuration - selects and configures the AI agent CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentConfig {
    /// Which agent provider to use: "cursor" or "claude"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Cursor-specific configuration
    #[serde(default)]
    pub cursor: CursorConfig,

    /// Claude-specific configuration
    #[serde(default)]
    pub claude: ClaudeConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            cursor: CursorConfig::default(),
            claude: ClaudeConfig::default(),
        }
    }
}

impl AgentConfig {
    /// Parse the provider string into a Provider enum
    pub fn get_provider(&self) -> Result<Provider> {
        self.provider.parse()
    }
}

fn default_provider() -> String {
    "cursor".to_string()
}

/// Cursor CLI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CursorConfig {
    /// Path to the Cursor agent CLI
    /// - Default: "agent"
    /// - NixOS: "cursor-agent"
    /// - Custom: "/path/to/agent"
    #[serde(default = "default_cursor_path")]
    pub path: String,

    /// Model to use (optional, uses Cursor's default if not set)
    #[serde(default)]
    pub model: Option<String>,

    /// Output format for non-interactive mode
    #[serde(default = "default_output_format")]
    pub output_format: String,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            path: default_cursor_path(),
            model: None,
            output_format: default_output_format(),
        }
    }
}

fn default_cursor_path() -> String {
    "agent".to_string()
}

fn default_output_format() -> String {
    "text".to_string()
}

/// Claude Code CLI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ClaudeConfig {
    /// Path to the Claude CLI
    /// - Default: "claude"
    /// - Custom: "/path/to/claude"
    #[serde(default = "default_claude_path")]
    pub path: String,

    /// Model to use (optional)
    /// - Default: Cursor's default model
    /// - Examples: "opus", "sonnet"
    #[serde(default)]
    pub model: Option<String>,

    /// Skip permission prompts (required for autonomous operation)
    #[serde(default = "default_true")]
    pub skip_permissions: bool,

    /// Output format
    #[serde(default = "default_claude_output_format")]
    pub output_format: String,

    /// Verbose output
    #[serde(default)]
    pub verbose: bool,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            path: default_claude_path(),
            model: None,
            skip_permissions: true,
            output_format: default_claude_output_format(),
            verbose: false,
        }
    }
}

fn default_claude_path() -> String {
    "claude".to_string()
}

fn default_claude_output_format() -> String {
    "stream-json".to_string()
}

/// Docker sandbox configuration for isolated execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SandboxConfig {
    /// Enable/disable Docker sandboxing.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Docker image to use
    #[serde(default = "default_image")]
    pub image: String,

    /// Additional volume mounts
    #[serde(default)]
    pub mounts: Vec<Mount>,

    /// Network configuration
    #[serde(default)]
    pub network: NetworkConfig,

    /// Resource limits
    #[serde(default)]
    pub resources: ResourceConfig,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            image: default_image(),
            mounts: Vec::new(),
            network: NetworkConfig::default(),
            resources: ResourceConfig::default(),
        }
    }
}

/// Volume mount configuration for Docker containers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Mount {
    /// Host path to mount.
    pub host: String,
    /// Container path to mount to.
    pub container: String,
    /// Whether the mount is read-only.
    #[serde(default = "default_true")]
    pub readonly: bool,
}

/// Network access policy for sandbox containers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NetworkPolicy {
    /// Allow all network access.
    #[default]
    AllowAll,
    /// Only allow access to specified domains.
    Allowlist,
    /// Deny all network access.
    Deny,
}

/// Network configuration for sandbox containers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkConfig {
    /// Network access policy.
    #[serde(default)]
    pub policy: NetworkPolicy,

    /// Allowed domains when policy is `Allowlist`.
    #[serde(default)]
    pub allowed: Vec<String>,

    /// Custom DNS servers.
    #[serde(default = "default_dns")]
    pub dns: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            policy: NetworkPolicy::AllowAll,
            allowed: Vec::new(),
            dns: default_dns(),
        }
    }
}

/// Resource limits for sandbox containers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResourceConfig {
    /// Memory limit (e.g., "8g", "512m").
    #[serde(default = "default_memory")]
    pub memory: String,

    /// CPU limit (e.g., "4", "2.5").
    #[serde(default = "default_cpus")]
    pub cpus: String,

    /// Timeout in minutes before killing the container.
    #[serde(default = "default_timeout")]
    pub timeout_minutes: u32,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            memory: default_memory(),
            cpus: default_cpus(),
            timeout_minutes: default_timeout(),
        }
    }
}

/// Git integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GitConfig {
    /// Automatically push after each iteration.
    #[serde(default = "default_true")]
    pub auto_push: bool,

    /// Branches that should not be modified directly.
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_push: true,
            protected_branches: default_protected_branches(),
        }
    }
}

/// Completion detection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompletionConfig {
    /// Format template for completion promises (use `{}` as placeholder).
    #[serde(default = "default_promise_format")]
    pub promise_format: String,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            promise_format: default_promise_format(),
        }
    }
}

// Default value functions
fn default_true() -> bool {
    true
}

fn default_image() -> String {
    "ralph:latest".to_string()
}

fn default_dns() -> Vec<String> {
    vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()]
}

fn default_memory() -> String {
    "8g".to_string()
}

fn default_cpus() -> String {
    "4".to_string()
}

fn default_timeout() -> u32 {
    60
}

fn default_protected_branches() -> Vec<String> {
    vec![
        "main".to_string(),
        "master".to_string(),
        "production".to_string(),
    ]
}

fn default_promise_format() -> String {
    "<promise>{}</promise>".to_string()
}

impl Config {
    /// Load configuration from file, using defaults if not found
    pub fn load(project_dir: &Path) -> Result<Self> {
        let config_path = project_dir.join(CONFIG_FILE);

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        Ok(config)
    }

    /// Check if current branch is protected
    #[allow(dead_code)]
    pub fn is_protected_branch(&self, branch: &str) -> bool {
        self.git.protected_branches.iter().any(|b| b == branch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.sandbox.enabled);
        assert!(config.git.auto_push);
        assert!(config.git.protected_branches.contains(&"main".to_string()));
        assert_eq!(config.agent.provider, "cursor");
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[agent]
provider = "claude"

[agent.claude]
path = "/usr/bin/claude"
skip_permissions = true

[sandbox]
enabled = false

[sandbox.network]
policy = "allowlist"
allowed = ["github.com"]

[git]
auto_push = false
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(!config.sandbox.enabled);
        assert!(!config.git.auto_push);
        assert_eq!(config.agent.provider, "claude");
        assert_eq!(config.agent.claude.path, "/usr/bin/claude");
        assert!(matches!(
            config.sandbox.network.policy,
            NetworkPolicy::Allowlist
        ));
    }

    #[test]
    fn test_cursor_config() {
        let toml = r#"
[agent]
provider = "cursor"

[agent.cursor]
path = "cursor-agent"
model = "gpt-5"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.agent.provider, "cursor");
        assert_eq!(config.agent.cursor.path, "cursor-agent");
        assert_eq!(config.agent.cursor.model, Some("gpt-5".to_string()));
    }
}
