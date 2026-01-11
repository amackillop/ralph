//! Revert Ralph commits.
//!
//! Core validation is pure. Git operations are injected.

use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::path::PathBuf;
use tracing::info;

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/// Runs the revert command, resetting the specified number of commits.
pub(crate) async fn run(count: u32) -> Result<()> {
    validate_count(count).map_err(|e| anyhow::anyhow!("{e}"))?;

    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    println!("{}", format_revert_start(count));

    // Get commits to revert
    let commits = git_log(&cwd, count).await?;

    print!("{}", format_commits_to_revert(&commits));

    // Perform reset
    git_reset(&cwd, count).await?;

    info!("Reverted {} commits", count);
    print!("{}", format_revert_success(count));

    Ok(())
}

// -----------------------------------------------------------------------------
// Internal types
// -----------------------------------------------------------------------------

/// Error conditions for revert.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum RevertError {
    #[error("count must be greater than 0")]
    InvalidCount,
    #[error("failed to get git log")]
    #[allow(dead_code)]
    GitLogFailed,
    #[error("git reset failed: {0}")]
    #[allow(dead_code)]
    GitResetFailed(String),
}

// -----------------------------------------------------------------------------
// Helper functions
// -----------------------------------------------------------------------------

/// Validates that revert count is greater than zero.
fn validate_count(count: u32) -> Result<(), RevertError> {
    if count == 0 {
        Err(RevertError::InvalidCount)
    } else {
        Ok(())
    }
}

/// Parses git log output into commit summary lines.
fn parse_commits(log_output: &str) -> Vec<String> {
    log_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

/// Formats the revert start message.
fn format_revert_start(count: u32) -> String {
    format!(
        "\n{} Reverting last {} Ralph commit(s)...",
        "⚠".yellow(),
        count.to_string().cyan()
    )
}

/// Formats the list of commits being reverted.
fn format_commits_to_revert(commits: &[String]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(&mut out, "\nCommits to revert:").unwrap();
    for line in commits {
        writeln!(&mut out, "  {}", line.dimmed()).unwrap();
    }
    out
}

/// Formats the revert success message.
fn format_revert_success(count: u32) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        &mut out,
        "\n{} Successfully reverted {} commit(s).",
        "✓".green(),
        count.to_string().cyan()
    )
    .unwrap();
    writeln!(
        &mut out,
        "  {}",
        "Use 'git reflog' to recover if needed.".dimmed()
    )
    .unwrap();
    out
}

// -----------------------------------------------------------------------------
// Git operations
// -----------------------------------------------------------------------------

async fn git_log(cwd: &PathBuf, count: u32) -> Result<Vec<String>> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "--oneline", "-n", &count.to_string()])
        .output()
        .await
        .context("Failed to get git log")?;

    if !output.status.success() {
        bail!("Failed to get git log");
    }

    Ok(parse_commits(&String::from_utf8_lossy(&output.stdout)))
}

async fn git_reset(cwd: &PathBuf, count: u32) -> Result<()> {
    let reset_ref = format!("HEAD~{count}");
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["reset", "--hard", &reset_ref])
        .output()
        .await
        .context("Failed to run git reset")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Git reset failed: {stderr}");
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_count_zero() {
        assert_eq!(validate_count(0), Err(RevertError::InvalidCount));
    }

    #[test]
    fn test_validate_count_positive() {
        assert!(validate_count(1).is_ok());
        assert!(validate_count(5).is_ok());
        assert!(validate_count(100).is_ok());
    }

    #[test]
    fn test_parse_commits_single() {
        let log = "abc1234 Fix bug";
        let commits = parse_commits(log);
        assert_eq!(commits, vec!["abc1234 Fix bug"]);
    }

    #[test]
    fn test_parse_commits_multiple() {
        let log = "abc1234 Fix bug\ndef5678 Add feature\nghi9012 Update docs";
        let commits = parse_commits(log);
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0], "abc1234 Fix bug");
        assert_eq!(commits[1], "def5678 Add feature");
        assert_eq!(commits[2], "ghi9012 Update docs");
    }

    #[test]
    fn test_parse_commits_empty() {
        let commits = parse_commits("");
        assert!(commits.is_empty());
    }

    #[test]
    fn test_parse_commits_with_empty_lines() {
        let log = "abc1234 Fix bug\n\ndef5678 Add feature\n";
        let commits = parse_commits(log);
        assert_eq!(commits.len(), 2);
    }

    #[test]
    fn test_revert_error_display() {
        assert_eq!(
            RevertError::InvalidCount.to_string(),
            "count must be greater than 0"
        );
        assert_eq!(
            RevertError::GitResetFailed("error msg".to_string()).to_string(),
            "git reset failed: error msg"
        );
    }

    #[test]
    fn test_format_revert_start() {
        let output = format_revert_start(3);
        assert!(output.contains("Reverting"));
        assert!(output.contains('3'));
    }

    #[test]
    fn test_format_commits_to_revert() {
        let commits = vec![
            "abc1234 First commit".to_string(),
            "def5678 Second commit".to_string(),
        ];
        let output = format_commits_to_revert(&commits);
        assert!(output.contains("Commits to revert"));
        assert!(output.contains("abc1234"));
        assert!(output.contains("def5678"));
    }

    #[test]
    fn test_format_revert_success() {
        let output = format_revert_success(2);
        assert!(output.contains("Successfully reverted"));
        assert!(output.contains('2'));
        assert!(output.contains("git reflog"));
    }
}
