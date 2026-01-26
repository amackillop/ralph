# ralph

A Rust CLI tool implementing the [Ralph Wiggum technique](https://ghuntley.com/ralph/) - iterative AI development loops that let AI agents (Cursor, Claude) work autonomously while you supervise.

## What is Ralph?

Ralph is a development methodology based on continuous AI agent loops. As Geoffrey Huntley describes it: **"Ralph is a Bash loop"** - a simple `while true` that repeatedly feeds an AI agent a prompt file, allowing it to iteratively improve its work until completion.

The technique is named after Ralph Wiggum from The Simpsons, embodying the philosophy of persistent iteration despite setbacks.

### Core Concept

```bash
# Traditional Ralph (bash loop):
while :; do cat PROMPT.md | claude ; done

# ralph does this with any supported AI agent:
ralph loop build --max-iterations 50
```

Each iteration:
1. Reads the prompt file (PROMPT_plan.md or PROMPT_build.md)
2. Invokes Cursor with the prompt
3. Waits for completion
4. Checks for completion promise or max iterations
5. Git commits and pushes changes
6. Repeats with fresh context

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/your-org/ralph.git
cd ralph

# Build and install
cargo install --path .
```

### Using Nix Flake

```bash
# Build and run directly
nix run github:your-org/ralph

# Or install into your profile
nix profile install github:your-org/ralph

# Build locally
nix build
./result/bin/ralph --help
```

### Development with Nix

```bash
# Enter the development shell with all tools
nix develop

# Or use direnv (recommended)
direnv allow
```

Run checks:

```bash
# Run all checks (build, clippy, fmt, tests)
nix flake check

# Individual checks
nix build .#checks.x86_64-linux.ralph-clippy
nix build .#checks.x86_64-linux.ralph-fmt
nix build .#checks.x86_64-linux.ralph-test
```

## Quick Start

```bash
# Initialize Ralph in your project
cd /path/to/your/project
ralph init

# Edit the configuration files:
# - ralph.toml - Select your agent (cursor or claude)
# - AGENTS.md - Add your build/test commands
# - specs/ - Create specification files for your features
# - PROMPT_plan.md / PROMPT_build.md - Customize if needed

# Run planning mode to generate IMPLEMENTATION_PLAN.md
ralph loop plan --max-iterations 5

# Review the plan, then start building
ralph loop build --max-iterations 50 --completion-promise "All tests passing"
```

## Commands

### `ralph init`

Initialize Ralph files in the current project.

```bash
ralph init          # Create default files
ralph init --force  # Overwrite existing files
```

Creates:
- `ralph.toml` - Project configuration
- `PROMPT_plan.md` - Planning mode prompt
- `PROMPT_build.md` - Building mode prompt
- `AGENTS.md` - Operational guide (build/test commands)
- `.cursor/rules/ralph.mdc` - Cursor rules for Ralph

### `ralph loop`

Start a Ralph loop.

```bash
# Planning mode (generates IMPLEMENTATION_PLAN.md)
ralph loop plan --max-iterations 5

# Building mode (implements from plan)
ralph loop build --max-iterations 50

# With completion promise
ralph loop build --completion-promise "DONE"

# Skip Docker sandbox (run directly on host)
ralph loop build --no-sandbox

# Use custom prompt file
ralph loop build --prompt my-custom-prompt.md
```

Options:
- `--max-iterations <N>` - Stop after N iterations (default: unlimited)
- `--completion-promise <TEXT>` - Stop when `<promise>TEXT</promise>` is detected
- `--no-sandbox` - Run without Docker isolation
- `--prompt <FILE>` - Use custom prompt file

### `ralph status`

Show current loop status.

```bash
ralph status
```

### `ralph cancel`

Cancel an active loop.

```bash
ralph cancel
```

### `ralph revert`

Revert Ralph commits.

```bash
ralph revert --last 3  # Revert last 3 commits
```

### `ralph clean`

Remove Ralph state files.

```bash
ralph clean        # Remove state file only
ralph clean --all  # Remove all Ralph files
```

## Configuration

### `ralph.toml`

```toml
[agent]
# Which AI agent to use: "cursor" or "claude"
provider = "cursor"

# Cursor CLI configuration
# See: https://cursor.com/docs/cli/overview
[agent.cursor]
# Path to Cursor agent CLI
# - Default: "agent" (standard installation)
# - NixOS: "cursor-agent"
# - Custom: "/path/to/agent"
path = "agent"
output_format = "text"

# Claude Code CLI configuration
# See: https://docs.anthropic.com/en/docs/claude-code
[agent.claude]
path = "claude"
skip_permissions = true  # Required for autonomous operation
output_format = "stream-json"

[sandbox]
# Enable Docker sandboxing for isolation
enabled = true
image = "ralph:latest"

[sandbox.network]
# Network policy: "allow-all", "allowlist", "deny"
policy = "allow-all"

# Allowed domains when policy = "allowlist"
# allowed = ["github.com", "registry.npmjs.org"]

[sandbox.resources]
memory = "8g"
cpus = "4"
timeout_minutes = 60

[git]
auto_push = true
protected_branches = ["main", "master", "production"]

[completion]
promise_format = "<promise>{}</promise>"
```

### Code Validation

Ralph can automatically validate code after each agent iteration to catch compilation errors and test failures:

```toml
[validation]
# Enable code validation after each iteration
# If disabled, the loop relies entirely on the agent to validate code
enabled = true

# Validation command to run
# Can be a single command or space-separated command with arguments
# Examples:
#   - "nix flake check" (default, recommended for Nix projects)
#   - "cargo check"
#   - "cargo test"
#   - "./validate.sh"
command = "nix flake check"
```

When validation fails:
- Error is logged and a notification is sent (if configured)
- Loop continues (agent can fix the issue in the next iteration)
- State tracks the validation error for `ralph status`

### Supported AI Agents

#### Cursor (default)

```toml
[agent]
provider = "cursor"

[agent.cursor]
path = "agent"           # Use "cursor-agent" on NixOS
# model = "claude-sonnet-4-20250514"  # Optional, uses Cursor's default
output_format = "text"
```

#### Claude Code

```toml
[agent]
provider = "claude"

[agent.claude]
path = "claude"
skip_permissions = true  # Required for autonomous operation
# model = "opus"         # Optional
output_format = "stream-json"
verbose = false
```

### Prompt Files

#### `PROMPT_plan.md`

Used in planning mode to generate `IMPLEMENTATION_PLAN.md`:
- Studies specs and existing code
- Performs gap analysis
- Creates prioritized task list
- Does NOT implement anything

#### `PROMPT_build.md`

Used in building mode:
- Studies specs and implementation plan
- Picks the most important task
- Implements and runs tests
- Commits and pushes on success
- Updates plan with learnings

### `AGENTS.md`

Operational guide with build/test commands:

```markdown
## Build & Run
npm install
npm run build

## Validation
- Tests: `npm test`
- Typecheck: `npm run typecheck`
- Lint: `npm run lint`
```

## Sandboxing

Ralph runs with maximum autonomy, which requires containment. The philosophy: *"It's not if it gets popped, it's when. And what is the blast radius?"*

### Docker Isolation

By default, ralph runs the AI agent inside a Docker container:

- **Workspace mounted read-write** at `/workspace`
- **Credentials mounted read-only** (~/.ssh, ~/.gitconfig)
- **Resource limits** (CPU, memory, timeout)
- **Network policy** (allow-all by default, configurable)

### Building the Docker Image

```bash
docker build -t ralph:latest .
```

### Disabling Sandbox

For trusted environments:

```bash
ralph loop build --no-sandbox
```

Or in `ralph.toml`:

```toml
[sandbox]
enabled = false
```

## Workflow

### Phase 1: Define Requirements

Create specification files in `specs/`:

```
specs/
├── user-authentication.md
├── product-catalog.md
└── shopping-cart.md
```

Each spec should define:
- What to build (requirements)
- Acceptance criteria
- Edge cases

### Phase 2: Planning

```bash
ralph loop plan --max-iterations 5
```

Ralph will:
1. Study all specs
2. Analyze existing code
3. Identify gaps
4. Generate `IMPLEMENTATION_PLAN.md`

Review the plan before proceeding.

### Phase 3: Building

```bash
ralph loop build --max-iterations 50 --completion-promise "All tests passing"
```

Ralph will:
1. Pick the most important task from the plan
2. Implement it fully (no stubs!)
3. Run tests
4. Commit and push on success
5. Update the plan
6. Repeat

## Philosophy

### Iteration > Perfection
Don't aim for perfect on first try. Let the loop refine the work.

### Failures Are Data
"Deterministically bad" means failures are predictable and informative. Use them to tune prompts.

### Operator Skill Matters
Success depends on writing good prompts, not just having a good model.

### Persistence Wins
Keep trying until success. The loop handles retry logic automatically.

### Trust but Verify
Let Ralph ralph. But use sandboxing, protected branches, and review commits.

## Safety

### Protected Branches

Ralph cannot force-push to branches listed in `protected_branches`:

```toml
[git]
protected_branches = ["main", "master", "production"]
```

### Escape Hatches

```bash
# Emergency stop
ralph cancel

# Revert uncommitted changes
git reset --hard

# Revert Ralph commits
ralph revert --last 3
```

### Git Reflog

All commits are recoverable via `git reflog`.

## Environment Compatibility

| Environment | How it Works |
|-------------|--------------|
| **Cursor Editor** | Run CLI in terminal, Cursor runs in background mode |
| **cursor-cli** | Direct CLI invocation in headless mode |
| **Git Worktrees** | Isolated state per worktree |
| **Cursor Cloud** | API-based invocation (future) |

## Troubleshooting

### "Agent not found"

ralph needs an AI agent CLI. Depending on your configuration:

**For Cursor:**
```bash
# Install Cursor CLI
curl https://cursor.com/install -fsS | bash

# On NixOS, configure in ralph.toml:
[agent.cursor]
path = "cursor-agent"
```

**For Claude:**
```bash
# Install Claude Code CLI
npm install -g @anthropic-ai/claude-code
```

### Docker Not Running

If you see Docker connection errors:

```bash
# Start Docker
sudo systemctl start docker

# Or disable sandbox
ralph loop build --no-sandbox
```

### Loop Running Forever

Always set `--max-iterations` or `--completion-promise`:

```bash
# Safe defaults
ralph loop build --max-iterations 50
```

## License

MIT

## Credits

- [Geoffrey Huntley](https://ghuntley.com/ralph/) - Original Ralph Wiggum technique
- [Ralph Playbook](https://github.com/ClaytonFarr/ralph-playbook) - Comprehensive Ralph documentation
