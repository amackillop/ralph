//! Domain-specific error types for sandbox operations.
//!
//! Typed errors enable callers to match on specific failure modes
//! rather than parsing error message strings.

use std::time::Duration;

/// Errors that can occur during sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Docker daemon is not running or not accessible.
    #[error("Docker is not available: {message}")]
    DockerUnavailable { message: String },

    /// Container image was not found.
    #[error("Container image not found: {image}")]
    ImageNotFound { image: String },

    /// Container execution exceeded the configured timeout.
    #[error("Container execution timed out after {timeout_secs} seconds")]
    Timeout { timeout_secs: u64 },

    /// Container is in an unrecoverable state (dead, removing, etc.).
    #[error("Container is unhealthy: {message}")]
    ContainerUnhealthy { message: String },

    /// Network/iptables setup failed.
    #[error("Network setup failed: {message}")]
    NetworkSetupFailed { message: String },

    /// Container operation failed (create, start, exec, etc.).
    #[error("Container operation failed: {message}")]
    ContainerFailed { message: String },
}

impl SandboxError {
    /// Creates a `DockerUnavailable` error.
    pub fn docker_unavailable(message: impl Into<String>) -> Self {
        Self::DockerUnavailable {
            message: message.into(),
        }
    }

    /// Creates an `ImageNotFound` error.
    pub fn image_not_found(image: impl Into<String>) -> Self {
        Self::ImageNotFound {
            image: image.into(),
        }
    }

    /// Creates a `Timeout` error from a `Duration`.
    pub fn timeout(duration: Duration) -> Self {
        Self::Timeout {
            timeout_secs: duration.as_secs(),
        }
    }

    /// Creates a `ContainerUnhealthy` error.
    pub fn container_unhealthy(message: impl Into<String>) -> Self {
        Self::ContainerUnhealthy {
            message: message.into(),
        }
    }

    /// Creates a `NetworkSetupFailed` error.
    pub fn network_setup_failed(message: impl Into<String>) -> Self {
        Self::NetworkSetupFailed {
            message: message.into(),
        }
    }

    /// Creates a `ContainerFailed` error.
    pub fn container_failed(message: impl Into<String>) -> Self {
        Self::ContainerFailed {
            message: message.into(),
        }
    }

    /// Returns true if this is a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    /// Returns true if this is a Docker unavailability error.
    #[allow(dead_code)] // Public API for callers
    pub fn is_docker_unavailable(&self) -> bool {
        matches!(self, Self::DockerUnavailable { .. })
    }

    /// Returns true if this is an image not found error.
    #[allow(dead_code)] // Public API for callers
    pub fn is_image_not_found(&self) -> bool {
        matches!(self, Self::ImageNotFound { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_unavailable_error() {
        let err = SandboxError::docker_unavailable("daemon not running");
        assert!(err.is_docker_unavailable());
        assert!(!err.is_timeout());
        assert_eq!(
            err.to_string(),
            "Docker is not available: daemon not running"
        );
    }

    #[test]
    fn test_image_not_found_error() {
        let err = SandboxError::image_not_found("ralph:latest");
        assert!(err.is_image_not_found());
        assert_eq!(err.to_string(), "Container image not found: ralph:latest");
    }

    #[test]
    fn test_timeout_error() {
        let err = SandboxError::timeout(Duration::from_secs(3600));
        assert!(err.is_timeout());
        assert_eq!(
            err.to_string(),
            "Container execution timed out after 3600 seconds"
        );
    }

    #[test]
    fn test_container_unhealthy_error() {
        let err = SandboxError::container_unhealthy("container is dead");
        assert!(!err.is_timeout());
        assert_eq!(err.to_string(), "Container is unhealthy: container is dead");
    }

    #[test]
    fn test_network_setup_failed_error() {
        let err = SandboxError::network_setup_failed("iptables not found");
        assert_eq!(err.to_string(), "Network setup failed: iptables not found");
    }

    #[test]
    fn test_container_failed_error() {
        let err = SandboxError::container_failed("failed to start");
        assert_eq!(
            err.to_string(),
            "Container operation failed: failed to start"
        );
    }

    #[test]
    fn test_error_variants_are_distinct() {
        let timeout = SandboxError::timeout(Duration::from_secs(60));
        let docker = SandboxError::docker_unavailable("test");
        let image = SandboxError::image_not_found("test");

        assert!(timeout.is_timeout());
        assert!(!timeout.is_docker_unavailable());
        assert!(!timeout.is_image_not_found());

        assert!(!docker.is_timeout());
        assert!(docker.is_docker_unavailable());
        assert!(!docker.is_image_not_found());

        assert!(!image.is_timeout());
        assert!(!image.is_docker_unavailable());
        assert!(image.is_image_not_found());
    }
}
