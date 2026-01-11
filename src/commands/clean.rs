use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

pub async fn run(all: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    let state_file = cwd.join(".cursor").join("ralph-state.toml");

    let mut removed = Vec::new();

    // Always remove state file
    if state_file.exists() {
        fs::remove_file(&state_file).context("Failed to remove state file")?;
        removed.push(".cursor/ralph-state.toml");
    }

    // Optionally remove all Ralph files
    if all {
        let files_to_remove = [
            "ralph.toml",
            "PROMPT_plan.md",
            "PROMPT_build.md",
            "AGENTS.md",
            ".cursor/rules/ralph.mdc",
        ];

        for file in &files_to_remove {
            let path = cwd.join(file);
            if path.exists() {
                fs::remove_file(&path).with_context(|| format!("Failed to remove {}", file))?;
                removed.push(*file);
            }
        }

        // Remove IMPLEMENTATION_PLAN.md if exists
        let impl_plan = cwd.join("IMPLEMENTATION_PLAN.md");
        if impl_plan.exists() {
            fs::remove_file(&impl_plan).context("Failed to remove IMPLEMENTATION_PLAN.md")?;
            removed.push("IMPLEMENTATION_PLAN.md");
        }
    }

    if removed.is_empty() {
        println!("\n{} No Ralph files found to clean.", "ℹ".blue());
    } else {
        println!("\n{} Cleaned Ralph files:", "✓".green());
        for file in removed {
            println!("  {} {}", "✗".red(), file.dimmed());
        }
    }

    Ok(())
}
