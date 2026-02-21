//! Git operations for the Ralph loop.
//!
//! This module handles all git interactions during loop execution:
//! push, branch detection, commit hash retrieval, and commit counting.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use tracing::{debug, info};

/// Push current changes to the remote repository.
///
/// Refuses to push to protected branches as a safety measure.
pub(crate) async fn git_push(cwd: &Path, protected_branches: &[String]) -> Result<()> {
    debug!("Pushing to git...");

    // Check if current branch is protected
    let branch = get_current_branch(cwd).await?;
    if protected_branches.iter().any(|b| b == &branch) {
        bail!(
            "Refusing to push to protected branch '{branch}'. \
             Remove it from git.protected_branches in ralph.toml to allow pushing."
        );
    }

    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["push"])
        .output()
        .await
        .context("Failed to run git push")?;

    if !output.status.success() {
        // Try to create upstream branch
        tokio::process::Command::new("git")
            .current_dir(cwd)
            .args(["push", "-u", "origin", &branch])
            .output()
            .await
            .context("Failed to push with upstream")?;
    }

    info!("Git push complete");
    Ok(())
}

/// Get the name of the current git branch.
pub(crate) async fn get_current_branch(cwd: &Path) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["branch", "--show-current"])
        .output()
        .await
        .context("Failed to get current branch")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the last commit message (first line only).
pub(crate) async fn get_last_commit_message(cwd: &Path) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "-1", "--pretty=%s"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

/// Count successful commits since loop started (commits with timestamps after `started_at`).
pub(crate) async fn count_successful_commits(cwd: &Path, started_at: DateTime<Utc>) -> u32 {
    // Format with explicit UTC timezone so git interprets it correctly regardless of local timezone
    let since_str = started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let output = match tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "--since", &since_str, "--pretty=format:%H"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };

    let commits = String::from_utf8_lossy(&output.stdout);
    let count = commits.lines().filter(|l| !l.is_empty()).count();
    // Truncate to u32::MAX if count exceeds u32 range (unlikely in practice)
    u32::try_from(count.min(u32::MAX as usize)).unwrap_or(u32::MAX)
}

/// Create a pull request using the `gh` CLI.
///
/// Returns the PR URL on success.
pub(crate) async fn create_pull_request(
    cwd: &Path,
    branch: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<String> {
    info!(
        "Creating pull request for branch '{}' -> '{}'",
        branch, base
    );

    let output = tokio::process::Command::new("gh")
        .current_dir(cwd)
        .args([
            "pr", "create", "--base", base, "--head", branch, "--title", title, "--body", body,
        ])
        .output()
        .await
        .context("Failed to run gh pr create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create PR for branch '{branch}': {stderr}");
    }

    let pr_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    info!("Created PR: {}", pr_url);
    Ok(pr_url)
}

/// Check if `gh` CLI is available and authenticated.
pub(crate) async fn check_gh_available() -> bool {
    tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .await
        .is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_push_rejects_protected_branch() {
        use std::process::Command;

        let cwd = std::env::current_dir().unwrap();

        // Check if we're in a git repo
        let Ok(git_output) = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&cwd)
            .output()
        else {
            return; // Git not available
        };
        if !git_output.status.success() {
            return; // Not in a git repo
        }

        // Get current branch
        let Ok(branch) = get_current_branch(&cwd).await else {
            return; // Couldn't get branch
        };

        // Call git_push with current branch in protected list - should fail
        let protected = vec![branch.clone()];
        let result = git_push(&cwd, &protected).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("protected branch"));
        assert!(err.contains(&branch));
    }

    #[tokio::test]
    async fn test_git_push_allows_non_protected_branch() {
        use std::process::Command;

        let cwd = std::env::current_dir().unwrap();

        // Check if we're in a git repo
        let Ok(git_output) = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&cwd)
            .output()
        else {
            return; // Git not available
        };
        if !git_output.status.success() {
            return; // Not in a git repo
        }

        // Get current branch (just to verify we can)
        let Ok(_branch) = get_current_branch(&cwd).await else {
            return; // Couldn't get branch
        };

        // Protected branches that don't match current branch
        let protected = vec![
            "this-branch-does-not-exist-1234567890".to_string(),
            "another-nonexistent-branch".to_string(),
        ];

        // Call git_push - it should not fail due to protected branch check
        // (it may fail for other reasons like no remote, but that's a different error)
        let result = git_push(&cwd, &protected).await;

        // If it failed, it shouldn't be because of protected branch
        if let Err(e) = result {
            assert!(
                !e.to_string().contains("protected branch"),
                "Should not fail due to protected branch"
            );
        }
        // Success or other failure is fine
    }
}
