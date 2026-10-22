//! Arguments of `casulo harness`.

use clap::Subcommand;

/// Subcommands of `casulo harness`.
#[derive(Debug, Subcommand)]
pub enum HarnessCommand {
    /// List the available harnesses, where each one comes from and what it receives
    List,
}
