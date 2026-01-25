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
- Completion promise detected in output (`--promise`)
- User cancellation (`ralph cancel` or Ctrl+C)

## State Persistence

State stored in `.ralph/state.toml`:
- Current iteration count
- Mode (plan/build)
- Start time
- Error count and last error
- Completion promise (if set)

State survives restarts — `ralph loop` resumes from last iteration.

## Error Recovery

- Validation failures: Append error to next iteration's prompt
- Agent timeouts: Increment iteration, continue
- Rate limits: Exponential backoff, continue
- Other errors: Stop loop, report error

## Acceptance Criteria

1. Fresh context each iteration (no pollution between runs)
2. State persists across restarts
3. Validation errors visible to agent in subsequent iteration
4. Graceful handling of timeouts and rate limits
