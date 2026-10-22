//! A path inside the repository, the same on every operating system.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// A path relative to the repository root, with `/` separators on every OS,
/// such as `src/main.rs`. It can never point outside the repository.
// `try_from`/`into` make serde go through `parse`, so an invalid path in an
// event payload is an error instead of a value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoPath(String);

impl RepoPath {
    /// Parses a path as git prints it. Rejects empty paths, absolute paths,
    /// `.` and `..` parts, empty parts, Windows separators and drive letters.
    pub fn parse(value: &str) -> Result<Self> {
        let inside = !value.is_empty()
            && !value.starts_with('/')
            && !value.contains(['\\', ':'])
            // Paths reach agents as text: a line break could forge a line of
            // memory's answer, so control characters are refused.
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !inside {
            bail!(
                "invalid repository path \"{value}\": use a path relative to the repository root with / separators, such as src/main.rs"
            );
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the path as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RepoPath {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<RepoPath> for String {
    fn from(path: RepoPath) -> Self {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_relative_paths_with_slashes() {
        for value in [
            "src/main.rs",
            "README.md",
            ".github/workflows/ci.yml",
            "a b/c.rs",
        ] {
            assert_eq!(RepoPath::parse(value).unwrap().as_str(), value);
        }
    }

    #[test]
    fn rejects_paths_that_could_leave_the_repository() {
        for value in [
            "",
            "/etc/passwd",
            "../outside",
            "src/../../x",
            "./src/main.rs",
            "src//main.rs",
            "src/",
            "src\\main.rs",
            "C:/repo/src",
            "nul\0byte",
            "line\nbreak.rs",
            "tab\there.rs",
            "carriage\rreturn.rs",
        ] {
            assert!(RepoPath::parse(value).is_err(), "{value:?}");
        }
    }
}
