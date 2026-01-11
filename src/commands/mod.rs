//! CLI command implementations.
//!
//! Each submodule implements a Ralph CLI command with pure core logic
//! separated from IO for testability.

pub mod cancel;
pub mod clean;
pub mod init;
pub mod loop_cmd;
pub mod revert;
pub mod status;
