# Sandbox Improvements Specification

## Overview

The Docker sandbox provides isolation for Ralph execution. This spec defines improvements to make the sandbox more robust and configurable.

## Current Implementation

Located in `src/sandbox/docker.rs`:
- Creates Docker container with workspace mounted
- Mounts SSH keys and gitconfig read-only
- Applies resource limits (CPU, memory)
- Network policy support (allow-all, deny, allowlist)
- Timeout enforcement for container execution
- Container reuse between iterations
- Automatic cleanup of orphaned containers

## Status

All improvements listed below have been implemented:

### 1. Allowlist Network Policy ✅

Implemented using iptables rules within the container. The implementation:
- Sets up iptables rules to block all outbound traffic except DNS and allowed domains
- Resolves allowed domains to IP addresses and allows traffic to those IPs
- Requires NET_ADMIN capability for the container
- Supports common domains (github.com, crates.io, api.anthropic.com, etc.)

### 2. Docker Image Management ✅

Commands implemented in `src/commands/image.rs`:
- `ralph image build` - Builds the sandbox image from Dockerfile
- `ralph image pull` - Pulls pre-built image from registry
- `ralph image status` - Shows image status and information

### 5. Nix-Based Image Building ✅

**Status**: Complete

Replace Dockerfile-based image building with Nix-based builds for better reproducibility and integration with the Nix ecosystem.

**Requirements**:

1. **Nix Image Build**:
   - Add Docker image build to `flake.nix` using `pkgs.dockerTools.buildImage` or similar
   - Image should include the same dependencies as current Dockerfile:
     - Base system (Ubuntu or NixOS base)
     - curl, git, ca-certificates, gnupg, lsb-release, sudo
     - iptables, dnsutils (for network policy)
     - Node.js (v20.x)
     - Python 3 with pip and venv
     - Rust toolchain (via rustup or Nix)
     - Cursor CLI placeholder (same as Dockerfile)
     - Git configuration for commits
   - Image should be built via `nix build .#dockerImage` or similar
   - Default `ralph image build` should use Nix build method

2. **Local Image Preference**:
   - Add configuration option to prefer local images over pulling
   - When `ralph image pull` is executed:
     - First check if image exists locally using Docker API
     - If found locally, skip pull and inform user
     - If not found locally, proceed with pull from registry
   - This avoids unnecessary network traffic when image is already available

3. **Dockerfile Deprecation**:
   - Remove `Dockerfile` from repository
   - Update `ralph image build` to use Nix by default
   - Optionally support `--dockerfile` flag for legacy builds (if needed)

**Implementation Notes**:
- Use `pkgs.dockerTools.buildImage` in `flake.nix` to create the Docker image
- Image should be loadable into Docker via `docker load < result`
- Consider adding a flake app: `nix run .#docker-image` to build and load the image
- Update `src/commands/image.rs` to:
  - Check for local image existence before pulling
  - Support Nix-based builds via `nix build` command execution
  - Tag the Nix-built image appropriately for use by sandbox

### 3. Container Lifecycle ✅

All improvements implemented:
- Container reuse between iterations (faster startup via persistent containers)
- Automatic cleanup of orphaned containers on startup
- Container logs available on error

### 4. Timeout Enforcement ✅

Timeout enforcement implemented:
- Timeout applied to container execution
- Container is killed on timeout
- Timeout errors are logged and reported

## Acceptance Criteria

### Completed ✅
1. ✅ Allowlist network policy works with at least 5 common domains
2. ✅ `ralph image build` creates working sandbox image
3. ✅ Container cleanup happens automatically
4. ✅ Timeout kills runaway containers

### Completed ✅
5. ✅ Nix-based image building replaces Dockerfile (in `flake.nix` as `dockerImage` package)
6. ✅ `ralph image pull` checks for local image before pulling
7. ✅ Sandbox uses Nix-built image by default (`ralph image build` uses Nix)

## Configuration

```toml
[sandbox]
enabled = true
image = "ralph:latest"
reuse_container = true  # Reuse between iterations
use_local_image = true  # Prefer local image, skip pull if exists

[sandbox.network]
policy = "allowlist"
allowed = ["github.com", "crates.io", "api.anthropic.com"]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60
```

**Available Configuration Options**:
- `use_local_image` (boolean): When true, `ralph image pull` will check for local image first and skip pull if found. Default: `true` (recommended to avoid unnecessary network traffic). Use `--force` flag with `ralph image pull` to override.
