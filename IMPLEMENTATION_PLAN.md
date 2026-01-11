# Implementation Plan

This document tracks implementation status based on specifications in `specs/`.

**Last verified**: 2026-01-10 (re-verified via code search)

## Summary

Ralph is a Rust CLI tool for iterative AI development loops. The core functionality is implemented:
- Two agent providers (Cursor, Claude) with configurable paths
- Core CLI commands (init, loop, status, cancel, revert, clean)
- State management and completion detection
- Docker sandbox infrastructure (not yet integrated with multi-provider)

This plan tracks remaining features from the specs, ordered by priority.

---

## Priority 1: Provider CLI Flag

**Spec**: `specs/provider-flag.md`  
**Status**: ❌ Not Implemented  
**Effort**: Small

Add `--provider` flag to `ralph loop` to override the provider from `ralph.toml` on the command line.

### Tasks

- [ ] Add `--provider` option to `Loop` command in `src/main.rs`
- [ ] Update `loop_cmd::run()` to accept provider override parameter
- [ ] If CLI flag is set, use it instead of config value
- [ ] Update help text to document the option

### Acceptance Criteria

- [ ] `ralph loop --help` shows `--provider` option
- [ ] `ralph loop build --provider claude` uses Claude regardless of config
- [ ] `ralph loop build --provider cursor` uses Cursor regardless of config
- [ ] Default behavior (no flag) unchanged - uses config value

### Files to Modify

- `src/main.rs` - Add `--provider` arg to Loop command
- `src/commands/loop_cmd.rs` - Use override when provided

---

## Priority 2: Sandbox Integration with Multi-Provider

**Spec**: `specs/agent-integration.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Currently `loop_cmd.rs` line 84 warns "Docker sandbox is not yet implemented for the provider system" and runs without sandbox even when enabled.

### Tasks

- [ ] Refactor `SandboxRunner` to work with `AgentProvider` trait
- [ ] Update `loop_cmd::run()` to use sandbox when enabled
- [ ] Pass agent configuration into sandbox
- [ ] Test with both Cursor and Claude providers

### Acceptance Criteria

- [ ] `ralph loop build` (without `--no-sandbox`) uses Docker sandbox
- [ ] Both Cursor and Claude work inside sandbox
- [ ] Warning message removed from `loop_cmd.rs`

### Files to Modify

- `src/sandbox/docker.rs` - Generalize for any provider
- `src/commands/loop_cmd.rs` - Integrate sandbox with loop

---

## Priority 3: Enhanced Status Command

**Spec**: `specs/monitoring.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Enhance `ralph status` to show more useful information for long-running loops. Current implementation only shows basic state fields.

### Tasks

- [ ] Add recent commit messages (last 3-5) from git log
- [ ] Calculate and display elapsed time since start
- [ ] Calculate average iteration duration
- [ ] Estimate time remaining (based on average × remaining iterations)
- [ ] Track and display error count (needs state extension)

### Acceptance Criteria

- [ ] `ralph status` shows current iteration and elapsed time
- [ ] Shows recent commit messages
- [ ] Shows average iteration time
- [ ] Shows estimated time remaining (when max_iterations is set)

### Files to Modify

- `src/commands/status.rs` - Add enhanced display
- `src/state.rs` - Add fields for error count, iteration timings

---

## Priority 4: Structured Logging

**Spec**: `specs/monitoring.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Write structured logs to `.cursor/ralph.log` for observability.

### Tasks

- [ ] Add `[monitoring]` section to config schema
- [ ] Create logging infrastructure (JSON or text format)
- [ ] Log iteration start/complete events with timestamps
- [ ] Log errors with context
- [ ] Log commit hashes

### Configuration

```toml
[monitoring]
log_file = ".cursor/ralph.log"
log_format = "json"  # or "text"
show_progress = true
```

### Acceptance Criteria

- [ ] Log file created and written during loop execution
- [ ] Each iteration logs start and completion events
- [ ] Log entries include timestamps, iteration number, and relevant data

### Files to Modify

- `src/config.rs` - Add `MonitoringConfig` struct
- `src/commands/loop_cmd.rs` - Write log entries
- New file: `src/logging.rs` - Structured logging utilities

---

## Priority 5: Progress Display

**Spec**: `specs/monitoring.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Show real-time progress during loop execution.

### Tasks

- [ ] Create progress display component
- [ ] Show iteration number with visual indicator
- [ ] Show elapsed time
- [ ] Show last commit message
- [ ] Update display between iterations

### Acceptance Criteria

- [ ] Progress shows during loop execution
- [ ] Updates between iterations
- [ ] Can be disabled via config

### Files to Modify

- `src/commands/loop_cmd.rs` - Add progress display
- `src/config.rs` - Add `show_progress` option

---

## Priority 6: Timeout Enforcement

**Spec**: `specs/sandbox-improvements.md`  
**Status**: ❌ Not Implemented  
**Effort**: Small

The `timeout_minutes` config is defined in `src/config.rs` (line 233) but never used.

### Tasks

- [ ] Add timeout to container execution in `SandboxRunner`
- [ ] Gracefully stop container on timeout
- [ ] Report timeout in status/logs
- [ ] Non-sandbox mode: add timeout to agent invocation

### Acceptance Criteria

- [ ] Container killed after timeout
- [ ] Timeout reported clearly
- [ ] Loop continues to next iteration after timeout

