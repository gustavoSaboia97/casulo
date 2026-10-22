//! Runs fix commands through the system shell, attached to the terminal.

use std::process::Command;

use anyhow::{Context, Result};

/// Runs one shell command line with inherited stdin, stdout and stderr, so
/// prompts from tools such as sudo or apt reach the user. Returns whether
/// the command succeeded.
pub fn run_interactive(command_line: &str) -> Result<bool> {
    let status = shell_command(command_line)
        .status()
        .with_context(|| format!("cannot run: {command_line}"))?;
    Ok(status.success())
}

// Fix commands use shell features (`$USER`, here-documents), so they need a shell.
#[cfg(windows)]
fn shell_command(command_line: &str) -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", command_line]);
    command
}

#[cfg(not(windows))]
fn shell_command(command_line: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", command_line]);
    command
}
