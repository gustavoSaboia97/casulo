//! The container, its compose services and how it is started. Everything
//! here is already valid once built.

pub mod compose;
pub mod container;
pub mod run_spec;

// Re-exported so other modules can write `sandbox::domain::RunSpec`.
pub use run_spec::{CommandLine, DetachedStart, EnvironmentVariable, HostUser, Mount, RunSpec};
