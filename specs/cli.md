# CLI Interface

Command-line interface for ralph operations.

## Commands

### `ralph init`

Initialize a project for Ralph:
- Creates `ralph.toml` with defaults
- Creates `PROMPT_plan.md` and `PROMPT_build.md` templates
- Creates `AGENTS.md` template
- Prints instructions to create `specs/` directory

### `ralph loop <mode>`

Run the main Ralph loop:

```bash
ralph loop plan                              # Planning mode
ralph loop build                             # Build mode (default)
ralph loop build -m 20                       # Limit iterations (--max-iterations)
ralph loop build -c "ALL TESTS PASS"         # Stop on promise (--completion-promise)
ralph loop build --provider claude           # Override provider
ralph loop build --no-sandbox                # Disable sandbox
ralph loop build --unlimited                 # No iteration limit
```

### `ralph status`

Show current loop state and progress.

### `ralph cancel`

Stop a running loop gracefully.

### `ralph revert`

Revert uncommitted changes from failed iteration.

### `ralph clean`

Remove Ralph state files (`.ralph/state.toml`).

### `ralph image <subcommand>`

Manage sandbox Docker image:
- `ralph image build` — Build from flake.nix
- `ralph image pull` — Pull pre-built image
- `ralph image status` — Show image info

## Configuration

All options configurable via `ralph.toml`, CLI flags override config.

## Acceptance Criteria

1. `ralph --help` documents all commands
2. `ralph init` creates working project structure
3. CLI flags override config file values
4. Error messages actionable (suggest fixes)
