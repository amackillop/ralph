# Monitoring and Observability Specification

## Overview

When running long Ralph loops (50+ iterations), operators need visibility into progress, performance, and issues. This spec defines monitoring capabilities.

## Features

### 1. Progress Display

Real-time progress during loop execution:

```
━━━━━━━━━━━━━━━━━━━━ Iteration 15 ━━━━━━━━━━━━━━━━━━━━
  Mode:      Build
  Started:   2 hours ago
  Duration:  ~8 min/iteration avg
  Commits:   12 successful
  Errors:    2 (recovered)

  Current task: Implementing user authentication
  Last commit: "Add JWT token validation"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 2. Status Command Enhancement

`ralph status` should show:
- Current iteration and elapsed time
- Recent commit messages
- Error count and last error
- Estimated time remaining (based on avg iteration time)

### 3. Log File

Write structured logs to `.cursor/ralph.log`:

```json
{"ts":"2024-01-15T10:30:00Z","iteration":15,"event":"start","task":"user-auth"}
{"ts":"2024-01-15T10:38:00Z","iteration":15,"event":"complete","commit":"abc123"}
```

### 4. Notification Hooks ✅

On completion or error:
- Webhook POST ✅
- Desktop notification ✅
- Sound alert ✅

## Acceptance Criteria

1. Progress shows during loop execution
2. `ralph status` shows meaningful information
3. Log file captures iteration history
4. Errors are reported clearly without stopping the loop

## Configuration

```toml
[monitoring]
log_file = ".cursor/ralph.log"
log_format = "json"  # or "text"
show_progress = true

[monitoring.notifications]
on_complete = "https://hooks.example.com/ralph"  # Webhook URL for completion
on_error = "desktop"  # Options: "webhook:<url>", "desktop", "sound", or "none"
```
