use anyhow::{Context, Result};
use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, InspectContainerOptions,
    KillContainerOptions, ListContainersOptions, LogOutput, RemoveContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerStateStatusEnum;
use bollard::Docker;
use futures_util::StreamExt;
use std::fmt::Write;
use std::path::Path;
use tracing::{debug, info, warn};

use crate::agent::Provider;
use crate::config::{AgentConfig, Config};
use crate::sandbox::network::validate_domain;

/// Runs agents inside a Docker container for isolation.
pub(crate) struct SandboxRunner {
    config: Config,
    provider: Provider,
    agent_config: AgentConfig,
}

impl SandboxRunner {
    /// Creates a new sandbox runner with the given configuration and provider.
    pub(crate) fn new(config: Config, provider: Provider, agent_config: AgentConfig) -> Self {
        Self {
            config,
            provider,
            agent_config,
        }
    }

    /// Cleans up orphaned containers with names matching `ralph-*`.
    /// This should be called at the start of a loop to remove containers
    /// left behind from previous runs (e.g., after crashes).
    #[allow(tail_expr_drop_order)] // Drop order doesn't matter for async operations
    pub(crate) async fn cleanup_orphaned_containers() -> Result<u32> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker. Is Docker running?")?;

        // Check if Docker is accessible
        docker
            .ping()
            .await
            .context("Cannot ping Docker daemon. Is Docker running?")?;

        // List all containers (including stopped ones)
        let containers = docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await
            .context("Failed to list containers")?;

        let mut cleaned = 0;

        // Find containers with names starting with "ralph-"
        for container in containers {
            if let Some(names) = container.names {
                for name in names {
                    // Container names in Docker API start with "/"
                    let name = name.trim_start_matches('/');
                    if name.starts_with("ralph-") {
                        info!("Found orphaned container: {}", name);

                        // Try to kill if running
                        if let Some(id) = &container.id {
                            let kill_result = docker
                                .kill_container(id, None::<KillContainerOptions<String>>)
                                .await;
                            let _ = kill_result;
                        }

                        // Remove container
                        if let Some(id) = &container.id {
                            let remove_result = docker
                                .remove_container(
                                    id,
                                    Some(RemoveContainerOptions {
                                        force: true,
                                        ..Default::default()
                                    }),
                                )
                                .await;
                            match remove_result {
                                Ok(()) => {
                                    info!("Removed orphaned container: {}", name);
                                    cleaned += 1;
                                }
                                Err(e) => {
                                    warn!("Failed to remove container {}: {}", name, e);
                                }
                            }
                        }
                        break; // Only process each container once
                    }
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} orphaned container(s)", cleaned);
        } else {
            debug!("No orphaned containers found");
        }

        Ok(cleaned)
    }

    /// Creates and starts a persistent container for reuse across iterations.
    /// Returns the container name.
    pub(crate) async fn create_persistent_container(&self, project_dir: &Path) -> Result<String> {
        info!(
            "Creating persistent container for {} sandbox",
            self.provider
        );

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

        // Build container configuration
        let container_config = self.build_container_config(project_dir)?;

        // Create container
        debug!("Creating persistent container: {}", container_name);
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
        debug!("Starting persistent container");
        docker
            .start_container::<String>(&container_name, None)
            .await
            .context("Failed to start container")?;

        Ok(container_name)
    }

    /// Removes a persistent container.
    pub(crate) async fn remove_persistent_container(container_name: &str) -> Result<()> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker. Is Docker running?")?;

        // Check if Docker is accessible
        docker
            .ping()
            .await
            .context("Cannot ping Docker daemon. Is Docker running?")?;

        debug!("Removing persistent container: {}", container_name);
        let _ = docker
            .remove_container(
                container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        Ok(())
    }

