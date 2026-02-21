# Agent Providers

Abstraction layer for AI agent CLIs that execute each loop iteration.

## Supported Providers

### Cursor CLI

```toml
[agent]
provider = "cursor"

[agent.cursor]
path = "agent"           # "cursor-agent" on NixOS
model = "auto"           # Optional: "auto", "claude-sonnet-4-20250514", etc.
output_format = "text"   # Options: "text", "json", "stream-json"
sandbox = "disabled"     # Required for shell access
timeout_minutes = 60     # Optional: override sandbox timeout for Cursor
```

Invocation: `agent -p "prompt" --sandbox disabled --output-format text --model <model>`

Note: Cursor CLI takes prompt as `-p` argument.

### Claude Code CLI

```toml
[agent]
provider = "claude"

[agent.claude]
path = "claude"
model = "opus"           # Default: "opus". Options: "opus", "sonnet"
skip_permissions = true  # Required for autonomous operation
output_format = "text"   # Options: "text", "json", "stream-json"
timeout_minutes = 120    # Optional: override sandbox timeout for Claude
verbose = false          # Optional: enable verbose output
```

Invocation: `claude -p --dangerously-skip-permissions --model opus < prompt`

Note: Claude CLI takes prompt via stdin, `-p` enables print mode.

## Provider Selection

Priority (highest to lowest):
1. CLI flag (`--provider`)
2. Environment variable `RALPH_PROVIDER`
3. Config file (`ralph.toml`)

## Provider Trait

```rust
trait AgentProvider {
    fn name(&self) -> &'static str;
    async fn invoke(&self, project_dir: &Path, prompt: &str) -> Result<String>;
}
```

## Acceptance Criteria

1. Provider configurable via config and CLI flag
2. Clear error messages when agent CLI not found
3. Prompt delivered to agent (stdin for Claude, CLI arg for Cursor), output captured from stdout
4. Non-zero exit codes reported as errors
