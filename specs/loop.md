# Loop Execution

The core iteration mechanism that drives Ralph development cycles.

## Behavior

Each iteration:
1. Load prompt file (`PROMPT_plan.md` or `PROMPT_build.md` based on mode)
2. Pipe prompt to agent CLI via stdin
3. Capture agent output
4. Run validation (backpressure)
5. Auto-push if configured
6. Check completion conditions
7. Persist state and continue

## Modes

- **Plan**: Gap analysis, generates `IMPLEMENTATION_PLAN.md`
- **Build**: Implements from plan, commits after each task

## Completion Conditions

Loop terminates when:
- Max iterations reached (`--max`)
- Idle detection: N consecutive iterations without git changes (configurable via `idle_threshold`, default 2)
- Circuit breaker: N consecutive errors (configurable via `max_consecutive_errors`, default 3)
- User cancellation (`ralph cancel` or Ctrl+C)

## State Persistence

State stored in `.ralph/state.toml`:
- `active`: Whether loop is currently running
- `iteration`: Current iteration count
- `mode`: Loop mode (plan/build)
- `started_at`: Start timestamp
- `last_iteration_at`: Last iteration timestamp
- `max_iterations`: Iteration limit (if set)
- `error_count`: Total errors encountered
- `consecutive_errors`: Current consecutive error streak
- `last_error`: Most recent error message
- `last_commit`: Last recorded git commit hash (for idle detection)
- `idle_iterations`: Consecutive iterations without git changes

State survives restarts — `ralph loop` resumes from last iteration.

## Error Recovery

- Validation failures: Append error to next iteration's prompt, reset consecutive error count
- Agent timeouts: Increment iteration, increment consecutive errors, continue
- Rate limits: Exponential backoff (2^n seconds, capped at 60s), continue
- Circuit breaker: After `max_consecutive_errors` consecutive failures, stop loop
- Other errors: Stop loop, report error

## Acceptance Criteria

1. Fresh context each iteration (no pollution between runs)
2. State persists across restarts
3. Validation errors visible to agent in subsequent iteration
4. Graceful handling of timeouts and rate limits
