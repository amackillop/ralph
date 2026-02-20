# Validation (Backpressure)

Code validation after each iteration to enforce quality constraints.

## Philosophy

Backpressure steers Ralph toward correct output. The prompt says "run tests"
generically; validation specifies the actual commands. Tests, typechecks,
and lints reject invalid work before it accumulates.

## Behavior

After each agent iteration:
1. Run validation command
2. On success: proceed to next iteration
3. On failure: append error to next iteration's prompt, continue

The agent sees validation failures and fixes them in subsequent runs.

## Error Feedback

Validation errors stored in state and appended to the next iteration's
prompt:

```
## ⚠️ VALIDATION ERROR FROM PREVIOUS ITERATION
The following validation error occurred. Please fix it:

```
cargo check failed:
error[E0382]: borrow of moved value: `x`
```

Fix the issues above and ensure validation passes before proceeding.
```

## Configuration

```toml
[validation]
enabled = true
command = "nix flake check --quiet"  # Default for Nix projects
# command = "cargo check"            # Rust without Nix
# command = "npm test"               # Node.js
# command = "./validate.sh"          # Custom script
```

## Error Truncation

- **Agent prompt**: Full error output (not truncated) for maximum context
- **Notifications**: First 5 lines only (keeps notifications concise)
- **Logs**: Full error output

## Acceptance Criteria

1. Validation runs after every iteration
2. Failures don't stop the loop — agent gets another chance
3. Full error output available to agent (not truncated)
4. Validation command configurable per project
