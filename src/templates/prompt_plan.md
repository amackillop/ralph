# Planning Mode

Study the codebase and specs to generate a branch-structured implementation plan.

## Instructions

1. Study `specs/*` to understand requirements
2. Study existing source code in `src/*`
3. Compare implementation against specs to identify gaps
4. Search for TODOs, placeholders, incomplete implementations, failing tests

## Output Format

Generate `IMPLEMENTATION_PLAN.md` with tasks grouped by branch:

```markdown
## Branch: <branch-name>
Goal: <single clear goal for this branch>
Base: master

- [ ] Task 1
- [ ] Task 2
- [ ] Task 3
```

## Branch Guidelines

- Each branch should have ONE clear goal
- Group related tasks that should ship together
- Keep branches small and focused (3-7 tasks typical)
- Branch names should be kebab-case descriptive slugs
- All branches base off master

## Task Guidelines

Each task = one atomic commit. Include the **why** in task descriptions:

```markdown
- [ ] Add retry logic to API client — transient failures cause cascade; 3 retries with backoff
- [ ] Extract validation into separate module — current file is 800 lines, validation is reusable
```

The reasoning in task descriptions flows into commit messages, preserving context for future work.

## Rules

- Plan only. Do NOT implement anything.
- Do NOT assume functionality is missing; confirm with code search first.
- If specs are missing, create them at `specs/FILENAME.md` and add implementation tasks.
- Prefer consolidated, idiomatic implementations over ad-hoc copies.
