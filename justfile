# List available recipes
default:
    @just --list

# Build the project
build:
    cargo build

# Run the CLI
run *args:
    cargo run -- {{args}}

# Run all checks (build + clippy + fmt + test)
check:
    nix flake check

# Run tests
test:
    cargo test

# Check coverage (must meet threshold)
coverage:
    nix run .#coverage

# Fix formatting
fmt:
    cargo fmt

# Auto-fix clippy lints
fix:
    cargo clippy --fix --allow-dirty

# Create a worktree for parallel agent development
worktree branch:
    #!/usr/bin/env bash
    set -euo pipefail

    branch="{{branch}}"
    worktree_dir=".worktrees/$branch"

    if [ -d "$worktree_dir" ]; then
        echo "Worktree already exists at $worktree_dir"
        exit 1
    fi

    # Enable worktreeConfig extension (allows per-worktree config to override)
    git config extensions.worktreeConfig true

    # Create the worktree
    git worktree add "$worktree_dir" -b "$branch" 2>/dev/null || \
        git worktree add "$worktree_dir" "$branch"

    # Configure agent identity and signing in the worktree
    git -C "$worktree_dir" config --worktree user.name "kitaebot"
    git -C "$worktree_dir" config --worktree user.email "kitaebot@pm.me"
    git -C "$worktree_dir" config --worktree user.signingkey "D90B07BF61863EA1"
    git -C "$worktree_dir" config --worktree commit.gpgsign true
    git -C "$worktree_dir" config --worktree core.sshCommand "ssh -i ~/.ssh/kitaebot -o IdentitiesOnly=yes"

    echo "Worktree created at $worktree_dir"
