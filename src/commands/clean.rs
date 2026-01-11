//! Remove Ralph state and configuration files.
//!
//! Core logic determines which files to remove based on existence.
//! Formatting is pure. IO happens only at the top level.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

/// Files that can be cleaned
pub fn state_files() -> Vec<PathBuf> {
    vec![PathBuf::from(".cursor/ralph-state.toml")]
}

/// Additional files cleaned with --all
pub fn config_files() -> Vec<PathBuf> {
    vec![
        PathBuf::from("ralph.toml"),
        PathBuf::from("PROMPT_plan.md"),
        PathBuf::from("PROMPT_build.md"),
        PathBuf::from("AGENTS.md"),
        PathBuf::from(".cursor/rules/ralph.mdc"),
        PathBuf::from("IMPLEMENTATION_PLAN.md"),
    ]
}

/// Determine which files to remove based on existence
pub fn files_to_clean<E>(all: bool, exists: E) -> Vec<PathBuf>
where
    E: Fn(&Path) -> bool,
{
    let mut files = state_files();
    if all {
        files.extend(config_files());
    }
    files.into_iter().filter(|f| exists(f)).collect()
}

/// Remove files and return list of removed paths
pub fn clean_files<E, R>(all: bool, exists: E, mut remove: R) -> Result<Vec<PathBuf>>
where
    E: Fn(&Path) -> bool,
    R: FnMut(&Path) -> Result<()>,
{
    let to_remove = files_to_clean(all, &exists);
    let mut removed = Vec::new();

    for file in to_remove {
        remove(&file)?;
        removed.push(file);
    }

    Ok(removed)
}

/// Format the clean results as a displayable string
pub fn format_results(removed: &[PathBuf]) -> String {
    let mut out = String::new();
    if removed.is_empty() {
        writeln!(&mut out, "\n{} No Ralph files found to clean.", "ℹ".blue()).unwrap();
    } else {
        writeln!(&mut out, "\n{} Cleaned Ralph files:", "✓".green()).unwrap();
        for file in removed {
            writeln!(
                &mut out,
                "  {} {}",
                "✗".red(),
                file.display().to_string().dimmed()
            )
            .unwrap();
        }
    }
    out
}

/// Entry point: runs clean with real filesystem
pub async fn run(all: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    let removed = clean_files(
        all,
        |path| cwd.join(path).exists(),
        |path| {
            fs::remove_file(cwd.join(path))
                .with_context(|| format!("Failed to remove {}", path.display()))
        },
    )?;

    print!("{}", format_results(&removed));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[test]
    fn test_state_files() {
        let files = state_files();
        assert!(files.contains(&PathBuf::from(".cursor/ralph-state.toml")));
    }

    #[test]
    fn test_config_files() {
        let files = config_files();
        assert!(files.contains(&PathBuf::from("ralph.toml")));
        assert!(files.contains(&PathBuf::from("AGENTS.md")));
    }

    #[test]
    fn test_files_to_clean_state_only() {
        let existing: HashSet<PathBuf> = [
            PathBuf::from(".cursor/ralph-state.toml"),
            PathBuf::from("ralph.toml"),
        ]
        .into_iter()
        .collect();

        let to_clean = files_to_clean(false, |p| existing.contains(p));

        assert_eq!(to_clean.len(), 1);
        assert!(to_clean.contains(&PathBuf::from(".cursor/ralph-state.toml")));
    }

    #[test]
    fn test_files_to_clean_all() {
        let existing: HashSet<PathBuf> = [
            PathBuf::from(".cursor/ralph-state.toml"),
            PathBuf::from("ralph.toml"),
            PathBuf::from("AGENTS.md"),
        ]
        .into_iter()
        .collect();

        let to_clean = files_to_clean(true, |p| existing.contains(p));

        assert_eq!(to_clean.len(), 3);
    }

    #[test]
    fn test_files_to_clean_none_exist() {
        let to_clean = files_to_clean(true, |_| false);
        assert!(to_clean.is_empty());
    }

    #[test]
    fn test_clean_files_removes_existing() {
        let existing: HashSet<PathBuf> = [
            PathBuf::from(".cursor/ralph-state.toml"),
            PathBuf::from("ralph.toml"),
        ]
        .into_iter()
        .collect();

        let removed_files = RefCell::new(Vec::new());

        let removed = clean_files(
            true,
            |p| existing.contains(p),
            |p| {
                removed_files.borrow_mut().push(p.to_path_buf());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(removed.len(), 2);
        assert_eq!(removed_files.borrow().len(), 2);
    }

    #[test]
    fn test_clean_files_empty_when_nothing_exists() {
        let removed = clean_files(true, |_| false, |_| Ok(())).unwrap();
        assert!(removed.is_empty());
    }

    #[test]
    fn test_format_results_empty() {
        let output = format_results(&[]);
        assert!(output.contains("No Ralph files found"));
    }

    #[test]
    fn test_format_results_with_files() {
        let removed = vec![PathBuf::from("ralph.toml"), PathBuf::from("AGENTS.md")];
        let output = format_results(&removed);
        assert!(output.contains("Cleaned Ralph files"));
        assert!(output.contains("ralph.toml"));
        assert!(output.contains("AGENTS.md"));
    }
}
