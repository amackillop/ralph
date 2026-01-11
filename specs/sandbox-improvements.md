# Sandbox Improvements Specification

## Overview

The Docker sandbox provides isolation for Ralph execution. This spec defines improvements to make the sandbox more robust and configurable.

## Current Implementation

Located in `src/sandbox/docker.rs`:
- Creates Docker container with workspace mounted
- Mounts SSH keys and gitconfig read-only
- Applies resource limits (CPU, memory)
- Basic network policy support (allow-all, deny)

## Improvements Needed

### 1. Allowlist Network Policy

The "allowlist" network policy is currently not fully implemented:

```rust
NetworkPolicy::Allowlist => {
    // For allowlist, we'd need to set up iptables rules or use a custom network
    warn!("Allowlist network policy is not fully implemented yet. Using allow-all.");
}
```

**Implementation approach:**
- Create a custom Docker network with DNS interception
- Use iptables rules to only allow specified domains
- Or use a proxy container that filters traffic

### 2. Docker Image Management

Add commands to manage the sandbox Docker image:

```bash
# Build the sandbox image
cursor-ralph image build

# Pull pre-built image
cursor-ralph image pull

# Show image status
cursor-ralph image status
```

### 3. Container Lifecycle

Improve container management:
- Reuse containers between iterations (faster startup)
- Clean up orphaned containers on startup
- Show container logs on error

### 4. Timeout Enforcement

Currently `timeout_minutes` is defined but not enforced:
- Add timeout to container execution
- Graceful shutdown on timeout
- Report timeout in status

## Acceptance Criteria

1. Allowlist network policy works with at least 5 common domains
2. `cursor-ralph image build` creates working sandbox image
3. Container cleanup happens automatically
4. Timeout kills runaway containers

## Configuration

```toml
[sandbox]
enabled = true
image = "cursor-ralph:latest"
reuse_container = true  # NEW: reuse between iterations

[sandbox.network]
policy = "allowlist"
allowed = ["github.com", "crates.io", "api.anthropic.com"]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60
```
