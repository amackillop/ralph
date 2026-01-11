//! Docker sandbox for isolated agent execution.
//!
//! Provides container-based isolation for running AI agents with
//! configurable network policies, resource limits, and volume mounts.

mod docker;
mod network;

// Sandbox is not yet integrated with the multi-provider system
#[allow(unused_imports)]
pub(crate) use docker::SandboxRunner;
