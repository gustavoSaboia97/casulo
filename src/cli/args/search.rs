//! Arguments of `casulo search`.

use clap::Subcommand;

use super::MemoryArgs;

/// Subcommands of `casulo search`.
#[derive(Debug, Subcommand)]
pub enum SearchCommand {
    /// Files, their links and notes, and project notes that match the text
    Memory(SearchMemoryArgs),
}

/// Arguments of `casulo search memory`.
#[derive(Debug, clap::Args)]
pub struct SearchMemoryArgs {
    #[command(flatten)]
    pub memory: MemoryArgs,
    /// What to look for, such as "docker run arguments"
    pub text: String,
    /// How many files to return
    #[arg(long, default_value_t = 5)]
    pub limit: usize,
}
