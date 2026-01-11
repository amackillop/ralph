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

/// Common domains that should typically be allowed
#[allow(dead_code)]
pub const COMMON_ALLOWED_DOMAINS: &[&str] = &[
    // Git hosting
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    // Package registries
    "registry.npmjs.org",
    "pypi.org",
    "crates.io",
    "rubygems.org",
    "maven.org",
    // AI APIs
    "api.anthropic.com",
    "api.openai.com",
    // CDNs commonly used by package managers
    "cdn.jsdelivr.net",
    "unpkg.com",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_policy_display() {
        assert_eq!(format!("{}", NetworkPolicy::AllowAll), "allow-all");
        assert_eq!(format!("{}", NetworkPolicy::Allowlist), "allowlist");
        assert_eq!(format!("{}", NetworkPolicy::Deny), "deny");
    }
}
