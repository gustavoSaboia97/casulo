//! The name of a project note.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Longest topic accepted, short enough for a file name everywhere.
const MAX_LENGTH: usize = 63;

/// A short project-note name such as `error-handling`: lowercase letters,
/// digits and single hyphens, starting and ending with a letter or digit.
/// It is used as a file name under `notes/_project/`, so it can never
/// escape that folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Topic(String);

impl Topic {
    /// Parses a topic, failing with a message for the user if it is invalid.
    pub fn parse(value: &str) -> Result<Self> {
        let allowed = value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
        let edges_alphanumeric = value
            .starts_with(|character: char| character.is_ascii_alphanumeric())
            && value.ends_with(|character: char| character.is_ascii_alphanumeric());
        if value.is_empty()
            || value.len() > MAX_LENGTH
            || !allowed
            || !edges_alphanumeric
            || value.contains("--")
        {
            bail!(
                "invalid topic \"{value}\": use lowercase letters, digits and single hyphens, such as error-handling, at most {MAX_LENGTH} characters"
            );
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the topic as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Topic {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<Topic> for String {
    fn from(topic: Topic) -> Self {
        topic.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs() {
        for value in ["error-handling", "anyhow", "v2", "a"] {
            assert!(Topic::parse(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_anything_else() {
        let too_long = "a".repeat(MAX_LENGTH + 1);
        for value in [
            "",
            "Error",
            "-a",
            "a-",
            "a--b",
            "a_b",
            "a/b",
            "../x",
            "a b",
            too_long.as_str(),
        ] {
            assert!(Topic::parse(value).is_err(), "{value:?}");
        }
    }
}
