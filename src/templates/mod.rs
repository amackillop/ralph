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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Validates that `ralph.toml` template is valid TOML that deserializes into Config.
    /// This catches:
    /// - TOML syntax errors
    /// - Schema mismatches between template and config structs
    /// - Invalid default values
    #[test]
    fn ralph_toml_template_parses_into_config() {
        let config: Config = toml::from_str(RALPH_TOML)
            .expect("ralph.toml template should be valid TOML that deserializes into Config");

        // Verify key default values match what the template specifies
        assert_eq!(config.agent.provider, "cursor");
        assert!(config.sandbox.enabled);
        assert_eq!(config.sandbox.image, "ralph:latest");
        assert!(config.git.auto_push);
        assert_eq!(config.completion.idle_threshold, 2);
        assert!(config.validation.enabled);
        assert_eq!(config.validation.command, "nix flake check --quiet");
        assert_eq!(config.monitoring.max_consecutive_errors, 5);
    }

    /// Validates that `ralph.toml` template is non-empty and reasonable size.
    #[test]
    fn ralph_toml_template_has_content() {
        assert!(!RALPH_TOML.is_empty());
        assert!(
            RALPH_TOML.len() > 100,
            "Template should have substantial content"
        );
        assert!(
            RALPH_TOML.contains("[agent]"),
            "Template should have agent section"
        );
        assert!(
            RALPH_TOML.contains("[sandbox]"),
            "Template should have sandbox section"
        );
        assert!(
            RALPH_TOML.contains("[validation]"),
            "Template should have validation section"
        );
    }

    /// Validates `prompt_plan.md` template is non-empty valid UTF-8.
    #[test]
    fn prompt_plan_template_has_content() {
        assert!(!PROMPT_PLAN.is_empty());
        assert!(
            PROMPT_PLAN.len() > 50,
            "Plan prompt should have meaningful content"
        );
        // Should contain key planning keywords
        assert!(
            PROMPT_PLAN.contains("spec") || PROMPT_PLAN.contains("plan"),
            "Plan prompt should reference planning concepts"
        );
    }

    /// Validates `prompt_build.md` template is non-empty valid UTF-8.
    #[test]
    fn prompt_build_template_has_content() {
        assert!(!PROMPT_BUILD.is_empty());
        assert!(
            PROMPT_BUILD.len() > 50,
            "Build prompt should have meaningful content"
        );
        // Should contain key build keywords
        assert!(
            PROMPT_BUILD.contains("implement") || PROMPT_BUILD.contains("test"),
            "Build prompt should reference implementation concepts"
        );
    }

    /// Validates `rules.mdc` template is non-empty valid UTF-8.
    #[test]
    fn rules_mdc_template_has_content() {
        assert!(!RULES_MDC.is_empty());
        assert!(
            RULES_MDC.len() > 50,
            "Rules file should have meaningful content"
        );
    }

    /// Validates `agents.md` template is non-empty valid UTF-8.
    #[test]
    fn agents_md_template_has_content() {
        assert!(!AGENTS_MD.is_empty());
        assert!(
            AGENTS_MD.len() > 50,
            "AGENTS.md should have meaningful content"
        );
    }

    /// Ensures all templates are valid UTF-8 (they are &str, so this is compile-time guaranteed,
    /// but this test documents the expectation and verifies no embedded null bytes).
    #[test]
    fn all_templates_are_clean_utf8() {
        for (name, content) in [
            ("ralph.toml", RALPH_TOML),
            ("prompt_plan.md", PROMPT_PLAN),
            ("prompt_build.md", PROMPT_BUILD),
            ("rules.mdc", RULES_MDC),
            ("agents.md", AGENTS_MD),
        ] {
            assert!(
                !content.contains('\0'),
                "{name} should not contain null bytes"
            );
            // Verify it's printable (no control chars except newlines/tabs)
            for (i, c) in content.chars().enumerate() {
                assert!(
                    !c.is_control() || c == '\n' || c == '\r' || c == '\t',
                    "{name} contains unexpected control character at position {i}"
                );
            }
        }
    }
}
