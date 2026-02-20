//! Mock agent provider for testing.
//!
//! Provides a configurable mock that returns predetermined responses
//! for E2E loop testing without invoking real agent CLIs.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::AgentProvider;

/// A mock agent provider for testing.
///
/// Returns configurable responses and tracks invocations for test assertions.
#[derive(Debug, Clone)]
pub(crate) struct MockAgentProvider {
    /// Responses to return in order. Cycles if more invocations than responses.
    responses: Arc<Vec<MockResponse>>,
    /// Number of times `invoke` has been called.
    invocation_count: Arc<AtomicUsize>,
    /// Provider name for display.
    name: &'static str,
}

/// A single mock response configuration.
#[derive(Debug, Clone)]
pub(crate) enum MockResponse {
    /// Return a successful response with the given output.
    Success(String),
    /// Return an error with the given message.
    Error(String),
    /// Return an error that looks like a timeout.
    Timeout,
    /// Return an error that looks like a rate limit.
    RateLimit,
}

impl MockAgentProvider {
    /// Create a new mock provider that returns the given responses in order.
    ///
    /// If invoked more times than responses, it cycles back to the first.
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(responses),
            invocation_count: Arc::new(AtomicUsize::new(0)),
            name: "Mock",
        }
    }

    /// Create a mock that always succeeds with the given output.
    pub fn always_succeed(output: &str) -> Self {
        Self::new(vec![MockResponse::Success(output.to_string())])
    }

    /// Create a mock that always fails with the given error.
    pub fn always_fail(error: &str) -> Self {
        Self::new(vec![MockResponse::Error(error.to_string())])
    }

    /// Get the number of times `invoke` was called.
    pub fn invocation_count(&self) -> usize {
        self.invocation_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AgentProvider for MockAgentProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn invoke(&self, _project_dir: &Path, _prompt: &str) -> Result<String> {
        let count = self.invocation_count.fetch_add(1, Ordering::SeqCst);
        let response = &self.responses[count % self.responses.len()];

        match response {
            MockResponse::Success(output) => Ok(output.clone()),
            MockResponse::Error(msg) => anyhow::bail!("{msg}"),
            MockResponse::Timeout => anyhow::bail!("Agent execution timed out after 60 minutes"),
            MockResponse::RateLimit => {
                anyhow::bail!("rate limit exceeded (resource_exhausted)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider_name() {
        let provider = MockAgentProvider::always_succeed("ok");
        assert_eq!(provider.name(), "Mock");
    }

    #[tokio::test]
    async fn test_mock_provider_success() {
        let provider = MockAgentProvider::always_succeed("test output");
        let result = provider
            .invoke(Path::new("/tmp"), "test prompt")
            .await
            .unwrap();
        assert_eq!(result, "test output");
    }

    #[tokio::test]
    async fn test_mock_provider_error() {
        let provider = MockAgentProvider::always_fail("test error");
        let result = provider.invoke(Path::new("/tmp"), "test prompt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[tokio::test]
    async fn test_mock_provider_cycles_responses() {
        let provider = MockAgentProvider::new(vec![
            MockResponse::Success("first".to_string()),
            MockResponse::Success("second".to_string()),
        ]);

        let r1 = provider.invoke(Path::new("/tmp"), "").await.unwrap();
        let r2 = provider.invoke(Path::new("/tmp"), "").await.unwrap();
        let r3 = provider.invoke(Path::new("/tmp"), "").await.unwrap();

        assert_eq!(r1, "first");
        assert_eq!(r2, "second");
        assert_eq!(r3, "first"); // Cycles back
    }

    #[tokio::test]
    async fn test_mock_provider_tracks_invocations() {
        let provider = MockAgentProvider::always_succeed("ok");
        assert_eq!(provider.invocation_count(), 0);

        let _ = provider.invoke(Path::new("/tmp"), "").await;
        assert_eq!(provider.invocation_count(), 1);

        let _ = provider.invoke(Path::new("/tmp"), "").await;
        assert_eq!(provider.invocation_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_provider_timeout() {
        let provider = MockAgentProvider::new(vec![MockResponse::Timeout]);
        let result = provider.invoke(Path::new("/tmp"), "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_mock_provider_rate_limit() {
        let provider = MockAgentProvider::new(vec![MockResponse::RateLimit]);
        let result = provider.invoke(Path::new("/tmp"), "").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("resource_exhausted"));
    }
}
