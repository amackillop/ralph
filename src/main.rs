use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod agent;
mod commands;
mod config;
mod detection;
mod sandbox;
mod state;
mod templates;

#[derive(Parser)]
#[command(name = "ralph")]
#[command(
    author,
    version,
    about = "Ralph Wiggum technique - iterative AI development loops"
)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Ralph files in the current project
    Init {
        /// Force overwrite existing files
        #[arg(short, long)]
        force: bool,
    },

    /// Start a Ralph loop
    Loop {
        /// Mode: plan or build
        #[arg(value_enum, default_value = "build")]
        mode: commands::loop_cmd::LoopMode,

        /// Maximum number of iterations (0 = unlimited)
        #[arg(short, long, default_value = "0")]
        max_iterations: u32,

        /// Completion promise phrase
        #[arg(short, long)]
        completion_promise: Option<String>,

        /// Disable Docker sandbox
        #[arg(long)]
        no_sandbox: bool,

        /// Custom prompt file (overrides default)
        #[arg(short, long)]
        prompt: Option<String>,
    },

    /// Show current Ralph loop status
    Status,

    /// Cancel active Ralph loop
    Cancel,

    /// Revert Ralph commits
    Revert {
        /// Number of commits to revert
        #[arg(long, default_value = "1")]
        last: u32,
    },

    /// Remove Ralph state files
    Clean {
        /// Also remove prompt and rules files
        #[arg(long)]
        all: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("ralph=debug")
    } else {
        EnvFilter::new("ralph=info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    match cli.command {
        Commands::Init { force } => {
            commands::init::run(force).await?;
        }
        Commands::Loop {
            mode,
            max_iterations,
            completion_promise,
            no_sandbox,
            prompt,
        } => {
            commands::loop_cmd::run(mode, max_iterations, completion_promise, no_sandbox, prompt)
                .await?;
        }
        Commands::Status => {
            commands::status::run().await?;
        }
        Commands::Cancel => {
            commands::cancel::run().await?;
        }
        Commands::Revert { last } => {
            commands::revert::run(last).await?;
        }
        Commands::Clean { all } => {
            commands::clean::run(all).await?;
        }
    }

    Ok(())
}
