use anyhow::{Context, Result};
use colored::Colorize;

use crate::state::RalphState;

pub async fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    match RalphState::load(&cwd)? {
        Some(state) => {
            println!("\n{}", "━".repeat(50).dimmed());
            println!("{}", "   🔄 Ralph Loop Status".yellow().bold());
            println!("{}", "━".repeat(50).dimmed());

            let status = if state.active {
                "active".green().bold()
            } else {
                "inactive".red()
            };
            println!("  Status:     {}", status);

            println!("  Mode:       {}", format!("{:?}", state.mode).cyan());
            println!("  Iteration:  {}", state.iteration.to_string().cyan());
            println!(
                "  Max:        {}",
                state
                    .max_iterations
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "unlimited".to_string())
                    .cyan()
            );
            println!(
                "  Promise:    {}",
                state.completion_promise.as_deref().unwrap_or("none").cyan()
            );
            println!(
                "  Started:    {}",
                state
                    .started_at
                    .format("%Y-%m-%d %H:%M:%S UTC")
                    .to_string()
                    .cyan()
            );

            if let Some(last) = state.last_iteration_at {
                println!(
                    "  Last iter:  {}",
                    last.format("%Y-%m-%d %H:%M:%S UTC").to_string().cyan()
                );
            }

            println!("{}", "━".repeat(50).dimmed());
        }
        None => {
            println!("\n{} No active Ralph loop found.", "ℹ".blue());
            println!("  Run {} to start one.", "cursor-ralph loop".green());
        }
    }

    Ok(())
}
