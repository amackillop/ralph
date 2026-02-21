# Loop Execution

The core iteration mechanism that drives Ralph development cycles.

## Modes

- **Plan**: Analyzes specs and codebase, generates `IMPLEMENTATION_PLAN.md` organized by branches
- **Build**: Implements tasks for a specific branch, creates PR on completion

## Plan Mode

Plan mode generates a branch-structured implementation plan:

```markdown
## Branch: fix-sandbox-image
Goal: Update Docker image to include agent CLIs
Base: master

- [ ] Delete Dockerfile
- [ ] Update flake.nix dockerImage with claude-code, cursor-cli
- [ ] Remove --dockerfile flag from image command

## Branch: add-watch-flag
Goal: Stream agent output for debugging
Base: master

- [ ] Add --watch flag to CLI
- [ ] Add invoke_streaming to AgentProvider trait
- [ ] Implement in Claude/Cursor providers
```

Each branch group:
- Has a clear, single goal
- Branches from `master` (configurable via `pr_base`)
- Contains cohesive, related tasks
- Results in one PR when complete

Each task within a branch represents an **atomic commit**:
- One logical change per task/commit
- Task description includes the **why** behind the change, not just the what
- Reasoning and decisions captured in task description flow into commit message
- Future iterations can reference commit history to understand context

## Build Mode

Build mode reads `IMPLEMENTATION_PLAN.md`, creates worktrees for all branches, and executes them in parallel:

```bash
ralph loop build                   # Create worktrees, build all branches in parallel
ralph loop build --sequential      # Build branches one at a time instead
```

### Automatic Workflow

1. **Parse plan**: Extract all `## Branch: <name>` sections from `IMPLEMENTATION_PLAN.md`

2. **Create worktrees**: For each branch:
   ```bash
   git config extensions.worktreeConfig true
   git worktree add .worktrees/<branch> -b <branch>
   ```

3. **Configure identity** (from `[git.worktree]` config):
   ```bash
   git -C .worktrees/<branch> config --worktree user.name "<name>"
   git -C .worktrees/<branch> config --worktree user.email "<email>"
   git -C .worktrees/<branch> config --worktree user.signingkey "<key>"
   git -C .worktrees/<branch> config --worktree commit.gpgsign true
   git -C .worktrees/<branch> config --worktree core.sshCommand "ssh -i <key_path> -o IdentitiesOnly=yes"
   ```

4. **Copy plan**: `cp IMPLEMENTATION_PLAN.md .worktrees/<branch>/`

5. **Build in parallel**: Spawn agent for each worktree concurrently

6. **On branch completion**: Create PR, mark branch done

7. **On all complete**: Report summary

### Configuration

```toml
[git.worktree]
name = "ralph-bot"
email = "ralph-bot@example.com"
signing_key = "ABCD1234"          # GPG key ID (optional)
ssh_key = "~/.ssh/ralph-bot"      # SSH key for push (optional)
```

### Worktree Cleanup

```bash
ralph clean --worktrees            # Remove all worktrees
```

## Iteration Behavior

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

1. Plan mode generates branch-structured `IMPLEMENTATION_PLAN.md`
2. Build mode parses plan, creates worktrees, builds all branches in parallel
3. Each branch results in one PR on successful completion
4. Worktrees auto-created with proper identity from `[git.worktree]` config
5. `IMPLEMENTATION_PLAN.md` copied to each worktree on creation
6. Fresh context each iteration (no pollution between runs)
7. State persists across restarts (per-worktree)
8. Validation errors visible to agent in subsequent iteration
9. Graceful handling of timeouts and rate limits
10. `ralph clean --worktrees` removes all worktrees