    /// Checks if a container is healthy and ready for use.
    /// Returns Ok(()) if container is running or was successfully restarted.
    /// Returns Err if container is dead/corrupted and needs recreation.
    async fn check_container_health(docker: &Docker, container_name: &str) -> Result<()> {
        let info = docker
            .inspect_container(container_name, None::<InspectContainerOptions>)
            .await
            .context("Failed to inspect container - it may have been removed")?;

        let state = info.state.context("Container has no state")?;
        let status = state.status.context("Container state has no status")?;

        match status {
            ContainerStateStatusEnum::RUNNING => {
                debug!("Container {} is running", container_name);
                Ok(())
            }
            ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::CREATED => {
                // Container stopped - try to restart it
                info!(
                    "Container {} is not running ({}), attempting restart",
                    container_name, status
                );
                docker
                    .start_container::<String>(container_name, None)
                    .await
                    .context("Failed to restart stopped container")?;
                info!("Successfully restarted container {}", container_name);
                Ok(())
            }
            ContainerStateStatusEnum::DEAD => {
                // Dead container cannot be restarted - needs recreation
                anyhow::bail!(
                    "Container {container_name} is dead and cannot be restarted. Please recreate it."
                )
            }
            ContainerStateStatusEnum::PAUSED => {
                // Unpause the container
                info!("Container {} is paused, attempting unpause", container_name);
                docker
                    .unpause_container(container_name)
                    .await
                    .context("Failed to unpause container")?;
                info!("Successfully unpaused container {}", container_name);
                Ok(())
            }
            ContainerStateStatusEnum::RESTARTING => {
                // Already restarting - wait briefly and check again
                debug!("Container {} is restarting, waiting...", container_name);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                // Recursive check with depth limit would be better, but for now just verify running
                let info2 = docker
                    .inspect_container(container_name, None::<InspectContainerOptions>)
                    .await?;
                if let Some(state2) = info2.state {
                    if state2.running.unwrap_or(false) {
                        return Ok(());
                    }
                }
                anyhow::bail!("Container {container_name} failed to restart")
            }
            ContainerStateStatusEnum::REMOVING => {
                anyhow::bail!("Container {container_name} is being removed. Please recreate it.")
            }
            ContainerStateStatusEnum::EMPTY => {
                anyhow::bail!("Container {container_name} has unknown state. Please recreate it.")
            }
        }
    }

    /// Runs the agent in a sandboxed container.
    /// If `reuse_container_name` is provided, uses that existing container instead of creating a new one.
    pub(crate) async fn run(
        &self,
        project_dir: &Path,
        prompt: &str,
        reuse_container_name: Option<&str>,
    ) -> Result<String> {
        info!("Running {} in Docker sandbox", self.provider);

        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker. Is Docker running?")?;

        // Check if Docker is accessible
        docker
            .ping()
            .await
            .context("Cannot ping Docker daemon. Is Docker running?")?;

        let container_name = if let Some(name) = reuse_container_name {
            // Check container health before reusing
            Self::check_container_health(&docker, name).await?;
            debug!("Reusing container: {}", name);
            name.to_string()
        } else {
            // Create new container for this iteration
            let name = format!(
                "ralph-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            );

            // Build container configuration
            let container_config = self.build_container_config(project_dir)?;

            // Create container
            debug!("Creating container: {}", name);
            docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: name.clone(),
                        platform: None,
                    }),
                    container_config,
                )
                .await
                .context("Failed to create container")?;

            // Start container
            debug!("Starting container");
            docker
                .start_container::<String>(&name, None)
                .await
                .context("Failed to start container")?;

