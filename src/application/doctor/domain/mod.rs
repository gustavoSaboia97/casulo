//! The host and what the doctor finds on it. Everything here is already
//! valid once built.

pub mod doctor;
pub mod host;

// Re-exported so other modules can write `doctor::domain::HostOs`.
pub use doctor::{Finding, Requirement, ServerStatus, Status};
pub use host::{Fix, HostOs, LinuxDistro, Version};
