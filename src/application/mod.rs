//! The logic layer, split by group. Each group has its own domain models,
//! services and ports (see Architecture in `AGENTS.md`).

pub mod agent;
pub mod doctor;
pub mod memory;
pub mod project;
pub mod sandbox;
pub mod shared;
