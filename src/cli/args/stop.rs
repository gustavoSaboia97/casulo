//! Arguments of `casulo stop`.

use clap::Subcommand;

use super::MemoryArgs;

/// Subcommands of `casulo stop`.
#[derive(Debug, Subcommand)]
pub enum StopCommand {
    /// Stop the project's memory server now, even with sessions running
    Memory(MemoryArgs),
}
