use anyhow::{Context, Result};
use colored::Colorize;

use crate::state::RalphState;

pub async fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    match RalphState::load(&cwd)? {
        Some(mut state) => {
            let iteration = state.iteration;
            state.active = false;
            state.save(&cwd)?;

            println!(
                "\n{} Ralph loop cancelled (was at iteration {}).",
                "✓".green(),
                iteration.to_string().cyan()
            );
        }
        None => {
            println!("\n{} No active Ralph loop found.", "ℹ".blue());
        }
    }

    Ok(())
}
