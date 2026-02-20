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

## How to code
### Side Effects & Testability
- Abstract side effects (IO, network, time) as closure parameters
- Keep core logic pure and easily testable
- Inject real dependencies (clients, filesystem) at call sites
- For tests, pass closures returning controlled values or mutating test state

Example:
```rust
// Core logic takes closures for side effects
fn process_items<F, G>(items: Vec<Item>, fetch: F, save: G) -> Result<()>
where
    F: Fn(&str) -> Result<Data>,
    G: Fn(&Data) -> Result<()>,
{
    for item in items {
        let data = fetch(&item.id)?;
        save(&data)?;
    }
    Ok(())
}

// Production: inject real clients
process_items(items, |id| client.fetch(id), |data| db.save(data))?;

// Test: inject controlled behavior
let call_count = Cell::new(0);
process_items(items, |_| {
    call_count.set(call_count.get() + 1);
    Ok(test_data.clone())
}, |_| Ok(()))?;
assert_eq!(call_count.get(), 3);
```

### Output Formatting
- Separate formatting from printing: format functions return `String`
- Use `std::fmt::Write` to build strings, not `println!`
- Print only at the top-level `run()` function
- This makes output testable without capturing stdout

Example:
```rust
use std::fmt::Write;

// Pure: returns a string, easily testable
pub fn format_result(items: &[Item]) -> String {
    let mut out = String::new();
    writeln!(&mut out, "Found {} items:", items.len()).unwrap();
    for item in items {
        writeln!(&mut out, "  - {}", item.name).unwrap();
    }
    out
}

// Entry point: only place that does IO
pub async fn run() -> Result<()> {
    let items = load_items()?;
    print!("{}", format_result(&items));  // IO here only
    Ok(())
}

// Test: verify formatting without IO
#[test]
fn test_format_result() {
    let items = vec![Item { name: "foo" }, Item { name: "bar" }];
    let output = format_result(&items);
    assert!(output.contains("2 items"));
    assert!(output.contains("foo"));
}
```

### Error Handling
- Use `anyhow::Result` for application-level error propagation
- Use `thiserror` to model error domains requiring decision logic
- Match on error variants when caller must handle cases differently

Example:
```rust
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

// Caller decides behavior per variant
match fetch_resource(id) {
    Ok(data) => process(data),
    Err(FetchError::NotFound(_)) => create_default(),
    Err(FetchError::RateLimited { retry_after_secs }) => sleep_and_retry(retry_after_secs),
    Err(e) => return Err(e.into()),
}
```

### Iterators & Functional Style
- Prefer iterator chains over manual loops
- Use `.fold()` to accumulate into tuples or complex state
- Return `impl Iterator` from functions to compose lazily
- Use `.filter_map()` to filter and transform in one pass
- Use `.scan()` for stateful iteration
- Use Entry API for HashMap: `.entry().or_default()`

### Structs & Types
- Use simple structs with derives: `#[derive(Debug, Clone, PartialEq, Eq)]`
- Model domain concepts as structs even if small
- Use struct update syntax: `Struct { field: new_val, ..existing }`

## Testing
- Test individual functions, not just public API
- Use `debug_assert!` for internal invariants
- Include benchmarks for performance-critical code
