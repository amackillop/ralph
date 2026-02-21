//! Ralph - Iterative AI development loops using the Ralph Wiggum technique.
//!
//! Ralph orchestrates AI agents (Cursor, Claude) in iterative development loops,
//! enabling autonomous code generation with configurable completion detection,
//! iteration limits, and optional Docker sandboxing.
//!
//! # Usage
//!
//! ```bash
//! # Initialize a new project
//! ralph init
//!
//! # Start a planning loop
//! ralph loop plan
//!
//! # Start a build loop with max iterations
//! ralph loop build --max-iterations 10
//!
//! # Check status
//! ralph status
//!
//! # Cancel an active loop
//! ralph cancel
//! ```

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer, Registry,
};

mod agent;
mod commands;
mod config;
mod detection;
mod notifications;
mod sandbox;
mod state;
mod templates;

/// Set up logging with optional file appender based on config.
fn setup_logging(
    filter: EnvFilter,
    cwd: &Path,
    monitoring: &config::MonitoringConfig,
) -> Result<Option<WorkerGuard>> {
    if monitoring.log_file.is_empty() {
        Registry::default().with(fmt::layer()).with(filter).init();
        return Ok(None);
    }

    let log_file = if Path::new(&monitoring.log_file).is_absolute() {
        PathBuf::from(&monitoring.log_file)
    } else {
        cwd.join(&monitoring.log_file)
    };

    // Create parent directory if needed
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory: {}", parent.display()))?;
    }

    let rotation = match monitoring.log_rotation {
        config::LogRotation::Daily => Rotation::DAILY,
        config::LogRotation::Hourly => Rotation::HOURLY,
        config::LogRotation::Never => Rotation::NEVER,
    };
    let parent = log_file.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Log file path has no parent directory: {}",
            log_file.display()
        )
    })?;
    let file_name = log_file
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Log file path has no file name: {}", log_file.display()))?;
    let file_appender = RollingFileAppender::new(rotation, parent, file_name);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = if monitoring.log_format == "json" {
        fmt::layer()
            .with_writer(non_blocking)
            .json()
            .with_timer(ChronoUtc::rfc_3339())
            .boxed()
    } else {
        fmt::layer()
            .with_writer(non_blocking)
            .with_timer(ChronoUtc::rfc_3339())
            .boxed()
    };

    Registry::default()
        .with(fmt::layer())
        .with(file_layer)
        .with(filter)
        .init();

    Ok(Some(guard))
}

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

        /// Maximum number of iterations (default: 10 for plan, 20 for build; use --unlimited for no limit)
        #[arg(short, long)]
        max_iterations: Option<u32>,

        /// Run without iteration limit (overrides `max_iterations`)
        #[arg(long)]
        unlimited: bool,

        /// Disable Docker sandbox
        #[arg(long)]
        no_sandbox: bool,

        /// Custom prompt file (overrides default)
        #[arg(short, long)]
        prompt: Option<String>,

        /// Override agent provider (cursor or claude)
        #[arg(long)]
        provider: Option<String>,
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

        /// Remove all worktrees created by Ralph
        #[arg(long)]
        worktrees: bool,
    },

    /// Manage Docker sandbox image
    Image {
        #[command(subcommand)]
        action: commands::image::ImageAction,
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

    match cli.command {
        Commands::Init { force } => {
            Registry::default().with(fmt::layer()).with(filter).init();
            commands::init::run(force)?;
        }
        Commands::Loop {
            mode,
            max_iterations,
            unlimited,
            no_sandbox,
            prompt,
            provider,
        } => {
            // Load config to get log file settings
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            let config = config::Config::load(&cwd).context("Failed to load ralph.toml")?;

            // Set up logging with file appender (guard must stay alive for duration)
            let _file_guard = setup_logging(filter, &cwd, &config.monitoring)?;

            // Determine default max_iterations based on mode if not specified
            let effective_max = if unlimited {
                None
            } else {
                max_iterations.or({
                    Some(match mode {
                        commands::loop_cmd::LoopMode::Plan => 10,
                        commands::loop_cmd::LoopMode::Build => 20,
                    })
                })
            };

            commands::loop_cmd::run(mode, effective_max, no_sandbox, prompt, provider).await?;
        }
        Commands::Status => {
            commands::status::run()?;
        }
        Commands::Cancel => {
            commands::cancel::run()?;
        }
        Commands::Revert { last } => {
            commands::revert::run(last).await?;
        }
        Commands::Clean { all, worktrees } => {
            commands::clean::run(all, worktrees).await?;
        }
        Commands::Image { action } => {
            commands::image::run(action).await?;
        }
    }

    Ok(())
}
