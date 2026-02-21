# Configuration

Ralph configuration via `ralph.toml`.

## Full Example

```toml
[agent]
provider = "claude"  # or "cursor"

[agent.cursor]
path = "cursor-cli"
model = "auto"
output_format = "text"
sandbox = "disabled"
timeout_minutes = 60

[agent.claude]
path = "claude"
model = "opus"
skip_permissions = true
output_format = "text"
verbose = false
timeout_minutes = 120

[sandbox]
enabled = true
image = "ralph:latest"
reuse_container = false
use_local_image = true
mounts = []
credential_mounts = [
    { host = "~/.ssh", container = "/root/.ssh", readonly = true },
    { host = "~/.gitconfig", container = "/root/.gitconfig", readonly = true },
]

[sandbox.network]
policy = "allow-all"  # or "allowlist", "deny"
allowed = ["github.com", "api.anthropic.com"]
dns = ["8.8.8.8", "1.1.1.1"]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60

[git]
auto_push = true
auto_pr = true
pr_base = "master"
protected_branches = ["main", "master", "production"]

[git.worktree]
name = "ralph-bot"
email = "ralph-bot@example.com"
signing_key = "ABCD1234"
ssh_key = "~/.ssh/ralph-bot"

[completion]
idle_threshold = 2

[validation]
enabled = true
command = "nix flake check --quiet"

[monitoring]
max_consecutive_errors = 5
show_progress = true
log_file = ".ralph/loop.log"
log_format = "json"
log_rotation = "daily"
```

## Section Reference

### `[agent]`
- `provider`: Which agent to use (`cursor` or `claude`)

### `[agent.cursor]` / `[agent.claude]`
- See [agents.md](agents.md) for provider-specific options

### `[sandbox]`
- See [sandbox.md](sandbox.md) for sandbox options

### `[git]`
- `auto_push`: Push after each iteration (default: true)
- `auto_pr`: Create PR on branch completion (default: true)
- `pr_base`: Base branch for PRs (default: master)
- `protected_branches`: Branches that cannot be modified directly

### `[git.worktree]`
Identity configuration for worktree commits (used by bot):
- `name`: Git user.name for commits
- `email`: Git user.email for commits
- `signing_key`: GPG key ID for signed commits (optional)
- `ssh_key`: Path to SSH key for push (optional)

### `[completion]`
- `idle_threshold`: Consecutive iterations without commits before marking complete (default: 2)

### `[validation]`
- See [validation.md](validation.md) for validation options

### `[monitoring]`
- See [monitoring.md](monitoring.md) for monitoring options
