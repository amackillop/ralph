//! Sandbox abstraction for isolated agent execution.
//!
//! Provides a trait-based interface for sandboxed execution, with
//! Docker as the primary implementation.

mod docker;
mod error;
mod network;
mod noop;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

pub(crate) use docker::DockerSandbox;
pub(crate) use error::SandboxError;
#[allow(unused_imports)] // Available for tests and future use
pub(crate) use noop::NoopSandbox;

/// Trait for sandbox execution backends.
///
/// Implementations provide isolated environments for running AI agents.
/// The Docker implementation runs agents in containers with configurable
/// resource limits and network policies. The Noop implementation does
/// nothing and is useful for testing.
#[async_trait]
pub(crate) trait Sandbox: Send + Sync {
    /// Cleans up orphaned resources from previous runs.
    ///
    /// For Docker, this removes containers with names matching `ralph-*`.
    /// Returns the number of resources cleaned up.
    async fn cleanup_orphaned(&self) -> Result<u32>;

    /// Creates a persistent container/environment for reuse across iterations.
    ///
    /// Returns an identifier for the created resource, or empty string if
    /// the implementation doesn't support persistence.
    async fn create_persistent(&self, project_dir: &Path) -> Result<String>;

    /// Removes a persistent container/environment by its identifier.
    async fn remove_persistent(&self, id: &str) -> Result<()>;

    /// Runs the agent with the given prompt.
    ///
    /// If `reuse_id` is provided, attempts to reuse an existing environment.
    /// Returns the agent's output.
    async fn run(&self, project_dir: &Path, prompt: &str, reuse_id: Option<&str>)
        -> Result<String>;
}
