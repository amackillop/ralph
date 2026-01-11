//! Claude Code agent provider
//!
//! Invokes the Claude CLI in print mode:
//! ```bash
//! claude -p --dangerously-skip-permissions --model opus --output-format stream-json
//! ```
//!
//! The prompt is piped via stdin.
//!
//! See: https://docs.anthropic.com/en/docs/claude-code

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

use super::AgentProvider;
use crate::config::ClaudeConfig;

/// Claude Code CLI agent provider
pub struct ClaudeProvider {
    config: ClaudeConfig,
}

impl ClaudeProvider {
    pub fn new(config: ClaudeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentProvider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "Claude"
    }

    async fn invoke(&self, project_dir: &Path, prompt: &str) -> Result<String> {
        let claude_path = &self.config.path;
        info!("Running Claude agent: {}", claude_path);
        debug!("Project dir: {}", project_dir.display());

        // Build command arguments
        // claude -p [--dangerously-skip-permissions] [--model model] [--output-format format]
        let mut args = vec!["-p".to_string()];

        // Add dangerous skip permissions flag (required for autonomous operation)
        if self.config.skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }

        // Add model if configured
        if let Some(ref model) = self.config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        // Add output format
        args.push("--output-format".to_string());
        args.push(self.config.output_format.clone());

        // Add verbose flag if configured
        if self.config.verbose {
            args.push("--verbose".to_string());
        }

        debug!("Claude args: {:?}", args);

        // Claude reads prompt from stdin
        let mut child = tokio::process::Command::new(claude_path)
            .current_dir(project_dir)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to run Claude agent '{}'. \n\
                     \n\
                     Make sure Claude Code CLI is installed:\n\
                     - Install: npm install -g @anthropic-ai/claude-code\n\
                     - Or: brew install claude-code\n\
                     \n\
                     Configure the path in ralph.toml:\n\
                     [agent.claude]\n\
                     path = \"claude\"  # Default\n\
                     path = \"/full/path/to/claude\"  # Custom path",
                    claude_path
                )
            })?;

        // Write prompt to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.flush().await?;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            if stderr.contains("command not found") || stderr.contains("No such file") {
                anyhow::bail!(
                    "Claude agent '{}' not found.\n\
                     \n\
                     Install Claude Code CLI:\n\
                     - npm install -g @anthropic-ai/claude-code\n\
                     \n\
                     Or configure the path in ralph.toml:\n\
                     [agent.claude]\n\
                     path = \"/full/path/to/claude\"",
                    claude_path
                );
            }

            warn!("Agent stderr: {}", stderr);
            warn!("Agent stdout: {}", stdout);
            anyhow::bail!(
                "Claude agent failed with exit code {:?}:\n{}",
                output.status.code(),
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        info!("Claude agent completed successfully");
        debug!("Output length: {} bytes", stdout.len());

        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_provider_name() {
        let config = ClaudeConfig::default();
        let provider = ClaudeProvider::new(config);
        assert_eq!(provider.name(), "Claude");
    }
}
