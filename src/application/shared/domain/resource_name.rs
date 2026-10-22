//! The name of one piece of the user's agent setup: a subagent, a skill or
//! an MCP server.

use std::fmt;

use anyhow::{Result, bail};

use super::name;

/// A slug such as `code-reviewer`: the project name rules, plus no `--`, as
/// the Agent Skills spec requires. It is used as a file or folder name, and
/// can never escape its folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceName(String);

impl ResourceName {
    /// Parses a resource name, failing with a message for the user if it is invalid.
    pub fn parse(value: &str) -> Result<Self> {
        name::check("name", value)?;
        if value.contains("--") {
            bail!(
                "invalid name \"{value}\": use single hyphens between words, such as code-reviewer"
            );
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceName {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs() {
        for value in ["code-reviewer", "a", "context7", "x-1-y"] {
            assert!(ResourceName::parse(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_double_hyphens_and_unsafe_names() {
        for value in [
            "code--reviewer",
            "Code",
            "../x",
            "a/b",
            "",
            "-a",
            "nul",
            "a.md",
        ] {
            assert!(ResourceName::parse(value).is_err(), "{value:?}");
        }
    }
}
