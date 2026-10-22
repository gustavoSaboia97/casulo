//! Arguments of `casulo config`.

use crate::application::shared::domain::ProjectName;

/// Arguments of `casulo config`.
#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    /// Open this project's casulo.toml instead of the global one
    #[arg(long, value_parser = ProjectName::parse)]
    pub project: Option<ProjectName>,
    /// Only print the file's path
    #[arg(long)]
    pub path: bool,
}
