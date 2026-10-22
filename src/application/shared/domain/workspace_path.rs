//! A path inside the project workspace, such as `.env` or `config/secrets`.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// A relative path that stays inside the workspace, written with `/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// Parses a path relative to the workspace. Absolute paths, `..` and
    /// Windows separators are rejected, so it can never point outside.
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim_end_matches('/');
        let inside = !trimmed.is_empty()
            && !trimmed.starts_with('/')
            && !trimmed.contains(['\\', ':'])
            && trimmed
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !inside {
            bail!(
                "invalid workspace path \"{value}\": use a path inside the workspace, such as .env or config/secrets.json"
            );
        }
        Ok(Self(trimmed.to_string()))
    }

    /// The path under `workspace` on the host.
    pub fn on_host(&self, workspace: &Path) -> PathBuf {
        self.0
            .split('/')
            .fold(workspace.to_path_buf(), |path, part| path.join(part))
    }

    /// The path under `workspace` inside the container, which is always Linux.
    pub fn in_container(&self, workspace: &str) -> String {
        format!("{workspace}/{}", self.0)
    }
}

impl fmt::Display for WorkspacePath {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for WorkspacePath {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<WorkspacePath> for String {
    fn from(path: WorkspacePath) -> Self {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_paths_inside_the_workspace() {
        for value in [".env", "config/secrets.json", "deploy/keys/"] {
            assert!(WorkspacePath::parse(value).is_ok(), "{value}");
        }
        assert_eq!(WorkspacePath::parse("keys/").unwrap().to_string(), "keys");
    }

    #[test]
    fn rejects_paths_that_could_leave_the_workspace() {
        for value in [
            "",
            "/",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "./.env",
            "a//b",
            "C:\\x",
            "a\\b",
        ] {
            assert!(WorkspacePath::parse(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn maps_to_host_and_container_paths() {
        let path = WorkspacePath::parse("config/.env").unwrap();

        assert_eq!(
            path.on_host(Path::new("workspace")),
            Path::new("workspace").join("config").join(".env")
        );
        assert_eq!(path.in_container("/workspace"), "/workspace/config/.env");
    }
}
