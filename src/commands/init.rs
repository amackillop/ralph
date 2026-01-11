//! Initialize Ralph files in a project directory.
//!
//! This module separates pure logic from IO by accepting closures for
//! filesystem operations, making the core logic easily testable.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::templates;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the init command, creating Ralph project files.
pub(crate) fn run(force: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    info!("Initializing Ralph in {}", cwd.display());

    let files = init_files();

    let results = init_project(
        &files,
        force,
        |path| cwd.join(path).exists(),
        |path| {
            fs::create_dir_all(cwd.join(path))
                .with_context(|| format!("Failed to create directory: {}", path.display()))
        },
        |path, content| {
            fs::write(cwd.join(path), content)
                .with_context(|| format!("Failed to write {}", path.display()))
        },
    )?;

    print!("{}", format_results(&results, &files));

    Ok(())
}

// -----------------------------------------------------------------------------
// Internal types
// -----------------------------------------------------------------------------

/// File to be written during init, with its relative path and content.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InitFile {
    /// Relative path for the file.
    path: PathBuf,
    /// File content.
    content: &'static str,
    /// Human-readable description.
    description: &'static str,
}

/// Result of attempting to write a file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WriteResult {
    /// File was created.
    Created,
    /// File was overwritten.
    Overwritten,
    /// File was skipped (already exists).
    Skipped,
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Returns the list of files to initialize in a Ralph project.
fn init_files() -> Vec<InitFile> {
    vec![
        InitFile {
            path: PathBuf::from("ralph.toml"),
            content: templates::RALPH_TOML,
            description: "Project configuration",
        },
        InitFile {
            path: PathBuf::from("PROMPT_plan.md"),
            content: templates::PROMPT_PLAN,
            description: "Planning mode prompt",
        },
        InitFile {
            path: PathBuf::from("PROMPT_build.md"),
            content: templates::PROMPT_BUILD,
            description: "Building mode prompt",
        },
        InitFile {
            path: PathBuf::from(".cursor/rules/ralph.mdc"),
            content: templates::RULES_MDC,
            description: "Ralph rules for Cursor",
        },
        InitFile {
            path: PathBuf::from("AGENTS.md"),
            content: templates::AGENTS_MD,
            description: "Operational guide (customize this!)",
        },
    ]
}

