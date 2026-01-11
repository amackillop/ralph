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

## Acceptance Criteria (All Met ✅)

1. ✅ Allowlist network policy works with at least 5 common domains
2. ✅ `ralph image build` creates working sandbox image
3. ✅ Container cleanup happens automatically
4. ✅ Timeout kills runaway containers

## Configuration

```toml
[sandbox]
enabled = true
image = "ralph:latest"
reuse_container = true  # NEW: reuse between iterations

[sandbox.network]
policy = "allowlist"
allowed = ["github.com", "crates.io", "api.anthropic.com"]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60
```
