# Agent Providers

Abstraction layer for AI agent CLIs that execute each loop iteration.

## Supported Providers

### Cursor CLI

```toml
[agent]
provider = "cursor"

[agent.cursor]
path = "agent"           # "cursor-agent" on NixOS
output_format = "text"
sandbox = "disabled"     # Required for shell access
```

Invocation: `agent -p --sandbox disabled --output-format text < prompt`

### Claude Code CLI

```toml
[agent]
provider = "claude"

[agent.claude]
path = "claude"
model = "opus"           # Recommended for primary agent
skip_permissions = true  # Required for autonomous operation
output_format = "stream-json"
```

Invocation: `claude -p --dangerously-skip-permissions --model opus < prompt`

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
3. Prompt passed via stdin, output captured from stdout
4. Non-zero exit codes reported as errors
