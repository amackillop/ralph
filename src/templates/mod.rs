//! Embedded templates for Ralph files

/// Default ralph.toml configuration
pub const RALPH_TOML: &str = include_str!("ralph.toml");

/// Planning mode prompt template
pub const PROMPT_PLAN: &str = include_str!("prompt_plan.md");

/// Building mode prompt template
pub const PROMPT_BUILD: &str = include_str!("prompt_build.md");

/// Cursor rules file for Ralph
pub const RULES_MDC: &str = include_str!("rules.mdc");

/// AGENTS.md template
pub const AGENTS_MD: &str = include_str!("agents.md");
