# CLI Interface

Command-line interface for ralph operations.

## Commands

### `ralph init`

Initialize a project for Ralph:
- Creates `ralph.toml` with defaults
- Creates `PROMPT_plan.md` and `PROMPT_build.md` templates
- Creates `AGENTS.md` template
- Creates `.cursor/rules/ralph.mdc` (Ralph rules for Cursor)
- Prints instructions to create `specs/` directory

```bash
ralph init           # Initialize (fails if files exist)
ralph init --force   # Overwrite existing files
```

### `ralph loop <mode>`

Run the main Ralph loop:

```bash
# Plan mode - generates branch-structured IMPLEMENTATION_PLAN.md
ralph loop plan                              # Analyze specs, generate plan

# Build mode - creates worktrees and builds all branches in parallel
ralph loop build                             # Build all branches from plan (parallel)
ralph loop build --sequential                # Build branches one at a time

# Build options
ralph loop build -m 20                       # Limit iterations (--max-iterations)
ralph loop build --provider claude           # Override provider
ralph loop build --no-sandbox                # Disable sandbox
ralph loop build --unlimited                 # No iteration limit
ralph loop build -p custom_prompt.md         # Custom prompt file (--prompt)
```

### `ralph status`

Show current loop state and progress.

### `ralph cancel`

Stop a running loop gracefully.

### `ralph revert`

Revert commits from failed iterations. Ralph should always be run on a branch with only its commits:

```bash
ralph revert                # Revert last commit (default)
ralph revert --last 3       # Revert last 3 commits
```

### `ralph clean`

Remove Ralph state files and worktrees:

```bash
ralph clean                        # Remove .ralph/state.toml only
ralph clean --all                  # Also remove prompt and rules files
ralph clean --worktrees            # Remove all worktrees
```

### `ralph image <subcommand>`

Manage sandbox Docker image:

```bash
ralph image build                    # Build from flake.nix (default)
ralph image build --dockerfile ./Dockerfile --tag myimage:v1
ralph image pull                     # Pull image (skips if exists locally)
ralph image pull --image ghcr.io/org/ralph:latest --force
ralph image status                   # Show configured image info
ralph image status --image custom:tag
```

## Configuration

All options configurable via `ralph.toml`, CLI flags override config.

## Acceptance Criteria

1. `ralph --help` documents all commands
2. `ralph init` creates working project structure
3. CLI flags override config file values
4. Error messages actionable (suggest fixes)
