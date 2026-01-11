use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use colored::Colorize;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::agent::{AgentProvider, ClaudeProvider, CursorProvider, Provider};
use crate::config::Config;
use crate::detection::CompletionDetector;
use crate::state::{Mode, RalphState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LoopMode {
    Plan,
    Build,
}

impl From<LoopMode> for Mode {
    fn from(mode: LoopMode) -> Self {
        match mode {
            LoopMode::Plan => Mode::Plan,
            LoopMode::Build => Mode::Build,
        }
    }
}

pub async fn run(
    mode: LoopMode,
    max_iterations: u32,
    completion_promise: Option<String>,
    no_sandbox: bool,
    custom_prompt: Option<String>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Load configuration
    let config = Config::load(&cwd).context("Failed to load ralph.toml")?;

    // Determine prompt file
    let prompt_file = match custom_prompt {
        Some(p) => PathBuf::from(p),
        None => match mode {
            LoopMode::Plan => cwd.join("PROMPT_plan.md"),
            LoopMode::Build => cwd.join("PROMPT_build.md"),
        },
    };

    if !prompt_file.exists() {
        bail!(
            "Prompt file not found: {}\nRun 'cursor-ralph init' to create default files.",
            prompt_file.display()
        );
    }

    // Load or create state
    let mut state = RalphState::load_or_create(&cwd, mode.into())?;

    // Update state with CLI options
    state.max_iterations = if max_iterations > 0 {
        Some(max_iterations)
    } else {
        None
    };
    state.completion_promise = completion_promise.clone();
    state.active = true;
    state.save(&cwd)?;

    // Get the configured agent provider
    let provider = config.agent.get_provider()?;

    // Print startup banner
    print_banner(&state, &prompt_file, no_sandbox, &config, provider);

    // Create the agent provider
    let agent: Box<dyn AgentProvider> = match provider {
        Provider::Cursor => Box::new(CursorProvider::new(config.agent.cursor.clone())),
        Provider::Claude => Box::new(ClaudeProvider::new(config.agent.claude.clone())),
    };

    let detector = CompletionDetector::new(completion_promise.as_deref());

    // Warn about sandbox (not yet implemented for multi-provider)
    if !no_sandbox && config.sandbox.enabled {
        warn!("Docker sandbox is not yet implemented for the provider system. Running without sandbox.");
    }

    // Main loop
    loop {
        // Check max iterations
        if let Some(max) = state.max_iterations {
            if state.iteration > max {
                println!("\n{} Max iterations ({}) reached.", "🛑".red(), max);
                state.active = false;
                state.save(&cwd)?;
                break;
            }
        }

        println!(
            "\n{} Iteration {} {}",
            "━".repeat(20).dimmed(),
            state.iteration.to_string().cyan().bold(),
            "━".repeat(20).dimmed()
        );

        // Read prompt
        let prompt = std::fs::read_to_string(&prompt_file)
            .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))?;

        // Run agent
        info!(
            "Running {} agent iteration {}",
            agent.name(),
            state.iteration
        );
        let output = agent.invoke(&cwd, &prompt).await?;

        // Update last iteration timestamp
        state.last_iteration_at = Some(chrono::Utc::now());
        state.save(&cwd)?;

        // Check for completion
        if detector.is_complete(&output, &cwd)? {
            println!(
                "\n{} Completion detected: {}",
                "✅".green(),
                state
                    .completion_promise
                    .as_deref()
                    .unwrap_or("task complete")
            );
            state.active = false;
            state.save(&cwd)?;
            break;
        }

        // Git operations
        if config.git.auto_push {
            if let Err(e) = git_push(&cwd).await {
                warn!("Git push failed: {}", e);
            }
        }

        // Increment iteration
        state.iteration += 1;
        state.save(&cwd)?;
    }

    println!("\n{} Ralph loop finished.", "🎉".green());
    println!("  Total iterations: {}", state.iteration.to_string().cyan());

    Ok(())
}

fn print_banner(
    state: &RalphState,
    prompt_file: &std::path::Path,
    no_sandbox: bool,
    config: &Config,
    provider: Provider,
) {
    println!("\n{}", "━".repeat(50).dimmed());
    println!("{}", "   🔄 Ralph Loop Starting".yellow().bold());
    println!("{}", "━".repeat(50).dimmed());

    println!("  Agent:      {}", provider.to_string().cyan().bold());
    println!("  Mode:       {}", format!("{:?}", state.mode).cyan());
    println!("  Prompt:     {}", prompt_file.display().to_string().cyan());
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

    let sandbox_status = if no_sandbox || !config.sandbox.enabled {
        "disabled".red()
    } else {
        "enabled".green()
    };
    println!("  Sandbox:    {}", sandbox_status);

    println!("{}", "━".repeat(50).dimmed());
    println!("\n  {} to stop\n", "Ctrl+C or 'ralph cancel'".dimmed());
}

async fn git_push(cwd: &PathBuf) -> Result<()> {
    debug!("Pushing to git...");

    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["push"])
        .output()
        .await
        .context("Failed to run git push")?;

    if !output.status.success() {
        // Try to create upstream branch
        let branch = get_current_branch(cwd).await?;
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

async fn get_current_branch(cwd: &PathBuf) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(["branch", "--show-current"])
        .output()
        .await
        .context("Failed to get current branch")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
