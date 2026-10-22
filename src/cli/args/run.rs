//! Arguments of `casulo run` and `casulo shell`.

use super::ProjectArgs;

/// Arguments of `casulo run` and `casulo shell`.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    // `flatten` reuses the --project and --harness options of `ProjectArgs`,
    // in args/mod.rs.
    #[command(flatten)]
    pub project: ProjectArgs,
    /// Print the exact container command instead of running it
    #[arg(long)]
    pub dry_run: bool,
}
