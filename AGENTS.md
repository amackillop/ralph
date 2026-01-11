# Ralph - Operational Guide

## Project Overview

ralph is a Rust CLI tool implementing the Ralph Wiggum technique for iterative AI development.

## Build & Run

```bash
# Enter development shell (provides Rust toolchain)
nix develop

# Build
cargo build

# Run
cargo run -- --help

# Or build with Nix
nix build
./result/bin/ralph --help
```

## Validation

Run these commands to validate changes:

```bash
# Format code
cargo fmt

# Run clippy (lints)
cargo clippy --all-targets -- -D warnings

# Run tests
cargo test

# Check coverage (required - must be >= 85%)
nix run .#coverage

# Full Nix check (build + clippy + fmt + test)
nix flake check
```

## Project Structure

```
src/
├── main.rs           # CLI entry point (clap)
├── agent/            # AI agent providers
│   ├── mod.rs        # AgentProvider trait
│   ├── cursor.rs     # Cursor CLI provider
│   └── claude.rs     # Claude CLI provider
├── commands/         # CLI subcommands
│   ├── init.rs       # ralph init
│   ├── loop_cmd.rs   # ralph loop
│   ├── status.rs     # ralph status
│   ├── cancel.rs     # ralph cancel
│   ├── revert.rs     # ralph revert
│   └── clean.rs      # ralph clean
├── config.rs         # ralph.toml parsing
├── state.rs          # Loop state management
├── detection.rs      # Completion detection
├── sandbox/          # Docker sandbox (not yet integrated)
└── templates/        # Embedded template files
```

## Key Dependencies

- `clap` - CLI argument parsing
- `tokio` - Async runtime
- `serde` + `toml` - Config parsing
- `async-trait` - Async traits for providers
- `bollard` - Docker API (for sandbox)

## Coding Guidelines

- Use `anyhow::Result` for error handling
- Add `#[allow(dead_code)]` for intentionally unused code
- Run `cargo fmt` before committing
- All clippy warnings must be resolved