/// Core init logic: determines what files to write and writes them.
///
/// Takes closures for IO operations to enable testing:
/// - `exists`: checks if a path exists
/// - `create_dir`: creates a directory (and parents)
/// - `write_file`: writes content to a path
fn init_project<E, D, W>(
    files: &[InitFile],
    force: bool,
    exists: E,
    create_dir: D,
    mut write_file: W,
) -> Result<Vec<(PathBuf, WriteResult)>>
where
    E: Fn(&Path) -> bool,
    D: Fn(&Path) -> Result<()>,
    W: FnMut(&Path, &str) -> Result<()>,
{
    let mut results = Vec::new();

    // Collect directories to create
    let dirs: Vec<_> = files
        .iter()
        .filter_map(|f| f.path.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .collect();

    // Create directories
    for dir in dirs {
        create_dir(dir)?;
    }

    // Write files
    for file in files {
        let result = if exists(&file.path) && !force {
            WriteResult::Skipped
        } else {
            write_file(&file.path, file.content)?;
            if exists(&file.path) && force {
                WriteResult::Overwritten
            } else {
                WriteResult::Created
            }
        };
        results.push((file.path.clone(), result));
    }

    Ok(results)
}

/// Formats init results for display with colored output.
fn format_results(results: &[(PathBuf, WriteResult)], files: &[InitFile]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(
        &mut out,
        "\n{} Ralph initialized successfully!\n",
        "✓".green().bold()
    )
    .unwrap();
    writeln!(&mut out, "Created files:").unwrap();

    for (path, result) in results {
        let desc = files
            .iter()
            .find(|f| &f.path == path)
            .map_or("", |f| f.description);

        match result {
            WriteResult::Created => {
                writeln!(
                    &mut out,
                    "  {} - {}",
                    path.display().to_string().cyan(),
                    desc
                )
                .unwrap();
            }
            WriteResult::Overwritten => {
                writeln!(
                    &mut out,
                    "  {} {} (overwritten)",
                    "↻".blue(),
                    path.display()
                )
                .unwrap();
            }
            WriteResult::Skipped => {
                writeln!(
                    &mut out,
                    "  {} {} (already exists, use --force to overwrite)",
                    "⊘".yellow(),
                    path.display()
                )
                .unwrap();
            }
        }
    }

    writeln!(&mut out, "\n{}", "Next steps:".yellow().bold()).unwrap();
    writeln!(
        &mut out,
        "  1. Edit {} to select your agent",
        "ralph.toml".cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  2. Edit {} to configure your project",
        "AGENTS.md".cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  3. Create specs in {} directory",
        "specs/".cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  4. Run {} to generate implementation plan",
        "ralph loop plan".green()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  5. Run {} to start building",
        "ralph loop build".green()
    )
    .unwrap();

    out
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn test_init_files_not_empty() {
        let files = init_files();
        assert!(!files.is_empty());
        assert!(
            files
                .iter()
                .any(|f| f.path.as_path() == Path::new("ralph.toml"))
        );
    }

    #[test]
    fn test_init_project_creates_files() {
        let files = init_files();
        let written = RefCell::new(HashMap::new());
        let dirs_created = RefCell::new(HashSet::new());

        let results = init_project(
            &files,
            false,
            |_| false, // Nothing exists
            |path| {
                dirs_created.borrow_mut().insert(path.to_path_buf());
                Ok(())
            },
            |path, content| {
                written
                    .borrow_mut()
                    .insert(path.to_path_buf(), content.to_string());
                Ok(())
            },
        )
        .unwrap();

        // All files should be created
        assert_eq!(results.len(), files.len());
        for (_, result) in &results {
            assert_eq!(*result, WriteResult::Created);
        }

        // All files should have been written
        assert_eq!(written.borrow().len(), files.len());

        // .cursor/rules directory should have been created
        assert!(
            dirs_created
                .borrow()
                .contains(&PathBuf::from(".cursor/rules"))
        );
    }

    #[test]
    fn test_init_project_skips_existing_without_force() {
        let files = init_files();
        let written = RefCell::new(HashMap::new());

        let results = init_project(
            &files,
            false,
            |_| true, // Everything exists
            |_| Ok(()),
            |path, content| {
                written
                    .borrow_mut()
                    .insert(path.to_path_buf(), content.to_string());
                Ok(())
            },
        )
        .unwrap();

        // All files should be skipped
        for (_, result) in &results {
            assert_eq!(*result, WriteResult::Skipped);
        }

        // No files should have been written
        assert!(written.borrow().is_empty());
    }

    #[test]
    fn test_init_project_overwrites_with_force() {
        let files = init_files();
        let written = RefCell::new(HashMap::new());

        let results = init_project(
            &files,
            true,     // Force
            |_| true, // Everything exists
            |_| Ok(()),
            |path, content| {
                written
                    .borrow_mut()
                    .insert(path.to_path_buf(), content.to_string());
                Ok(())
            },
        )
        .unwrap();

        // All files should be overwritten
        for (_, result) in &results {
            assert_eq!(*result, WriteResult::Overwritten);
        }

        // All files should have been written
        assert_eq!(written.borrow().len(), files.len());
    }

    #[test]
    fn test_init_project_mixed_exists() {
        let files = vec![
            InitFile {
                path: PathBuf::from("new.txt"),
                content: "new",
                description: "New file",
            },
            InitFile {
                path: PathBuf::from("existing.txt"),
                content: "existing",
                description: "Existing file",
            },
        ];
        let written = RefCell::new(HashMap::new());

        let results = init_project(
            &files,
            false,
            |path| path == Path::new("existing.txt"),
            |_| Ok(()),
            |path, content| {
                written
                    .borrow_mut()
                    .insert(path.to_path_buf(), content.to_string());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(results[0], (PathBuf::from("new.txt"), WriteResult::Created));
        assert_eq!(
            results[1],
            (PathBuf::from("existing.txt"), WriteResult::Skipped)
        );
        assert_eq!(written.borrow().len(), 1);
    }

    #[test]
    fn test_format_results_created() {
        let files = vec![InitFile {
            path: PathBuf::from("test.txt"),
            content: "content",
            description: "Test file",
        }];
        let results = vec![(PathBuf::from("test.txt"), WriteResult::Created)];

        let output = format_results(&results, &files);
        assert!(output.contains("initialized successfully"));
        assert!(output.contains("test.txt"));
        assert!(output.contains("Test file"));
        assert!(output.contains("Next steps"));
    }

    #[test]
    fn test_format_results_skipped() {
        let files = vec![InitFile {
            path: PathBuf::from("existing.txt"),
            content: "content",
            description: "Existing",
        }];
        let results = vec![(PathBuf::from("existing.txt"), WriteResult::Skipped)];

        let output = format_results(&results, &files);
        assert!(output.contains("already exists"));
        assert!(output.contains("--force"));
    }

    #[test]
    fn test_format_results_overwritten() {
        let files = vec![InitFile {
            path: PathBuf::from("old.txt"),
            content: "new",
            description: "Overwritten",
        }];
        let results = vec![(PathBuf::from("old.txt"), WriteResult::Overwritten)];

        let output = format_results(&results, &files);
        assert!(output.contains("overwritten"));
    }
}
