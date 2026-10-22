//! Arguments of `casulo delete`.

use super::ResourceArgs;

/// Arguments of `casulo delete`.
#[derive(Debug, clap::Args)]
pub struct DeleteArgs {
    #[command(flatten)]
    pub resource: ResourceArgs,
    /// Delete without asking
    #[arg(long)]
    pub yes: bool,
}
