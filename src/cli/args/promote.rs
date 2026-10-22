//! Arguments of `casulo promote`.

use clap::Subcommand;

use super::MemoryArgs;

/// Subcommands of `casulo promote`.
#[derive(Debug, Subcommand)]
pub enum PromoteCommand {
    /// Turn the current branch's note on a topic into a general project note
    Memory(PromoteMemoryArgs),
}

/// Arguments of `casulo promote memory`.
#[derive(Debug, clap::Args)]
pub struct PromoteMemoryArgs {
    #[command(flatten)]
    pub memory: MemoryArgs,
    /// What to promote
    #[command(subcommand)]
    pub target: PromoteMemoryTarget,
}

/// What `casulo promote memory` turns general.
#[derive(Debug, Subcommand)]
pub enum PromoteMemoryTarget {
    /// The current branch's notes on this topic
    Topic {
        /// Its name, such as runtime
        topic: String,
    },
}
