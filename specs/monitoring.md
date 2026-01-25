# Monitoring

Observability for long-running Ralph loops.

## Progress Display

Real-time status during loop execution:

```
━━━━━━━━━━━━━━━━━━━━ Iteration 15 ━━━━━━━━━━━━━━━━━━━━
  Mode:      Build
  Started:   2 hours ago
  Duration:  ~8 min/iteration avg
  Commits:   12 successful
  Errors:    2 (recovered)

  Last commit: "Add JWT token validation"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## Status Command

`ralph status` shows:
- Current iteration and elapsed time
- Recent commit messages
- Error count and last error
- Whether loop is active

## Structured Logging

JSON logs to `.ralph/loop.log`:

```json
{"ts":"2024-01-15T10:30:00Z","iteration":15,"event":"iteration_start"}
{"ts":"2024-01-15T10:38:00Z","iteration":15,"event":"iteration_complete","commit":"abc123"}
{"ts":"2024-01-15T10:38:01Z","iteration":15,"event":"error","error":"validation failed"}
```

## Notifications

Alert on completion or error:
- Webhook POST to URL
- Desktop notification
- Sound alert

## Configuration

```toml
[monitoring]
log_file = ".ralph/loop.log"
log_format = "json"
show_progress = true

[monitoring.notifications]
on_complete = "https://hooks.example.com/ralph"
on_error = "desktop"  # "webhook:<url>", "desktop", "sound", "none"
```

## Acceptance Criteria

1. Progress visible during loop execution
2. `ralph status` shows meaningful info even when loop not running
3. Structured logs capture full iteration history
4. Errors reported without stopping the loop
