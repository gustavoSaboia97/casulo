//! Starts the user's text editor as a child process.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::application::shared::domain::EditorCommand;
use crate::application::shared::ports::Editor;

/// The editor started as a process attached to the terminal.
pub struct EditorProcess;

impl Editor for EditorProcess {
    fn edit(&self, editor: &EditorCommand, file: &Path) -> Result<()> {
        let status = Command::new(&editor.program)
            .args(&editor.arguments)
            .arg(file)
            .status();
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => bail!(
                "the editor \"{}\" (from {}) exited with {status}",
                editor.program,
                editor.source
            ),
            Err(error) => bail!(
                "cannot start the editor \"{}\" (from {}): {error}; {}",
                editor.program,
                editor.source,
                editor.source.how_to_change()
            ),
        }
    }
}
