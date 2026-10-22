//! Arguments of `casulo index`.

use clap::Subcommand;

use super::MemoryArgs;

/// Subcommands of `casulo index`.
#[derive(Debug, Subcommand)]
pub enum IndexCommand {
    /// Read what changed in the checkout and the local branches into memory
    Memory(MemoryArgs),
}
