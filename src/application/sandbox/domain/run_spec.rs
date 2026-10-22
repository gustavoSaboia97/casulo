//! Everything needed to start one agent container, independent of the runtime.

use std::path::PathBuf;

use crate::application::shared::domain::Secret;

/// How to run one container. The runtime adapter turns it into arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    /// Image to run, such as `casulo/demo:claude`.
    pub image: String,
    /// Program and arguments to run in the container.
    pub command: Vec<String>,
    /// Workspace directory on the host, mounted at `/workspace`.
    pub workspace: PathBuf,
    /// Mount the workspace read-only, for containers that only read the code.
    pub workspace_read_only: bool,
    /// Extra bind mounts (agent state, masked files).
    pub mounts: Vec<Mount>,
    /// Environment variables for the container.
    pub environment: Vec<EnvironmentVariable>,
    /// Values for `FromHost` variables the host does not set, taken from
    /// casulo's config. They are set on the runtime process only, never on
    /// its command line.
    pub secret_values: Vec<(String, Secret)>,
    /// `uid:gid` to run as, so files in the workspace keep the host owner.
    /// Only set on Linux; Docker Desktop and Colima map owners themselves.
    pub user: Option<HostUser>,
    /// Attach a terminal, for interactive agents and shells.
    pub terminal: bool,
    /// A network to join instead of the default one, such as the network of
    /// the project's compose services.
    pub network: Option<String>,
    /// Another name the container is reached by on `network`, such as `memory`.
    pub network_alias: Option<String>,
    /// More networks to join before the program starts, such as the memory
    /// network. They need `name`.
    pub extra_networks: Vec<String>,
    /// Container name, needed to find it again. Docker picks one when unset.
    pub name: Option<String>,
    /// Start in the background instead of attached to casulo.
    pub detach: bool,
}

/// What happened when starting a named container in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachedStart {
    /// It started.
    Started,
    /// A container with that name already exists, such as one another
    /// `casulo run` started a moment before.
    NameTaken,
}

/// A user and group id on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostUser {
    pub uid: u32,
    pub gid: u32,
}

/// A host path mounted into the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Path on the host.
    pub source: PathBuf,
    /// Absolute path in the container.
    pub target: String,
    /// Whether the container may only read it.
    pub read_only: bool,
}

/// One environment variable for the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentVariable {
    /// A value that is not secret, passed as `NAME=value`.
    Value { name: String, value: String },
    /// A secret taken from the host environment by name only, so its value
    /// never appears in the command line.
    FromHost { name: String },
}

/// A command ready to start, as the runtime adapter would run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    /// The runtime binary.
    pub program: String,
    /// Its arguments. They never contain a secret value.
    pub args: Vec<String>,
}

impl CommandLine {
    /// The command as one line a person can read and paste into a POSIX
    /// shell, for `--dry-run`. `secrets_set` are the names of secret
    /// variables set on the host: they are shown in front as `NAME=***`.
    pub fn display(&self, secrets_set: &[String]) -> String {
        let mut parts: Vec<String> = secrets_set
            .iter()
            .map(|name| format!("{name}=***"))
            .collect();
        parts.push(shell_quote(&self.program));
        parts.extend(self.args.iter().map(|arg| shell_quote(arg)));
        parts.join(" ")
    }
}

/// Quotes `text` for a POSIX shell when it has characters the shell would read.
fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=,@%+".contains(character));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_masks_secrets_and_quotes_arguments() {
        let command = CommandLine {
            program: "docker".to_string(),
            args: vec![
                "run".to_string(),
                "--env=GIT_AUTHOR_NAME=Ada Lovelace".to_string(),
                "--env=ANTHROPIC_API_KEY".to_string(),
                "it's".to_string(),
                String::new(),
            ],
        };

        assert_eq!(
            command.display(&["ANTHROPIC_API_KEY".to_string()]),
            "ANTHROPIC_API_KEY=*** docker run '--env=GIT_AUTHOR_NAME=Ada Lovelace' --env=ANTHROPIC_API_KEY 'it'\\''s' ''"
        );
    }
}
