# Agent Integration Specification

## Overview

ralph supports multiple AI agent CLIs to execute each iteration of the Ralph loop.

## Supported Agents

### 1. Cursor CLI (Default)

See [Cursor CLI docs](https://cursor.com/docs/cli/overview).

```bash
# Install
curl https://cursor.com/install -fsS | bash
```

```toml
[agent]
provider = "cursor"

[agent.cursor]
path = "agent"           # Default ("cursor-agent" on NixOS)
output_format = "text"
sandbox = "disabled"     # Required for shell access (cargo test, git, etc.)
```

### 2. Claude Code CLI

See [Claude Code docs](https://docs.anthropic.com/en/docs/claude-code).

```bash
# Install
npm install -g @anthropic-ai/claude-code
```

```toml
[agent]
provider = "claude"

[agent.claude]
path = "claude"
skip_permissions = true  # Required for autonomous operation
output_format = "stream-json"
```

## Architecture

Provider trait in `src/agent/mod.rs`:
- `CursorProvider` - Uses `agent -p "prompt"`
- `ClaudeProvider` - Uses `claude -p --dangerously-skip-permissions`

## Environments

1. **Cursor Editor** - Run `ralph loop` in terminal
2. **Headless/CLI** - Print mode for automation
3. **Git Worktrees** - Isolated state per worktree
4. **Cloud Agents** - API-based (future)

## Status

- [x] Cursor CLI support
- [x] Claude Code support
- [x] Configurable paths
- [x] Clear error messages
- [x] Provider CLI override (`--provider` flag)
- [x] Sandbox integration with multi-provider
