//! Docker sandbox for isolated agent execution.
//!
//! Provides container-based isolation for running AI agents with
//! configurable network policies, resource limits, and volume mounts.

mod docker;
mod network;

pub(crate) use docker::SandboxRunner;
