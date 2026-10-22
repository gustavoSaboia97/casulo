//! The name of a project managed by casulo.

use std::fmt;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::name;

/// A valid project name, used as the directory name under `projects/`.
// The field is private, so the only way to get a `ProjectName` is `parse`, and
// every `ProjectName` in the program is known to be valid.
// `try_from`/`into` make serde go through `parse` when reading TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectName(String);

impl ProjectName {
    /// Parses a project name, failing with a message for the user if it is invalid.
    pub fn parse(value: &str) -> Result<Self> {
        name::check("project name", value)?;
        Ok(Self(value.to_string()))
    }

    /// Returns the name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Makes a project name from free text such as a folder name: lowercase
    /// ASCII letters and digits, with a single `-` for anything else.
    /// `None` when nothing valid is left.
    pub fn slug_of(text: &str) -> Option<Self> {
        let mut name = String::new();
        for character in text.chars() {
            if character.is_ascii_alphanumeric() {
                name.push(character.to_ascii_lowercase());
            } else if !name.ends_with('-') {
                name.push('-');
            }
        }
        Self::parse(name.trim_matches('-')).ok()
    }
}

// Lets `format!("{name}")` and `println!` show the name.
impl fmt::Display for ProjectName {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

// Used by serde (`try_from = "String"`) when reading a name from a file.
impl TryFrom<String> for ProjectName {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

// Used by serde (`into = "String"`) when writing a name to a file.
impl From<ProjectName> for String {
    fn from(name: ProjectName) -> Self {
        name.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_name() {
        let name = ProjectName::parse("my-project").expect("valid name");

        assert_eq!(name.as_str(), "my-project");
        assert_eq!(name.to_string(), "my-project");
    }

    #[test]
    fn rejects_invalid_name() {
        let error = ProjectName::parse("../escape").expect_err("path traversal is invalid");

        assert!(error.to_string().contains("invalid project name"));
    }
}
