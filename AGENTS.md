# Ralph - Operational Guide

ralph is a Rust CLI tool implementing the Ralph Wiggum technique for iterative AI development.

## Commands

Run `just --list` for available commands. Key recipes:

- `just check` - Run all checks (build + clippy + fmt + test)
- `just coverage` - Check coverage meets threshold
- `just fmt` - Fix formatting
- `just fix` - Auto-fix clippy lints
- `just test` - Run tests


## Change Validation

Before committing, run:

```bash
just check && just coverage
```

Both must pass. If checks fail:
1. `just fmt` - Fix formatting issues
2. `just fix` - Auto-fix clippy lints
3. Fix remaining errors manually

Do not commit failing code.

## Commit Guidelines

Work in atomic, focused commits. Each commit should represent a single logical change.

**Only commit when:**
1. `just check` passes
2. `just coverage` meets threshold

 Follow the seven rules:

- Separate subject from body with blank line
- Limit subject to 50 characters (72 hard limit)
- Capitalize subject line
- No period at end of subject
- Use imperative mood in subject (e.g., "Fix bug" not "Fixed bug" or "Fixes bug")
- Wrap body at 72 characters
- Body explains what and why, not how
- The diff explains how

Subject test: "If applied, this commit will [subject]" must make sense.

Like how a comment provides important context for a line of code, the commit message
should provide important context for the change being committed.

After committing, push immediately.
