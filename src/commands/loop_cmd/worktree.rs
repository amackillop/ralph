//! Git worktree management for parallel branch builds.
//!
//! Handles creation, configuration, and removal of worktrees for each branch
//! in the implementation plan.

use anyhow::{bail, Context, Result};
use std::path::Path;
use tokio::process::Command;

use crate::config::WorktreeConfig;

/// Directory where worktrees are created.
const WORKTREE_DIR: &str = ".worktrees";

/// A parsed branch section from `IMPLEMENTATION_PLAN.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub struct BranchSection {
    /// Branch name (from `## Branch: <name>`).
    pub name: String,
    /// Branch goal (from `Goal: <description>`).
    pub goal: String,
    /// Base branch to branch from (from `Base: <branch>`).
    pub base: String,
}

/// Parse `IMPLEMENTATION_PLAN.md` and extract all branch sections.
///
/// Expected format:
/// ```markdown
/// ## Branch: <name>
/// Goal: <description>
/// Base: <branch>
///
/// - [ ] Task 1
/// - [ ] Task 2
/// ```
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub fn parse_implementation_plan(content: &str) -> Vec<BranchSection> {
    let mut sections = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_goal: Option<String> = None;
    let mut current_base: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for branch header
        if let Some(name) = trimmed.strip_prefix("## Branch:") {
            // Save previous section if complete
            if let (Some(name), Some(goal), Some(base)) = (
                current_name.take(),
                current_goal.take(),
                current_base.take(),
            ) {
                sections.push(BranchSection { name, goal, base });
            }
            current_name = Some(name.trim().to_string());
            current_goal = None;
            current_base = None;
        } else if let Some(goal) = trimmed.strip_prefix("Goal:") {
            current_goal = Some(goal.trim().to_string());
        } else if let Some(base) = trimmed.strip_prefix("Base:") {
            current_base = Some(base.trim().to_string());
        }
    }

    // Don't forget the last section
    if let (Some(name), Some(goal), Some(base)) = (
        current_name.take(),
        current_goal.take(),
        current_base.take(),
    ) {
        sections.push(BranchSection { name, goal, base });
    }

    sections
}

/// Enable worktree configuration in git.
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub async fn enable_worktree_config(project_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(project_dir)
        .args(["config", "extensions.worktreeConfig", "true"])
        .status()
        .await
        .context("Failed to run git config")?;

    if !status.success() {
        bail!("Failed to enable worktree config");
    }
    Ok(())
}

/// Create a new worktree for a branch.
///
/// Runs: `git worktree add .worktrees/<branch> -b <branch>`
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub async fn create_worktree(project_dir: &Path, branch: &str) -> Result<()> {
    let worktree_path = format!("{WORKTREE_DIR}/{branch}");

    let output = Command::new("git")
        .current_dir(project_dir)
        .args(["worktree", "add", &worktree_path, "-b", branch])
        .output()
        .await
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create worktree for branch '{branch}': {stderr}");
    }
    Ok(())
}

/// Configure worktree identity using git config --worktree.
///
/// Sets `user.name`, `user.email`, and optionally `signing_key` and `ssh_key`.
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub async fn configure_worktree_identity(
    project_dir: &Path,
    branch: &str,
    config: &WorktreeConfig,
) -> Result<()> {
    let worktree_path = project_dir.join(WORKTREE_DIR).join(branch);

    // Set user.name
    run_git_config(&worktree_path, "user.name", &config.name).await?;

    // Set user.email
    run_git_config(&worktree_path, "user.email", &config.email).await?;

    // Set signing key if configured
    if let Some(ref key) = config.signing_key {
        run_git_config(&worktree_path, "user.signingkey", key).await?;
        run_git_config(&worktree_path, "commit.gpgsign", "true").await?;
    }

    // Set SSH command if configured
    if let Some(ref ssh_key) = config.ssh_key {
        let ssh_command = format!("ssh -i {ssh_key} -o IdentitiesOnly=yes");
        run_git_config(&worktree_path, "core.sshCommand", &ssh_command).await?;
    }

    Ok(())
}

