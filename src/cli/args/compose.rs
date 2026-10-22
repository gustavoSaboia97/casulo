//! Arguments of `casulo compose`.

use clap::Subcommand;

use crate::application::shared::domain::ProjectName;

/// Subcommands of `casulo compose`.
#[derive(Debug, Subcommand)]
pub enum ComposeCommand {
    /// Check the compose file and show what would run, without starting anything
    Check(ComposeArgs),
    /// Check, approve and start the services in the background
    Up(ComposeArgs),
    /// Stop and remove the services (named volumes are kept)
    Down(ComposeArgs),
    /// Show the state of the services
    Ps(ComposeArgs),
}

/// Arguments of the `casulo compose` subcommands.
#[derive(Debug, clap::Args)]
pub struct ComposeArgs {
    /// Project to use [default: the project of the current directory]
    #[arg(long, value_parser = ProjectName::parse)]
    pub project: Option<ProjectName>,
}
