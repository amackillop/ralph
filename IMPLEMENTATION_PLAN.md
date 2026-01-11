# Implementation Plan

This document tracks implementation status based on specifications in `specs/`.

**Last verified**: 2026-01-11 (updated after Priority 11 implementation)

## Summary

Ralph is a Rust CLI tool for iterative AI development loops. All core functionality and priority features (1-11) are implemented and tested:
- Two agent providers (Cursor, Claude) with configurable paths and CLI override
- Core CLI commands (init, loop, status, cancel, revert, clean, image)
- State management and completion detection
- Docker sandbox with multi-provider support, timeout enforcement, container reuse, and allowlist network policy
- Monitoring: structured logging, progress display, enhanced status command with error tracking, and notification hooks (webhook, desktop, sound)

---

## Recent Fixes

### Configurable Code Validation in Loop ✅

**Issue**: The loop was relying entirely on the AI agent to run `cargo check`/`cargo test` before committing. If the agent skipped validation or ignored failures, compilation errors would slip through.

**Fix**: Added configurable validation system:
- New `[validation]` config section with `enabled` and `command` options
- Default command: `nix flake check` (recommended for Nix projects)
- Configurable to any command: `cargo check`, `cargo test`, `./validate.sh`, etc.
- Validation runs after each agent iteration, before completion detection
- If validation fails: error logged, notification sent, loop continues (agent can fix it)
- Can be disabled by setting `validation.enabled = false`

**Implementation**:
- Added `ValidationConfig` struct in `src/config.rs`
- Updated `validate_code()` function to use configured command
- Updated config templates and examples
- Default: `enabled = true`, `command = "nix flake check"`

**Location**:
- Config: `src/config.rs` - `ValidationConfig` struct
- Logic: `src/commands/loop_cmd.rs` - `validate_code()` function

---

## Future Work

### Priority 11: Notification Hooks ✅

**Spec**: `specs/monitoring.md`
**Status**: ✅ Implemented
**Effort**: Large

On completion or error, send notifications (webhook POST, desktop notification, sound alert).

**Implementation**:
- Added `reqwest` HTTP client dependency for webhook support
- Created `src/notifications.rs` module with:
  - Webhook POST support (JSON payload with event details)
  - Desktop notification support (notify-send, osascript, growlnotify)
  - Sound alert support (paplay, aplay, afplay, beep, bell character)
- Added `[monitoring.notifications]` config section:
  - `on_complete`: Webhook URL for completion events
  - `on_error`: Error notification method ("webhook:<url>", "desktop", "sound", or "none")
- Integrated notifications into loop completion and error handling
- Notifications are fire-and-forget (errors logged but don't stop loop)
- Added comprehensive tests for notification functionality
