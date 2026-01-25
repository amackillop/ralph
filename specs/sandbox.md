# Docker Sandbox

Isolated execution environment for safe autonomous agent operation.

## Philosophy

Ralph requires `--dangerously-skip-permissions` for autonomous operation.
The sandbox is the security boundary — not the agent's permission system.

"It's not if it gets popped, it's when. And what is the blast radius?"

## Capabilities

- Workspace mounted read-write at `/workspace`
- SSH keys and gitconfig mounted read-only
- Resource limits (CPU, memory, timeout)
- Network policy enforcement
- Container reuse between iterations (optional)

## Network Policies

```toml
[sandbox.network]
policy = "allow-all"  # Default: unrestricted
policy = "allowlist"  # Only allowed domains
policy = "deny"       # No network access

allowed = ["github.com", "crates.io", "api.anthropic.com"]
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
reuse_container = true   # Faster iteration startup
use_local_image = true   # Skip pull if image exists locally

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
