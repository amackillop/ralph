# Feature: Provider CLI Flag

## Overview

Add a `--provider` flag to `ralph loop` to override the provider from ralph.toml on the command line.

## Current Behavior

The agent provider is configured only in `ralph.toml`:

```toml
[agent]
provider = "cursor"  # or "claude"
```

## Desired Behavior

Allow overriding via CLI flag:

```bash
# Use provider from ralph.toml (default)
ralph loop build

# Override to use Claude
ralph loop build --provider claude

# Override to use Cursor
ralph loop build --provider cursor
```

## Implementation

1. Add `--provider` option to the `Loop` command in `src/main.rs`
2. In `src/commands/loop_cmd.rs`, check if CLI flag is set; if so, use it instead of config
3. Update help text to document the option

## Acceptance Criteria

- [x] `ralph loop --help` shows `--provider` option
- [x] `ralph loop build --provider claude` uses Claude regardless of config
- [x] `ralph loop build --provider cursor` uses Cursor regardless of config
- [x] Default behavior (no flag) unchanged - uses config value
