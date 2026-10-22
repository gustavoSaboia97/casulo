//! Harnesses and the resources casulo gives an agent. Everything here is
//! already valid once built.

pub mod harness;
pub mod hooks;
pub mod mcp_server;
pub mod skill_file;
pub mod subagent;
pub mod workflow;

// Re-exported so other modules can write `agent::domain::Harness`.
pub use harness::{ConfigFile, Harness};
pub use mcp_server::McpServer;
pub use skill_file::SkillFile;
pub use subagent::Subagent;
pub use workflow::Workflow;
