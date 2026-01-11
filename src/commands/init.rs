use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use tracing::info;

use crate::templates;

pub async fn run(force: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    info!("Initializing Ralph in {}", cwd.display());

    // Create .cursor directory if it doesn't exist
    let cursor_dir = cwd.join(".cursor");
    fs::create_dir_all(&cursor_dir).context("Failed to create .cursor directory")?;

    // Create .cursor/rules directory
    let rules_dir = cursor_dir.join("rules");
    fs::create_dir_all(&rules_dir).context("Failed to create .cursor/rules directory")?;

    // Write ralph.toml config
    write_file_if_not_exists(&cwd.join("ralph.toml"), templates::RALPH_TOML, force)?;

    // Write prompt files
    write_file_if_not_exists(&cwd.join("PROMPT_plan.md"), templates::PROMPT_PLAN, force)?;
    write_file_if_not_exists(&cwd.join("PROMPT_build.md"), templates::PROMPT_BUILD, force)?;

    // Write rules file
    write_file_if_not_exists(&rules_dir.join("ralph.mdc"), templates::RULES_MDC, force)?;

    // Write AGENTS.md template
    write_file_if_not_exists(&cwd.join("AGENTS.md"), templates::AGENTS_MD, force)?;

    println!("\n{} Ralph initialized successfully!\n", "✓".green().bold());
    println!("Created files:");
    println!("  {} - Project configuration", "ralph.toml".cyan());
    println!("  {} - Planning mode prompt", "PROMPT_plan.md".cyan());
    println!("  {} - Building mode prompt", "PROMPT_build.md".cyan());
    println!(
        "  {} - Ralph rules for Cursor",
        ".cursor/rules/ralph.mdc".cyan()
    );
    println!(
        "  {} - Operational guide (customize this!)",
        "AGENTS.md".cyan()
    );

    println!("\n{}", "Next steps:".yellow().bold());
    println!("  1. Edit {} to select your agent", "ralph.toml".cyan());
    println!("  2. Edit {} to configure your project", "AGENTS.md".cyan());
    println!("  3. Create specs in {} directory", "specs/".cyan());
    println!(
        "  4. Run {} to generate implementation plan",
        "ralph loop plan".green()
    );
    println!("  5. Run {} to start building", "ralph loop build".green());

    Ok(())
}

fn write_file_if_not_exists(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        println!(
            "  {} {} (already exists, use --force to overwrite)",
            "⊘".yellow(),
            path.display()
        );
        return Ok(());
    }

    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;

    if path.exists() && force {
        println!("  {} {} (overwritten)", "↻".blue(), path.display());
    }

    Ok(())
}
