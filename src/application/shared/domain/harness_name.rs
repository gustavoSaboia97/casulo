//! The name of a harness, the description of how to install and run an agent.

use std::fmt;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::name;

/// A valid harness name, such as `claude` or `codex`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HarnessName(String);

impl HarnessName {
    /// Parses a harness name, failing with a message for the user if it is invalid.
    pub fn parse(value: &str) -> Result<Self> {
        name::check("harness name", value)?;
        Ok(Self(value.to_string()))
    }

    /// Returns the name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HarnessName {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for HarnessName {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<HarnessName> for String {
    fn from(name: HarnessName) -> Self {
        name.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_name() {
        let name = HarnessName::parse("claude").expect("valid name");

        assert_eq!(name.as_str(), "claude");
    }

    #[test]
    fn rejects_invalid_name() {
        let error = HarnessName::parse("Claude Code").expect_err("spaces are invalid");

        assert!(error.to_string().contains("invalid harness name"));
    }
}