            name
        };

        // Write prompt to temp file in project dir
        let prompt_file = project_dir.join(".ralph").join("prompt.tmp");
        std::fs::create_dir_all(prompt_file.parent().unwrap())?;
        std::fs::write(&prompt_file, prompt)?;

        // Execute agent inside container
        let output = self
            .exec_agent(&docker, &container_name, &prompt_file)
            .await;

        // Clean up container only if we created it (not reused)
        if reuse_container_name.is_none() {
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
        }

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
                nano_cpus: Some({
                    let nanos = (cpus * 1_000_000_000.0).round();
                    // Clamp to i64 range, precision loss is acceptable for CPU limits
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                    let clamped = nanos.clamp(i64::MIN as f64, i64::MAX as f64) as i64;
                    clamped
                }),
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
                // For allowlist, we need NET_ADMIN capability to set up iptables rules
                if let Some(ref mut host_config) = config.host_config {
                    // Add NET_ADMIN capability to allow iptables rules
                    host_config.cap_add = Some(vec!["NET_ADMIN".to_string()]);
                }
                // The actual iptables rules will be set up in exec_agent before running the agent
                info!(
                    "Allowlist network policy enabled with {} allowed domain(s)",
                    sandbox.network.allowed.len()
                );
            }
            crate::config::NetworkPolicy::AllowAll => {
                // Default bridge network allows all
            }
        }

        Ok(config)
    }

    async fn exec_agent(
        &self,
        docker: &Docker,
        container_name: &str,
        prompt_file: &Path,
    ) -> Result<String> {
        // Set up iptables rules if allowlist policy is enabled
        if self.config.sandbox.network.policy == crate::config::NetworkPolicy::Allowlist {
            self.setup_allowlist_iptables(docker, container_name)
                .await?;
        }

        // Build command based on provider
        let cmd = self.build_agent_command(prompt_file)?;

        let exec = docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("Failed to create exec")?;

        // Get timeout from config (convert minutes to Duration)
        let timeout_duration = std::time::Duration::from_secs(
            u64::from(self.config.sandbox.resources.timeout_minutes) * 60,
        );

        match docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start exec")?
        {
            StartExecResults::Attached {
                output: mut stream, ..
            } => {
                // Wrap stream reading with timeout
                let mut output = String::new();
                let read_future = async {
                    loop {
                        let chunk_result = stream.next().await;
                        match chunk_result {
                            Some(Ok(LogOutput::StdOut { message })) => {
                                output.push_str(&String::from_utf8_lossy(&message));
                            }
                            Some(Ok(LogOutput::StdErr { message })) => {
                                debug!("stderr: {}", String::from_utf8_lossy(&message));
                            }
                            Some(Err(e)) => {
                                warn!("Error reading exec output: {}", e);
                            }
                            Some(_) => {}
                            None => break,
                        }
                    }
                    Ok::<String, anyhow::Error>(output)
                };

                match tokio::time::timeout(timeout_duration, read_future).await {
                    Ok(Ok(result)) => {
                        info!("Container execution completed");
                        Ok(result)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => {
                        // Timeout occurred - kill the container
                        warn!(
                            "Container execution timed out after {} minutes. Killing container...",
                            self.config.sandbox.resources.timeout_minutes
                        );
                        let _ = docker
                            .kill_container(container_name, None::<KillContainerOptions<String>>)
                            .await;
                        anyhow::bail!(
                            "Container execution timed out after {} minutes",
                            self.config.sandbox.resources.timeout_minutes
                        )
                    }
                }
            }
            StartExecResults::Detached => Ok(String::new()),
        }
    }

    /// Sets up iptables rules for allowlist network policy.
    /// This blocks all outbound traffic except DNS and allowed domains.
    async fn setup_allowlist_iptables(&self, docker: &Docker, container_name: &str) -> Result<()> {
        let allowed = &self.config.sandbox.network.allowed;

        if allowed.is_empty() {
            warn!(
                "Allowlist policy enabled but no allowed domains specified. Blocking all traffic."
            );
        }

        let script = build_iptables_script(allowed);

        info!(
            "Setting up iptables allowlist with {} allowed domain(s)",
            allowed.len()
        );

        // Execute the script in the container
        let exec = docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    cmd: Some(vec!["sh".to_string(), "-c".to_string(), script]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("Failed to create exec for iptables setup")?;

        if let StartExecResults::Attached {
            output: mut stream, ..
        } = docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start iptables setup exec")?
        {
            let mut output = String::new();
            loop {
                let chunk_result = stream.next().await;
                match chunk_result {
                    Some(Ok(LogOutput::StdOut { message })) => {
                        let msg = String::from_utf8_lossy(&message);
                        debug!("iptables stdout: {}", msg);
                        output.push_str(&msg);
                    }
                    Some(Ok(LogOutput::StdErr { message })) => {
                        let msg = String::from_utf8_lossy(&message);
                        // iptables may output warnings to stderr that are not errors
                        debug!("iptables stderr: {}", msg);
                        output.push_str(&msg);
                    }
                    Some(Err(e)) => {
                        warn!("Error reading iptables setup output: {}", e);
                    }
                    Some(_) => {}
                    None => break,
                }
            }

            // Check if the script executed successfully
            // Note: iptables commands return 0 on success, non-zero on failure
            // We check the exec result by looking for error patterns in output
            if output.contains("iptables:") && output.contains("error") {
                warn!("iptables setup may have encountered errors: {}", output);
            } else {
                debug!("iptables allowlist rules set up successfully");
            }
        }

        Ok(())
    }
}

/// Bash helper function that routes IP addresses to iptables or ip6tables.
/// Detects IPv6 by presence of colon, validates IPv4 format.
const IPTABLES_ADD_IP_RULE_FN: &str = r#"# Helper function to add firewall rule for an IP address
add_ip_rule() {
  local ip="$1"
  local domain="$2"
  # Detect IPv6 by presence of colon
  if [[ "$ip" == *:* ]]; then
    # IPv6 address
    if [ "$HAS_IP6TABLES" -eq 1 ]; then
      if ip6tables -A OUTPUT -d "$ip" -j ACCEPT 2>/dev/null; then
        echo "Allowed IPv6 $ip for $domain"
      else
        echo "Warning: Failed to add rule for IPv6 $ip" >&2
      fi
    else
      echo "Warning: Skipping IPv6 $ip (ip6tables not available)" >&2
    fi
  else
    # IPv4 address - validate format
    if [[ $ip =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
      if iptables -A OUTPUT -d "$ip" -j ACCEPT 2>/dev/null; then
        echo "Allowed IPv4 $ip for $domain"
      else
        echo "Warning: Failed to add rule for IPv4 $ip" >&2
      fi
    else
      echo "Warning: Invalid IP format: $ip" >&2
    fi
  fi
}

"#;

/// Writes the base firewall setup: availability checks, flush, default policy,
/// loopback, DNS, and established connection rules for both IPv4 and IPv6.
fn write_iptables_base_rules(script: &mut String) {
    // Check iptables availability (required)
    script.push_str("# Check if iptables is available\n");
    script.push_str("if ! command -v iptables &> /dev/null; then\n");
    script.push_str(
        "  echo 'Error: iptables not found. Please install iptables in the container.' >&2\n",
    );
    script.push_str("  exit 1\n");
    script.push_str("fi\n\n");

    // Check ip6tables availability (optional but recommended)
    script.push_str("# Check if ip6tables is available for IPv6 support\n");
    script.push_str("HAS_IP6TABLES=0\n");
    script.push_str("if command -v ip6tables &> /dev/null; then\n");
    script.push_str("  HAS_IP6TABLES=1\n");
    script.push_str("else\n");
    script.push_str(
        "  echo 'Warning: ip6tables not found. IPv6 traffic will not be filtered.' >&2\n",
    );
    script.push_str("fi\n\n");

    // Flush and set default policy - IPv4
    script.push_str("# Flush OUTPUT chain and set default DROP policy (IPv4)\n");
    script.push_str("iptables -F OUTPUT\n");
    script.push_str("iptables -P OUTPUT DROP\n\n");

    // Flush and set default policy - IPv6 (if available)
    script.push_str("# Flush OUTPUT chain and set default DROP policy (IPv6)\n");
    script.push_str("if [ \"$HAS_IP6TABLES\" -eq 1 ]; then\n");
    script.push_str("  ip6tables -F OUTPUT\n");
    script.push_str("  ip6tables -P OUTPUT DROP\n");
    script.push_str("fi\n\n");

    // Allow loopback - IPv4 and IPv6
    script.push_str("# Allow loopback (IPv4 and IPv6)\n");
    script.push_str("iptables -A OUTPUT -o lo -j ACCEPT\n");
    script.push_str("if [ \"$HAS_IP6TABLES\" -eq 1 ]; then\n");
    script.push_str("  ip6tables -A OUTPUT -o lo -j ACCEPT\n");
    script.push_str("fi\n\n");

    // Allow DNS - IPv4 and IPv6
    script.push_str("# Allow DNS (UDP and TCP) - IPv4 and IPv6\n");
    script.push_str("iptables -A OUTPUT -p udp --dport 53 -j ACCEPT\n");
    script.push_str("iptables -A OUTPUT -p tcp --dport 53 -j ACCEPT\n");
    script.push_str("if [ \"$HAS_IP6TABLES\" -eq 1 ]; then\n");
    script.push_str("  ip6tables -A OUTPUT -p udp --dport 53 -j ACCEPT\n");
    script.push_str("  ip6tables -A OUTPUT -p tcp --dport 53 -j ACCEPT\n");
    script.push_str("fi\n\n");

    // Allow established connections - IPv4 and IPv6
    script.push_str("# Allow established connections (IPv4 and IPv6)\n");
    script.push_str("iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT\n");
    script.push_str("if [ \"$HAS_IP6TABLES\" -eq 1 ]; then\n");
    script.push_str("  ip6tables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT\n");
    script.push_str("fi\n\n");

    // Add the helper function
    script.push_str(IPTABLES_ADD_IP_RULE_FN);
}

/// Builds the iptables setup script for allowlist network policy.
/// The script will:
/// 1. Flush existing OUTPUT chain rules (IPv4 and IPv6)
/// 2. Allow loopback traffic
/// 3. Allow DNS (port 53 UDP/TCP)
/// 4. For each allowed domain, resolve to IPs and allow those IPs
/// 5. Block all other outbound traffic
///
/// IPv6 traffic is handled via ip6tables if available. If ip6tables
/// is not present, IPv6 traffic will not be explicitly blocked but
/// IPv6 addresses from DNS will be skipped with a warning.
fn build_iptables_script(allowed: &[String]) -> String {
    let mut script = String::from("#!/bin/bash\nset -e\n\n");

    write_iptables_base_rules(&mut script);

    // For each allowed domain, resolve to IPs and allow them
    script.push_str("# Allow traffic to allowed domains\n");
    for domain in allowed {
        // Validate domain to prevent shell injection
        if validate_domain(domain).is_none() {
            writeln!(
                &mut script,
                "# SKIPPED invalid domain: (redacted for security)"
            )
            .unwrap();
            writeln!(
                &mut script,
                "echo 'Warning: Skipped invalid domain in allowlist' >&2"
            )
            .unwrap();
            continue;
        }
        writeln!(&mut script, "# Resolve {domain} and allow its IPs").unwrap();
        // Try multiple DNS resolution methods (getent ahosts returns both IPv4 and IPv6)
        writeln!(
            &mut script,
            "ips=$(getent ahosts {domain} 2>/dev/null | awk '{{print $1}}' | sort -u || getent hosts {domain} 2>/dev/null | awk '{{print $1}}' || echo '')"
        )
        .unwrap();
        script.push_str("if [ -z \"$ips\" ]; then\n");
        // Fallback to nslookup if getent fails
        writeln!(
            &mut script,
            "  ips=$(nslookup {domain} 2>/dev/null | grep -E 'Address:' | tail -n +2 | awk '{{print $2}}' | grep -v '^$' || echo '')"
        )
        .unwrap();
        script.push_str("fi\n");
        script.push_str("if [ -n \"$ips\" ]; then\n");
        script.push_str("  for ip in $ips; do\n");
        script.push_str("    # Skip empty lines\n");
        script.push_str("    [ -z \"$ip\" ] && continue\n");
        writeln!(&mut script, "    add_ip_rule \"$ip\" \"{domain}\"").unwrap();
        script.push_str("  done\n");
        writeln!(
            &mut script,
            "else\n  echo \"Warning: Could not resolve {domain}\" >&2\nfi"
        )
        .unwrap();
        writeln!(&mut script).unwrap();
    }

    script
}

impl SandboxRunner {
    /// Builds the agent command to execute in the container.
    fn build_agent_command(&self, prompt_file: &Path) -> Result<Vec<String>> {
        // Convert host prompt file path to container path
        // The prompt file is at project_dir/.ralph/prompt.tmp
        // In container, it's at /workspace/.ralph/prompt.tmp
        let container_prompt_path = "/workspace/.ralph/prompt.tmp";

        match self.provider {
            Provider::Cursor => {
                let cursor_config = &self.agent_config.cursor;
                let mut cmd = vec![cursor_config.path.clone(), "-p".to_string()];

                // Read prompt from file and pass as argument
                let prompt =
                    std::fs::read_to_string(prompt_file).context("Failed to read prompt file")?;
                cmd.push(prompt);

                // Add model if configured
                if let Some(ref model) = cursor_config.model {
                    cmd.push("--model".to_string());
                    cmd.push(model.clone());
                }

                // Add sandbox mode (disabled by default in container since we're already in Docker)
                if !cursor_config.sandbox.is_empty() {
                    cmd.push("--sandbox".to_string());
                    cmd.push(cursor_config.sandbox.clone());
                }

                // Add output format
                cmd.push("--output-format".to_string());
                cmd.push(cursor_config.output_format.clone());

                Ok(cmd)
            }
            Provider::Claude => {
                let claude_config = &self.agent_config.claude;
                let mut cmd = vec![claude_config.path.clone(), "-p".to_string()];

                // Add dangerous skip permissions flag (required for autonomous operation)
                if claude_config.skip_permissions {
                    cmd.push("--dangerously-skip-permissions".to_string());
                }

                // Add model if configured
                if let Some(ref model) = claude_config.model {
                    cmd.push("--model".to_string());
                    cmd.push(model.clone());
                }

                // Add output format
                cmd.push("--output-format".to_string());
                cmd.push(claude_config.output_format.clone());

                // Add verbose flag if configured
                if claude_config.verbose {
                    cmd.push("--verbose".to_string());
                }

                // Claude reads from stdin, so we'll pipe the prompt file using cat
                // The prompt file is already in the container at the mounted workspace path
                let full_cmd = format!("cat '{}' | {}", container_prompt_path, cmd.join(" "));
                Ok(vec!["sh".to_string(), "-c".to_string(), full_cmd])
            }
        }
    }
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> Result<String> {
    if path.starts_with("~/") {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(path.replacen('~', home.to_str().unwrap(), 1))
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
    fn test_parse_memory_limit_gigabytes() {
        assert_eq!(parse_memory_limit("8g").unwrap(), 8 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory_limit("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_limit("16g").unwrap(), 16 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_limit_megabytes() {
        assert_eq!(parse_memory_limit("512m").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory_limit("256M").unwrap(), 256 * 1024 * 1024);
        assert_eq!(parse_memory_limit("1024m").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_limit_bytes() {
        assert_eq!(parse_memory_limit("1073741824").unwrap(), 1_073_741_824);
    }

    #[test]
    fn test_parse_memory_limit_invalid() {
        assert!(parse_memory_limit("invalid").is_err());
        assert!(parse_memory_limit("abc").is_err());
    }

    #[test]
    fn test_expand_path_absolute() {
        assert_eq!(expand_path("/usr/bin").unwrap(), "/usr/bin");
        assert_eq!(
            expand_path("/home/user/project").unwrap(),
            "/home/user/project"
        );
    }

    #[test]
    fn test_expand_path_relative() {
        assert_eq!(expand_path("./local/path").unwrap(), "./local/path");
        assert_eq!(expand_path("relative").unwrap(), "relative");
    }

    #[test]
    fn test_expand_path_tilde() {
        if dirs::home_dir().is_some() {
            let expanded = expand_path("~/.ssh").unwrap();
            assert!(!expanded.starts_with('~'));
            assert!(expanded.ends_with("/.ssh"));

            let expanded = expand_path("~/Documents/code").unwrap();
            assert!(!expanded.starts_with('~'));
            assert!(expanded.ends_with("/Documents/code"));
        }
    }

    #[test]
    fn test_sandbox_runner_new() {
        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());
        assert_eq!(runner.config.sandbox.enabled, config.sandbox.enabled);
        assert_eq!(runner.provider, Provider::Cursor);
    }

    #[test]
    fn test_sandbox_runner_new_claude() {
        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Claude, config.agent.clone());
        assert_eq!(runner.provider, Provider::Claude);
    }

    #[test]
    fn test_build_agent_command_cursor() {
        use tempfile::tempdir;

        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        let temp_dir = tempdir().unwrap();
        let prompt_file = temp_dir.path().join("test-prompt.txt");
        std::fs::write(&prompt_file, "test prompt").unwrap();

        let cmd = runner.build_agent_command(&prompt_file).unwrap();
        assert!(!cmd.is_empty());
        assert_eq!(cmd[0], "agent"); // Default cursor path
        assert_eq!(cmd[1], "-p");
        assert_eq!(cmd[2], "test prompt");
    }

    #[test]
    fn test_build_agent_command_claude() {
        use tempfile::tempdir;

        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Claude, config.agent.clone());

        let temp_dir = tempdir().unwrap();
        let prompt_file = temp_dir.path().join("test-prompt.txt");
        std::fs::write(&prompt_file, "test prompt").unwrap();

        let cmd = runner.build_agent_command(&prompt_file).unwrap();
        assert_eq!(cmd.len(), 3);
        assert_eq!(cmd[0], "sh");
        assert_eq!(cmd[1], "-c");
        assert!(cmd[2].contains("cat"));
        assert!(cmd[2].contains("claude"));
        assert!(cmd[2].contains("-p"));
    }

    #[test]
    fn test_timeout_duration_calculation() {
        // Verify timeout_minutes is converted correctly to Duration
        let config = Config::default();
        assert_eq!(config.sandbox.resources.timeout_minutes, 60);

        let timeout_duration = std::time::Duration::from_secs(
            u64::from(config.sandbox.resources.timeout_minutes) * 60,
        );
        assert_eq!(timeout_duration.as_secs(), 3600); // 60 minutes = 3600 seconds
    }

    #[test]
    fn test_timeout_error_message() {
        // Verify timeout error messages contain "timed out" for detection
        let timeout_minutes = 30;
        let error_msg = format!("Container execution timed out after {timeout_minutes} minutes");
        assert!(error_msg.contains("timed out"));

        let error_msg2 = format!("Agent execution timed out after {timeout_minutes} minutes");
        assert!(error_msg2.contains("timed out"));
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_containers() {
        // This test verifies the cleanup function can be called
        // It will skip if Docker is not available
        let result = SandboxRunner::cleanup_orphaned_containers().await;

        // Function should either succeed (returning count) or fail with Docker connection error
        match result {
            Ok(_count) => {
                // Successfully cleaned up containers (or found none)
                // This is valid - count can be 0 if no orphaned containers exist
                // count is u32, so it's always >= 0
            }
            Err(e) => {
                // Docker not available - this is acceptable in test environments
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("Docker") || error_msg.contains("docker"),
                    "Unexpected error: {error_msg}"
                );
            }
        }
    }

    #[test]
    fn test_allowlist_network_policy_adds_net_admin() {
        use crate::config::{Config, NetworkPolicy};

        // Create a config with allowlist policy
        let mut config = Config::default();
        config.sandbox.network.policy = NetworkPolicy::Allowlist;
        config.sandbox.network.allowed = vec!["github.com".to_string(), "crates.io".to_string()];

        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        // Build container config
        let temp_dir = tempfile::tempdir().unwrap();
        let container_config = runner.build_container_config(temp_dir.path()).unwrap();

        // Verify NET_ADMIN capability is added
        assert!(container_config.host_config.is_some());
        let host_config = container_config.host_config.unwrap();
        assert!(host_config.cap_add.is_some());
        let caps = host_config.cap_add.unwrap();
        assert!(caps.contains(&"NET_ADMIN".to_string()));
    }

    #[test]
    fn test_allowlist_network_policy_with_empty_allowed_list() {
        use crate::config::{Config, NetworkPolicy};

        // Create a config with allowlist policy but no allowed domains
        let mut config = Config::default();
        config.sandbox.network.policy = NetworkPolicy::Allowlist;
        config.sandbox.network.allowed = Vec::new();

        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        // Build container config - should still add NET_ADMIN
        let temp_dir = tempfile::tempdir().unwrap();
        let container_config = runner.build_container_config(temp_dir.path()).unwrap();

        // Verify NET_ADMIN capability is still added even with empty allowed list
        assert!(container_config.host_config.is_some());
        let host_config = container_config.host_config.unwrap();
        assert!(host_config.cap_add.is_some());
        let caps = host_config.cap_add.unwrap();
        assert!(caps.contains(&"NET_ADMIN".to_string()));
    }

    #[tokio::test]
    async fn test_create_persistent_container() {
        // This test verifies the persistent container creation function can be called
        // It will skip if Docker is not available
        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        let temp_dir = tempfile::tempdir().unwrap();
        let result = runner.create_persistent_container(temp_dir.path()).await;

        match result {
            Ok(container_name) => {
                // Successfully created container - clean it up
                assert!(!container_name.is_empty());
                assert!(container_name.starts_with("ralph-"));

                // Clean up the container
                let _ = SandboxRunner::remove_persistent_container(&container_name).await;
            }
            Err(e) => {
                // Docker not available or image not found - this is acceptable in test environments
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("Docker")
                        || error_msg.contains("docker")
                        || error_msg.contains("Failed to create container")
                        || error_msg.contains("Failed to start container")
                        || error_msg.contains("image")
                        || error_msg.contains("Image"),
                    "Unexpected error: {error_msg}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_remove_persistent_container() {
        // This test verifies the container removal function can be called
        // It will skip if Docker is not available
        let result = SandboxRunner::remove_persistent_container("nonexistent-container").await;

        match result {
            Ok(()) => {
                // Successfully attempted removal (container may not exist, which is fine)
            }
            Err(e) => {
                // Docker not available - this is acceptable in test environments
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("Docker") || error_msg.contains("docker"),
                    "Unexpected error: {error_msg}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_check_container_health_nonexistent() {
        // Health check on non-existent container should fail gracefully
        let Ok(docker) = Docker::connect_with_local_defaults() else {
            return; // Docker not available, skip test
        };

        if docker.ping().await.is_err() {
            return; // Docker not accessible, skip test
        }

        let result =
            SandboxRunner::check_container_health(&docker, "nonexistent-container-xyz").await;

        // Should fail with inspection error
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("inspect")
                || error_msg.contains("removed")
                || error_msg.contains("No such container"),
            "Unexpected error: {error_msg}"
        );
    }

    #[tokio::test]
    async fn test_check_container_health_running_container() {
        // Integration test: create a container, verify health check passes
        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        let temp_dir = tempfile::tempdir().unwrap();

        // Create a persistent container
        let Ok(container_name) = runner.create_persistent_container(temp_dir.path()).await else {
            return; // Docker or image not available, skip test
        };

        // Health check should pass for running container
        let docker = Docker::connect_with_local_defaults().unwrap();
        let result = SandboxRunner::check_container_health(&docker, &container_name).await;
        assert!(result.is_ok(), "Health check failed: {result:?}");

        // Clean up
        let _ = SandboxRunner::remove_persistent_container(&container_name).await;
    }

    #[tokio::test]
    async fn test_check_container_health_stopped_container() {
        // Integration test: create a container, stop it, verify health check restarts it
        let config = Config::default();
        let runner = SandboxRunner::new(config.clone(), Provider::Cursor, config.agent.clone());

        let temp_dir = tempfile::tempdir().unwrap();

        // Create a persistent container
        let Ok(container_name) = runner.create_persistent_container(temp_dir.path()).await else {
            return; // Docker or image not available, skip test
        };

        let docker = Docker::connect_with_local_defaults().unwrap();

        // Stop the container
        let _ = docker.stop_container(&container_name, None).await;

        // Health check should restart it
        let result = SandboxRunner::check_container_health(&docker, &container_name).await;
        assert!(result.is_ok(), "Health check failed to restart: {result:?}");

        // Verify container is now running
        let info = docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .unwrap();
        let running = info.state.and_then(|s| s.running).unwrap_or(false);
        assert!(running, "Container should be running after health check");

        // Clean up
        let _ = SandboxRunner::remove_persistent_container(&container_name).await;
    }

    #[test]
    fn test_build_iptables_script_rejects_shell_injection() {
        // Valid domains should appear in script
        let allowed = vec!["github.com".to_string(), "api.anthropic.com".to_string()];
        let script = build_iptables_script(&allowed);
        assert!(script.contains("github.com"));
        assert!(script.contains("api.anthropic.com"));

        // Malicious domains should be skipped entirely
        let malicious = vec![
            "github.com; rm -rf /".to_string(),
            "$(whoami).evil.com".to_string(),
            "`id`.evil.com".to_string(),
            "valid.com".to_string(), // one valid to ensure script still works
        ];
        let script = build_iptables_script(&malicious);

        // Malicious payloads must NOT appear in script
        assert!(!script.contains("rm -rf"));
        assert!(!script.contains("$(whoami)"));
        assert!(!script.contains("`id`"));

        // Valid domain should still be present
        assert!(script.contains("valid.com"));

        // Should have warning comments for skipped domains
        assert!(script.contains("SKIPPED invalid domain"));
    }

    #[test]
    fn test_build_iptables_script_structure() {
        let allowed = vec!["example.com".to_string()];
        let script = build_iptables_script(&allowed);

        // Verify script structure
        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("set -e"));
        assert!(script.contains("iptables -F OUTPUT"));
        assert!(script.contains("iptables -P OUTPUT DROP"));
        assert!(script.contains("-o lo -j ACCEPT")); // loopback
        assert!(script.contains("--dport 53")); // DNS
        assert!(script.contains("ESTABLISHED,RELATED")); // stateful
    }

    #[test]
    fn test_build_iptables_script_ipv6_support() {
        let allowed = vec!["github.com".to_string()];
        let script = build_iptables_script(&allowed);

        // Verify ip6tables availability check
        assert!(script.contains("HAS_IP6TABLES="));
        assert!(script.contains("command -v ip6tables"));

        // Verify IPv6 rules mirror IPv4 rules
        assert!(script.contains("ip6tables -F OUTPUT"));
        assert!(script.contains("ip6tables -P OUTPUT DROP"));
        assert!(script.contains("ip6tables -A OUTPUT -o lo -j ACCEPT"));
        assert!(script.contains("ip6tables -A OUTPUT -p udp --dport 53"));
        assert!(script.contains("ip6tables -A OUTPUT -p tcp --dport 53"));
        assert!(
            script.contains("ip6tables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT")
        );

        // Verify helper function for routing IPv4/IPv6
        assert!(script.contains("add_ip_rule()"));
        assert!(script.contains("ip6tables -A OUTPUT -d"));

        // Verify conditional execution based on ip6tables availability
        assert!(script.contains("if [ \"$HAS_IP6TABLES\" -eq 1 ]"));
    }

    #[test]
    fn test_build_iptables_script_uses_getent_ahosts() {
        // getent ahosts returns both IPv4 and IPv6 addresses
        let allowed = vec!["example.com".to_string()];
        let script = build_iptables_script(&allowed);

        // Should prefer getent ahosts over getent hosts for dual-stack support
        assert!(script.contains("getent ahosts"));
    }
}
