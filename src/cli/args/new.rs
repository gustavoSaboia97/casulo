//! Arguments of `casulo new`.

use crate::application::shared::domain::ProjectName;

/// Arguments of `casulo new`.
#[derive(Debug, clap::Args)]
pub struct NewArgs {
    /// Git URL to clone, or a directory to use in place, such as `.` or ./app
    // Kept as text here and parsed by `project_source`: clap repeats invalid
    // values in its errors, and a URL may carry a token that must not be shown.
    pub source: String,
    /// Project name [default: the repository or directory name]
    // `value_parser` runs `parse` while reading the arguments, so an invalid
    // name is reported by clap like any other bad argument.
    #[arg(long, value_parser = ProjectName::parse)]
    pub name: Option<ProjectName>,
}
