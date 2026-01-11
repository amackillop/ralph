//! Embedded templates for Ralph files.

/// Default `ralph.toml` configuration.
pub(crate) const RALPH_TOML: &str = include_str!("ralph.toml");

/// Planning mode prompt template.
pub(crate) const PROMPT_PLAN: &str = include_str!("prompt_plan.md");

/// Building mode prompt template.
pub(crate) const PROMPT_BUILD: &str = include_str!("prompt_build.md");

/// Cursor rules file for Ralph.
pub(crate) const RULES_MDC: &str = include_str!("rules.mdc");

/// `AGENTS.md` template.
pub(crate) const AGENTS_MD: &str = include_str!("agents.md");
