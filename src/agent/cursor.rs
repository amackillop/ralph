//! Cursor agent provider
//!
//! Invokes the Cursor CLI agent in print mode:
//! ```bash
//! agent -p "prompt" --output-format text
//! ```
//!
//! See: https://cursor.com/docs/cli/overview

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tracing::{debug, info, warn};

use super::AgentProvider;
use crate::config::CursorConfig;

/// Cursor CLI agent provider
pub struct CursorProvider {
    config: CursorConfig,
}

impl CursorProvider {
    pub fn new(config: CursorConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentProvider for CursorProvider {
    fn name(&self) -> &'static str {
        "Cursor"
    }

    async fn invoke(&self, project_dir: &Path, prompt: &str) -> Result<String> {
        let agent_path = &self.config.path;
        info!("Running Cursor agent: {}", agent_path);
        debug!("Project dir: {}", project_dir.display());

        // Build command arguments for print mode
        // agent -p "prompt" [--model "model"] --output-format text
        let mut args = vec!["-p".to_string(), prompt.to_string()];

        // Add model if configured
        if let Some(ref model) = self.config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        // Add output format
        args.push("--output-format".to_string());
        args.push(self.config.output_format.clone());

        debug!("Agent args: {:?}", args);

        let output = tokio::process::Command::new(agent_path)
            .current_dir(project_dir)
            .args(&args)
            .output()
            .await
            .with_context(|| {
                format!(
                    "Failed to run Cursor agent '{}'. \n\
                     \n\
                     Make sure the Cursor CLI is installed:\n\
                     - Install: curl https://cursor.com/install -fsS | bash\n\
                     - On NixOS: set [agent.cursor].path = \"cursor-agent\" in ralph.toml\n\
                     - Or specify a full path: [agent.cursor].path = \"/path/to/agent\"\n\
                     \n\
                     See: https://cursor.com/docs/cli/overview",
                    agent_path
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            if stderr.contains("command not found") || stderr.contains("No such file") {
                anyhow::bail!(
                    "Cursor agent '{}' not found.\n\
                     \n\
                     Install the Cursor CLI:\n\
                     - curl https://cursor.com/install -fsS | bash\n\
                     \n\
                     Or configure the path in ralph.toml:\n\
                     [agent.cursor]\n\
                     path = \"cursor-agent\"  # NixOS\n\
                     path = \"/full/path/to/agent\"  # Custom path",
                    agent_path
                );
            }

            warn!("Agent stderr: {}", stderr);
            warn!("Agent stdout: {}", stdout);
            anyhow::bail!(
                "Cursor agent failed with exit code {:?}:\n{}",
                output.status.code(),
                stderr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        info!("Cursor agent completed successfully");
        debug!("Output length: {} bytes", stdout.len());

        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_provider_name() {
        let config = CursorConfig::default();
        let provider = CursorProvider::new(config);
        assert_eq!(provider.name(), "Cursor");
    }
}
