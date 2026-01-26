//! No-op sandbox implementation for testing and non-sandboxed execution.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

use super::Sandbox;

/// A no-op sandbox that doesn't actually isolate anything.
///
/// Useful for:
/// - Testing without Docker
/// - Running agents without sandbox isolation (--no-sandbox flag)
/// - Unit tests that need a Sandbox implementation
#[derive(Debug, Default, Clone)]
#[allow(dead_code)] // Available for tests and future use
pub(crate) struct NoopSandbox;

impl NoopSandbox {
    /// Creates a new `NoopSandbox`.
    #[allow(dead_code)] // Available for tests and future use
    pub(crate) fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Sandbox for NoopSandbox {
    async fn cleanup_orphaned(&self) -> Result<u32> {
        // Nothing to clean up
        Ok(0)
    }

    async fn create_persistent(&self, _project_dir: &Path) -> Result<String> {
        // Return empty string to indicate no persistence
        Ok(String::new())
    }

    async fn remove_persistent(&self, _id: &str) -> Result<()> {
        // Nothing to remove
        Ok(())
    }

    async fn run(
        &self,
        _project_dir: &Path,
        _prompt: &str,
        _reuse_id: Option<&str>,
    ) -> Result<String> {
        // Return empty output - caller should handle this case
        // by running the agent directly without sandboxing
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_noop_sandbox_cleanup() {
        let sandbox = NoopSandbox::new();
        let result = sandbox.cleanup_orphaned().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_noop_sandbox_create_persistent() {
        let sandbox = NoopSandbox::new();
        let temp_dir = tempdir().unwrap();
        let result = sandbox.create_persistent(temp_dir.path()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_noop_sandbox_remove_persistent() {
        let sandbox = NoopSandbox::new();
        let result = sandbox.remove_persistent("any-id").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_noop_sandbox_run() {
        let sandbox = NoopSandbox::new();
        let temp_dir = tempdir().unwrap();
        let result = sandbox.run(temp_dir.path(), "test prompt", None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_noop_sandbox_run_with_reuse_id() {
        let sandbox = NoopSandbox::new();
        let temp_dir = tempdir().unwrap();
        let result = sandbox
            .run(temp_dir.path(), "test prompt", Some("container-id"))
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_noop_sandbox_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoopSandbox>();
    }

    #[test]
    fn test_noop_sandbox_is_zero_sized() {
        let sandbox = NoopSandbox;
        assert!(std::mem::size_of_val(&sandbox) == 0);
    }
}
