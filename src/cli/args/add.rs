//! Arguments of `casulo add`.

use super::ResourceArgs;
use crate::application::agent::domain::SkillFile;

/// Arguments of `casulo add`.
#[derive(Debug, clap::Args)]
pub struct AddArgs {
    #[command(flatten)]
    pub resource: ResourceArgs,
    /// A file to create inside the skill's folder instead, such as assets/template.md
    #[arg(long, value_parser = SkillFile::parse)]
    pub file: Option<SkillFile>,
}