/// Run a git config command in the worktree.
async fn run_git_config(worktree_path: &Path, key: &str, value: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(worktree_path)
        .args(["config", "--worktree", key, value])
        .output()
        .await
        .context("Failed to run git config")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to set {key}: {stderr}");
    }
    Ok(())
}

/// Remove a worktree.
///
/// Runs: `git worktree remove .worktrees/<branch>`
pub async fn remove_worktree(project_dir: &Path, branch: &str) -> Result<()> {
    let worktree_path = format!("{WORKTREE_DIR}/{branch}");

    let output = Command::new("git")
        .current_dir(project_dir)
        .args(["worktree", "remove", &worktree_path, "--force"])
        .output()
        .await
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to remove worktree for branch '{branch}': {stderr}");
    }
    Ok(())
}

/// Remove all worktrees in the .worktrees directory.
pub async fn remove_all_worktrees(project_dir: &Path) -> Result<Vec<String>> {
    let worktrees_dir = project_dir.join(WORKTREE_DIR);

    if !worktrees_dir.exists() {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    let entries = std::fs::read_dir(&worktrees_dir)
        .with_context(|| format!("Failed to read {WORKTREE_DIR}"))?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let branch = entry.file_name().to_string_lossy().to_string();
            remove_worktree(project_dir, &branch).await?;
            removed.push(branch);
        }
    }

    // Clean up the .worktrees directory if empty
    if worktrees_dir.exists() {
        let _ = std::fs::remove_dir(&worktrees_dir);
    }

    Ok(removed)
}

/// Copy `IMPLEMENTATION_PLAN.md` to a worktree.
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub fn copy_plan_to_worktree(project_dir: &Path, branch: &str) -> Result<()> {
    let src = project_dir.join("IMPLEMENTATION_PLAN.md");
    let dest = project_dir
        .join(WORKTREE_DIR)
        .join(branch)
        .join("IMPLEMENTATION_PLAN.md");

    if src.exists() {
        std::fs::copy(&src, &dest)
            .with_context(|| format!("Failed to copy IMPLEMENTATION_PLAN.md to {branch}"))?;
    }

    Ok(())
}

/// Get the path to a worktree.
#[allow(dead_code)] // Used by parallel-build (not yet implemented)
pub fn worktree_path(project_dir: &Path, branch: &str) -> std::path::PathBuf {
    project_dir.join(WORKTREE_DIR).join(branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_implementation_plan_single_branch() {
        let content = r"
# Implementation Plan

## Branch: fix-bug
Goal: Fix the critical bug
Base: master

- [ ] Task 1
- [ ] Task 2
";
        let sections = parse_implementation_plan(content);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "fix-bug");
        assert_eq!(sections[0].goal, "Fix the critical bug");
        assert_eq!(sections[0].base, "master");
    }

    #[test]
    fn test_parse_implementation_plan_multiple_branches() {
        let content = r"
## Branch: feature-a
Goal: Add feature A
Base: master

- [ ] Task 1

## Branch: feature-b
Goal: Add feature B
Base: develop

- [ ] Task 2
";
        let sections = parse_implementation_plan(content);
        assert_eq!(sections.len(), 2);

        assert_eq!(sections[0].name, "feature-a");
        assert_eq!(sections[0].goal, "Add feature A");
        assert_eq!(sections[0].base, "master");

        assert_eq!(sections[1].name, "feature-b");
        assert_eq!(sections[1].goal, "Add feature B");
        assert_eq!(sections[1].base, "develop");
    }

    #[test]
    fn test_parse_implementation_plan_empty() {
        let content = "# Just some text\nNo branches here";
        let sections = parse_implementation_plan(content);
        assert!(sections.is_empty());
    }

    #[test]
    fn test_parse_implementation_plan_incomplete_branch() {
        // Missing Base: line - should not be included
        let content = r"
## Branch: incomplete
Goal: Missing base

- [ ] Task 1
";
        let sections = parse_implementation_plan(content);
        assert!(sections.is_empty());
    }

    #[test]
    fn test_worktree_path() {
        let project = Path::new("/project");
        let path = worktree_path(project, "feature-x");
        assert_eq!(path, Path::new("/project/.worktrees/feature-x"));
    }
}
