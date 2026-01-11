use anyhow::{bail, Context, Result};
use colored::Colorize;
use tracing::info;

pub async fn run(count: u32) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    if count == 0 {
        bail!("Count must be greater than 0");
    }

    println!(
        "\n{} Reverting last {} Ralph commit(s)...",
        "⚠".yellow(),
        count.to_string().cyan()
    );

    // Get the commits to revert
    let output = tokio::process::Command::new("git")
        .current_dir(&cwd)
        .args(["log", "--oneline", "-n", &count.to_string()])
        .output()
        .await
        .context("Failed to get git log")?;

    if !output.status.success() {
        bail!("Failed to get git log");
    }

    let commits = String::from_utf8_lossy(&output.stdout);
    println!("\nCommits to revert:");
    for line in commits.lines() {
        println!("  {}", line.dimmed());
    }

    // Reset to before these commits
    let reset_ref = format!("HEAD~{}", count);
    let output = tokio::process::Command::new("git")
        .current_dir(&cwd)
        .args(["reset", "--hard", &reset_ref])
        .output()
        .await
        .context("Failed to run git reset")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Git reset failed: {}", stderr);
    }

    info!("Reverted {} commits", count);
    println!(
        "\n{} Successfully reverted {} commit(s).",
        "✓".green(),
        count.to_string().cyan()
    );
    println!("  {}", "Use 'git reflog' to recover if needed.".dimmed());

    Ok(())
}
