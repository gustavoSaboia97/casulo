//! `casulo config`: opens the global casulo.toml, or a project's, in the
//! editor.

use anyhow::Result;

use super::configured_editor;
use crate::application::project::services::edit_config;
use crate::cli::args::ConfigArgs;
use crate::cli::environment;
use crate::infrastructure::config_file::ConfigFiles;
use crate::infrastructure::editor::EditorProcess;

/// `casulo config`: prints the file's path, or opens it in the editor.
pub fn config(arguments: ConfigArgs) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let path = match &arguments.project {
        Some(project) => home.project_config_file(project),
        None => home.global_config_file(),
    };
    if arguments.path {
        println!("{}", path.display());
        return Ok(());
    }
    // A broken global file must still open, so it can be fixed.
    let editor_command = environment::editor_from_environment(configured_editor(&home));
    match &arguments.project {
        Some(project) => edit_config::edit_project_config(
            &home,
            project,
            &editor_command,
            &EditorProcess,
            &ConfigFiles,
        ),
        None => {
            edit_config::edit_global_config(&home, &editor_command, &EditorProcess, &ConfigFiles)
        }
    }
}
