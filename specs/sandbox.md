# Docker Sandbox

Isolated execution environment for safe autonomous agent operation.

## Philosophy

Ralph requires `--dangerously-skip-permissions` for autonomous operation.
The sandbox is the security boundary — not the agent's permission system.

"It's not if it gets popped, it's when. And what is the blast radius?"

## Capabilities

- Workspace mounted read-write at `/workspace`
- Credential auto-mounting (SSH, gitconfig, npmrc, cargo, pypi)
- Custom volume mounts
- Resource limits (CPU, memory, timeout)
- Network policy enforcement (DNS configurable)
- Container reuse between iterations (optional)

## Network Policies

```toml
[sandbox.network]
policy = "allow-all"  # Default: unrestricted
policy = "allowlist"  # Only allowed domains
policy = "deny"       # No network access

allowed = ["github.com", "crates.io", "api.anthropic.com"]

# Custom DNS servers (default: ["8.8.8.8", "1.1.1.1"])
dns = ["8.8.8.8", "1.1.1.1"]
```

Allowlist implemented via iptables rules within container.

## Image Management

Built via Nix for reproducibility:
- `ralph image build` — Build image from flake.nix
- `ralph image pull` — Pull pre-built image (checks local first)
- `ralph image status` — Show image info

## Configuration

```toml
[sandbox]
enabled = true
image = "ralph:latest"
reuse_container = false  # Default: false. Set true for faster iteration startup
use_local_image = true   # Skip pull if image exists locally

# Custom volume mounts (workspace always mounted at /workspace)
mounts = [
    { host = "~/.npm", container = "/root/.npm", readonly = false }
]

# Credential auto-mounts (defaults shown, set to [] to disable)
# Auto-mounted read-only if they exist on host
credential_mounts = [
    { host = "~/.ssh", container = "/root/.ssh", readonly = true },
    { host = "~/.gitconfig", container = "/root/.gitconfig", readonly = true },
    { host = "~/.npmrc", container = "/root/.npmrc", readonly = true },
    { host = "~/.cargo/credentials.toml", container = "/root/.cargo/credentials.toml", readonly = true },
    { host = "~/.pypirc", container = "/root/.pypirc", readonly = true },
]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60
```

## Acceptance Criteria

1. Agent cannot access host credentials outside mounted paths
2. Network allowlist blocks unauthorized outbound traffic
3. Timeout kills runaway containers
4. Orphaned containers cleaned up on startup