### Files to Modify

- `src/sandbox/docker.rs` - Enforce timeout
- `src/commands/loop_cmd.rs` - Handle timeout in non-sandbox mode

---

## Priority 7: Container Cleanup

**Spec**: `specs/sandbox-improvements.md`  
**Status**: ❌ Not Implemented  
**Effort**: Small

Clean up orphaned containers on startup.

### Tasks

- [ ] On `ralph loop` start, check for orphaned `ralph-*` containers
- [ ] Remove orphaned containers before starting
- [ ] Log cleanup actions

### Acceptance Criteria

- [ ] Orphaned containers cleaned up automatically
- [ ] No manual cleanup needed after crashes

### Files to Modify

- `src/sandbox/docker.rs` - Add cleanup function
- `src/commands/loop_cmd.rs` - Call cleanup on start

---

## Priority 8: Image Management Commands

**Spec**: `specs/sandbox-improvements.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Add subcommands to manage the sandbox Docker image.

### Tasks

- [ ] Add `ralph image` subcommand with `build`, `pull`, `status` actions
- [ ] `ralph image build` - Build image from Dockerfile
- [ ] `ralph image pull` - Pull pre-built image from registry
- [ ] `ralph image status` - Show image info (exists, size, date)

### Acceptance Criteria

- [ ] `ralph image build` creates working sandbox image
- [ ] `ralph image status` shows useful information

### Files to Create/Modify

- `src/main.rs` - Add `Image` command
- New file: `src/commands/image.rs` - Image management

---

## Priority 9: Allowlist Network Policy

**Spec**: `specs/sandbox-improvements.md`  
**Status**: ⚠️ Partial (warns and falls back to allow-all)

Currently in `src/sandbox/docker.rs` line 153:
```rust
NetworkPolicy::Allowlist => {
    warn!("Allowlist network policy is not fully implemented yet. Using allow-all.");
}
```

### Tasks

- [ ] Research implementation approaches (iptables, proxy container, custom network)
- [ ] Implement allowlist with at least 5 common domains
- [ ] Test DNS resolution for allowed domains
- [ ] Block all other traffic

### Implementation Options

1. Custom Docker network with DNS interception
2. iptables rules inside container
3. Proxy container that filters traffic

Note: `src/sandbox/network.rs` has `COMMON_ALLOWED_DOMAINS` constant with common domains.

### Acceptance Criteria

- [ ] Allowlist network policy works with common domains
- [ ] Domains in `allowed` list are reachable
- [ ] Other domains are blocked

### Files to Modify

- `src/sandbox/docker.rs` - Implement allowlist logic
- `src/sandbox/network.rs` - Helper functions

---

## Priority 10: Container Reuse

**Spec**: `specs/sandbox-improvements.md`  
**Status**: ❌ Not Implemented  
**Effort**: Medium

Reuse containers between iterations for faster startup.

### Tasks

- [ ] Add `reuse_container` config option
- [ ] Keep container running between iterations
- [ ] Execute agent inside existing container
- [ ] Clean up container on loop end

### Configuration

```toml
[sandbox]
reuse_container = true
```

### Acceptance Criteria

- [ ] Container reused when enabled
- [ ] Faster iteration startup
- [ ] Container cleaned up on loop end

### Files to Modify

- `src/config.rs` - Add `reuse_container` option
- `src/sandbox/docker.rs` - Implement reuse logic

---

## Priority 11: Notification Hooks (Future)

**Spec**: `specs/monitoring.md`  
**Status**: ❌ Not Implemented (marked as "Future" in spec)  
**Effort**: Large

On completion or error, send notifications.

### Tasks

- [ ] Add `[monitoring.notifications]` config section
- [ ] Implement webhook POST
- [ ] Implement desktop notification
- [ ] Implement sound alert

### Configuration

```toml
[monitoring.notifications]
on_complete = "https://hooks.example.com/ralph"
on_error = "desktop"
```

### Acceptance Criteria

- [ ] Webhook called on completion
- [ ] Desktop notification shown on error

---

## Completed Features ✅

### Agent System (`specs/agent-integration.md`)
- [x] Cursor CLI support (`src/agent/cursor.rs`)
- [x] Claude Code support (`src/agent/claude.rs`)
- [x] Configurable paths
- [x] Clear error messages
- [x] Provider selection via config

### Core CLI
- [x] `ralph init` - Initialize project (`src/commands/init.rs`)
- [x] `ralph loop [plan|build]` - Main loop (`src/commands/loop_cmd.rs`)
- [x] `ralph status` - Basic status display (`src/commands/status.rs`)
- [x] `ralph cancel` - Cancel loop (`src/commands/cancel.rs`)
- [x] `ralph revert` - Revert commits (`src/commands/revert.rs`)
- [x] `ralph clean` - Remove files (`src/commands/clean.rs`)

### Configuration (`src/config.rs`)
- [x] Agent configuration (cursor/claude)
- [x] Sandbox configuration
- [x] Git configuration
- [x] Completion promise detection

### State Management (`src/state.rs`)
- [x] Loop state persistence
- [x] Iteration tracking
- [x] Timestamps

---

## Notes

- No `src/lib/` directory exists; shared utilities are in main source modules
- Sandbox code in `src/sandbox/` has `#[allow(dead_code)]` as it's not yet integrated
- All source modules have unit tests
- Dependencies are managed via `Cargo.toml`
