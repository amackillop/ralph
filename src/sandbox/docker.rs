// Sandbox is not yet integrated with the multi-provider system
// Will be connected once the provider system is stable
#![allow(dead_code)]

use anyhow::{Context, Result};
use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, LogOutput, RemoveContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures_util::StreamExt;
use std::path::Path;
use tracing::{debug, info, warn};

use crate::config::Config;

/// Runs Cursor inside a Docker container for isolation
pub struct SandboxRunner {
    config: Config,
}

impl SandboxRunner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Run Cursor in a sandboxed container
    pub async fn run(&self, project_dir: &Path, prompt: &str) -> Result<String> {
        info!("Running Cursor in Docker sandbox");

        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker. Is Docker running?")?;

        // Check if Docker is accessible
        docker
            .ping()
            .await
            .context("Cannot ping Docker daemon. Is Docker running?")?;

        let container_name = format!(
            "ralph-{}",
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );

        // Write prompt to temp file in project dir
        let prompt_file = project_dir.join(".cursor").join("ralph-prompt.tmp");
        std::fs::create_dir_all(prompt_file.parent().unwrap())?;
        std::fs::write(&prompt_file, prompt)?;

        // Build container configuration
        let container_config = self.build_container_config(project_dir)?;

        // Create container
        debug!("Creating container: {}", container_name);
        docker
            .create_container(
                Some(CreateContainerOptions {
                    name: container_name.clone(),
                    platform: None,
                }),
                container_config,
            )
            .await
            .context("Failed to create container")?;

        // Start container
        debug!("Starting container");
        docker
            .start_container::<String>(&container_name, None)
            .await
            .context("Failed to start container")?;

        // Execute Cursor inside container
        let output = self.exec_cursor(&docker, &container_name).await;

        // Clean up container
        debug!("Removing container");
        let _ = docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        // Clean up temp file
        let _ = std::fs::remove_file(&prompt_file);

        output
    }

    fn build_container_config(&self, project_dir: &Path) -> Result<ContainerConfig<String>> {
        let sandbox = &self.config.sandbox;

        // Build volume bindings
        let mut binds = vec![
            // Mount workspace read-write
            format!(
                "{}:/workspace:rw",
                project_dir.to_str().context("Invalid project path")?
            ),
        ];

        // Add configured mounts
        for mount in &sandbox.mounts {
            let host_path = expand_path(&mount.host)?;
            let mode = if mount.readonly { "ro" } else { "rw" };
            binds.push(format!("{}:{}:{}", host_path, mount.container, mode));
        }

        // Add default credential mounts if they exist
        if let Some(home) = dirs::home_dir() {
            let ssh_dir = home.join(".ssh");
            if ssh_dir.exists() {
                binds.push(format!("{}:/root/.ssh:ro", ssh_dir.display()));
            }

            let gitconfig = home.join(".gitconfig");
            if gitconfig.exists() {
                binds.push(format!("{}:/root/.gitconfig:ro", gitconfig.display()));
            }
        }

        // Parse resource limits
        let memory = parse_memory_limit(&sandbox.resources.memory)?;
        let cpus = sandbox.resources.cpus.parse::<f64>().unwrap_or(4.0);

        let mut config = ContainerConfig {
            image: Some(sandbox.image.clone()),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(bollard::service::HostConfig {
                binds: Some(binds),
                memory: Some(memory),
                nano_cpus: Some((cpus * 1_000_000_000.0) as i64),
                dns: Some(sandbox.network.dns.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Apply network policy
        match sandbox.network.policy {
            crate::config::NetworkPolicy::Deny => {
                if let Some(ref mut host_config) = config.host_config {
                    host_config.network_mode = Some("none".to_string());
                }
            }
            crate::config::NetworkPolicy::Allowlist => {
                // For allowlist, we'd need to set up iptables rules or use a custom network
                // For now, just warn that it's not fully implemented
                warn!("Allowlist network policy is not fully implemented yet. Using allow-all.");
            }
            crate::config::NetworkPolicy::AllowAll => {
                // Default bridge network allows all
            }
        }

        Ok(config)
    }

    async fn exec_cursor(&self, docker: &Docker, container_name: &str) -> Result<String> {
        let exec = docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    cmd: Some(vec![
                        "cursor",
                        "--background",
                        "--folder",
                        "/workspace",
                        "--prompt-file",
                        "/workspace/.cursor/ralph-prompt.tmp",
                    ]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("Failed to create exec")?;

        let mut output = String::new();

        if let StartExecResults::Attached {
            output: mut stream, ..
        } = docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start exec")?
        {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(LogOutput::StdOut { message }) => {
                        output.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(LogOutput::StdErr { message }) => {
                        debug!("stderr: {}", String::from_utf8_lossy(&message));
                    }
                    Err(e) => {
                        warn!("Error reading exec output: {}", e);
                    }
                    _ => {}
                }
            }
        }

        info!("Container execution completed");
        Ok(output)
    }
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> Result<String> {
    if path.starts_with("~/") {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(path.replacen("~", home.to_str().unwrap(), 1))
    } else {
        Ok(path.to_string())
    }
}

/// Parse memory limit string (e.g., "8g", "512m") to bytes
fn parse_memory_limit(limit: &str) -> Result<i64> {
    let limit = limit.to_lowercase();

    if let Some(num) = limit.strip_suffix('g') {
        let gigs: i64 = num.parse().context("Invalid memory limit")?;
        Ok(gigs * 1024 * 1024 * 1024)
    } else if let Some(num) = limit.strip_suffix('m') {
        let megs: i64 = num.parse().context("Invalid memory limit")?;
        Ok(megs * 1024 * 1024)
    } else {
        limit.parse().context("Invalid memory limit")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_limit() {
        assert_eq!(parse_memory_limit("8g").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_limit("512m").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory_limit("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_expand_path() {
        // Test non-tilde path
        assert_eq!(expand_path("/usr/bin").unwrap(), "/usr/bin");

        // Test tilde expansion (only works if home dir is set)
        if dirs::home_dir().is_some() {
            let expanded = expand_path("~/.ssh").unwrap();
            assert!(!expanded.starts_with("~"));
            assert!(expanded.ends_with("/.ssh"));
        }
    }
}
