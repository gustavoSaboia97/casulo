//! Arguments of `casulo describe`.

use clap::Subcommand;

use super::MemoryArgs;

/// Subcommands of `casulo describe`.
#[derive(Debug, Subcommand)]
pub enum DescribeCommand {
    /// What the project's memory holds, or details of one of its notes
    Memory(DescribeMemoryArgs),
}

/// Arguments of `casulo describe memory`.
#[derive(Debug, clap::Args)]
pub struct DescribeMemoryArgs {
    #[command(flatten)]
    pub memory: MemoryArgs,
    /// What to describe [default: memory as a whole]
    #[command(subcommand)]
    pub resource: Option<MemoryResource>,
}

/// Resources inside the project's memory.
#[derive(Debug, Subcommand)]
pub enum MemoryResource {
    /// A file's neighbors in the code graph and its notes
    Note {
        /// File path relative to the repository root, such as src/main.rs
        path: String,
    },
}
