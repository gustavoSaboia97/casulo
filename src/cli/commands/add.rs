//! `casulo add`: a subagent, skill, MCP server or workflow from a template,
//! or a file inside a skill.

use anyhow::{Result, bail};

use super::configured_editor;
use super::edit::edit_resource;
use crate::application::agent::services::resources::{self, EditOutcome};
use crate::application::shared::domain::ResourceKind;
use crate::cli::args::AddArgs;
use crate::cli::environment;
use crate::infrastructure::editor::EditorProcess;

/// `casulo add`: a resource, or with `--file` a file inside a skill.
pub fn add(arguments: AddArgs) -> Result<()> {
    let Some(file) = arguments.file else {
        return edit_resource(arguments.resource, true);
    };
    let (kind, name) = (arguments.resource.kind, &arguments.resource.name);
    if kind != ResourceKind::Skill {
        bail!(
            "--file only works with skills, such as `casulo add skill {name} --file assets/template.md`"
        );
    }
    let home = environment::casulo_home_from_environment()?;
    let editor_command = environment::editor_from_environment(configured_editor(&home));
    let outcome = resources::add_skill_file(&home, name, &file, &editor_command, &EditorProcess)?;
    match outcome {
        EditOutcome::Saved(path) => eprintln!("saved {}", path.display()),
        EditOutcome::Unchanged => {
            eprintln!("nothing saved, so \"{file}\" was not created in skill \"{name}\"")
        }
    }
    Ok(())
}
