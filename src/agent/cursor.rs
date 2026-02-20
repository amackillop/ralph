//! Cursor agent provider
//!
//! Invokes the Cursor CLI agent in print mode:
//! ```bash
//! agent -p "prompt" --output-format text
//! ```
//!
//! See: <https://cursor.com/docs/cli/overview>

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use tracing::{debug, info, warn};

use super::AgentProvider;
use crate::config::CursorConfig;

/// Cursor CLI agent provider.
pub(crate) struct CursorProvider {
    config: CursorConfig,
}

impl CursorProvider {
    /// Creates a new Cursor provider with the given configuration.
    pub(crate) fn new(config: CursorConfig) -> Self {
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
        // agent -p "prompt" [--model "model"] [--sandbox mode] --output-format text
        let mut args = vec!["-p".to_string(), prompt.to_string()];

        // Add model if configured
        if let Some(ref model) = self.config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        // Add sandbox mode (disabled by default to allow shell access for validation)
        if !self.config.sandbox.is_empty() {
            args.push("--sandbox".to_string());
            args.push(self.config.sandbox.clone());
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
                    "Failed to run Cursor agent '{agent_path}'. \n\
                     \n\
                     Make sure the Cursor CLI is installed:\n\
                     - Install: curl https://cursor.com/install -fsS | bash\n\
                     - On NixOS: set [agent.cursor].path = \"cursor-agent\" in ralph.toml\n\
                     - Or specify a full path: [agent.cursor].path = \"/path/to/agent\"\n\
                     \n\
                     See: https://cursor.com/docs/cli/overview"
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            if stderr.contains("command not found") || stderr.contains("No such file") {
                anyhow::bail!(
                    "Cursor agent '{agent_path}' not found.\n\
                     \n\
                     Install the Cursor CLI:\n\
                     - curl https://cursor.com/install -fsS | bash\n\
                     \n\
                     Or configure the path in ralph.toml:\n\
                     [agent.cursor]\n\
                     path = \"cursor-agent\"  # NixOS\n\
                     path = \"/full/path/to/agent\"  # Custom path"
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

    #[test]
    fn test_cursor_provider_new() {
        let config = CursorConfig {
            path: "/custom/agent".to_string(),
            model: Some("gpt-4".to_string()),
            sandbox: "on".to_string(),
            output_format: "json".to_string(),
            timeout_minutes: Some(30),
        };
        let provider = CursorProvider::new(config.clone());
        assert_eq!(provider.config.path, "/custom/agent");
        assert_eq!(provider.config.model, Some("gpt-4".to_string()));
        assert_eq!(provider.config.sandbox, "on");
        assert_eq!(provider.config.output_format, "json");
        assert_eq!(provider.config.timeout_minutes, Some(30));
    }

    #[test]
    fn test_cursor_provider_default_config() {
        let config = CursorConfig::default();
        assert_eq!(config.path, "agent");
        assert!(config.model.is_none());
        assert_eq!(config.sandbox, "disabled");
        assert_eq!(config.output_format, "text");
    }

    /// Test argument building logic
    fn build_args(config: &CursorConfig, prompt: &str) -> Vec<String> {
        let mut args = vec!["-p".to_string(), prompt.to_string()];
        if let Some(ref model) = config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        if !config.sandbox.is_empty() {
            args.push("--sandbox".to_string());
            args.push(config.sandbox.clone());
        }
        args.push("--output-format".to_string());
        args.push(config.output_format.clone());
        args
    }

    #[test]
    fn test_build_args_default() {
        let config = CursorConfig::default();
        let args = build_args(&config, "test prompt");
        assert_eq!(args[0], "-p");
        assert_eq!(args[1], "test prompt");
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(args.contains(&"disabled".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"text".to_string()));
        assert!(!args.contains(&"--model".to_string()));
    }

    #[test]
    fn test_build_args_with_model() {
        let config = CursorConfig {
            model: Some("gpt-4".to_string()),
            ..Default::default()
        };
        let args = build_args(&config, "prompt");
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"gpt-4".to_string()));
    }

    #[test]
    fn test_build_args_empty_sandbox() {
        let config = CursorConfig {
            sandbox: String::new(),
            ..Default::default()
        };
        let args = build_args(&config, "prompt");
        assert!(!args.contains(&"--sandbox".to_string()));
    }

    #[test]
    fn test_build_args_custom_output_format() {
        let config = CursorConfig {
            output_format: "json".to_string(),
            ..Default::default()
        };
        let args = build_args(&config, "prompt");
        assert!(args.contains(&"json".to_string()));
    }

    #[test]
    fn test_build_args_preserves_prompt() {
        let config = CursorConfig::default();
        let prompt = "This is a complex\nmultiline\nprompt with special chars: ${}";
        let args = build_args(&config, prompt);
        assert_eq!(args[1], prompt);
    }

    #[tokio::test]
    async fn test_invoke_nonexistent_binary() {
        let config = CursorConfig {
            path: "/nonexistent/path/cursor-fake-binary".to_string(),
            ..Default::default()
        };
        let provider = CursorProvider::new(config);
        let result = provider
            .invoke(std::path::Path::new("/tmp"), "test prompt")
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to run Cursor agent"));
    }

    #[tokio::test]
    async fn test_invoke_with_mock_binary_success() {
        // Create a mock binary that echoes the prompt arg (second arg after -p)
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_path = temp_dir.path().join("mock-cursor");

        // Shell script: echo the second argument (the prompt after -p)
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&mock_path).unwrap();
            file.write_all(b"#!/bin/sh\necho \"$2\"\n").unwrap();
            file.sync_all().unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&mock_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let config = CursorConfig {
            path: mock_path.to_str().unwrap().to_string(),
            sandbox: String::new(), // Don't add --sandbox flag
            ..Default::default()
        };
        let provider = CursorProvider::new(config);

        let result = provider
            .invoke(temp_dir.path(), "test prompt from args")
            .await;

        assert!(result.is_ok(), "Expected success, got: {result:?}");
        // Note: echo adds a newline
        assert_eq!(result.unwrap().trim(), "test prompt from args");
    }

    #[tokio::test]
    async fn test_invoke_with_mock_binary_failure() {
        // Create a mock binary that fails with exit code 1
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_path = temp_dir.path().join("mock-cursor-fail");

        {
            use std::io::Write;
            let mut file = std::fs::File::create(&mock_path).unwrap();
            file.write_all(b"#!/bin/sh\necho 'Cursor error' >&2\nexit 1\n")
                .unwrap();
            file.sync_all().unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&mock_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let config = CursorConfig {
            path: mock_path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        let provider = CursorProvider::new(config);

        let result = provider.invoke(temp_dir.path(), "test").await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed with exit code"));
        assert!(err.contains("Cursor error"));
    }

    #[tokio::test]
    async fn test_invoke_uses_correct_working_directory() {
        // Mock binary that outputs the current working directory
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_path = temp_dir.path().join("mock-cursor-pwd");

        {
            use std::io::Write;
            let mut file = std::fs::File::create(&mock_path).unwrap();
            file.write_all(b"#!/bin/sh\npwd\n").unwrap();
            file.sync_all().unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&mock_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let config = CursorConfig {
            path: mock_path.to_str().unwrap().to_string(),
            ..Default::default()
        };
        let provider = CursorProvider::new(config);

        // Use a specific subdirectory as project dir
        let project_dir = temp_dir.path().join("workspace");
        std::fs::create_dir(&project_dir).unwrap();

        let result = provider.invoke(&project_dir, "ignored").await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.trim().ends_with("workspace"),
            "Expected working dir to be workspace, got: {output}"
        );
    }
}
