//! Sessions: one `casulo run` of an agent, as the memory server sees it
//! through the harness's hooks. A session is not memory's content: nothing
//! in it ever becomes a note.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{NoteKind, RepoPath};

/// Longest session id accepted.
const ID_MAX: usize = 64;

/// The id casulo gives one run, such as `4242`. It only names a session; it
/// is not a secret and grants nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(String);

impl SessionId {
    /// Parses an id: 1 to 64 ASCII letters, digits and `-`.
    pub fn parse(value: &str) -> Result<Self> {
        let valid = !value.is_empty()
            && value.len() <= ID_MAX
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-');
        if !valid {
            bail!("invalid session id; use 1 to 64 letters, digits and -");
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SessionId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The files of the checkout and their blob hashes, as `Git::ls_files` lists them.
pub type FileSnapshot = Vec<(RepoPath, String)>;

/// What a session did, found by rule from the files and the tool calls.
/// Agents can change the files it is made from, so it is untrusted data:
/// it is shown, never acted on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// Files added, changed or removed since the session started, sorted,
    /// at most `MAX_CHANGED_KEPT`.
    pub changed: Vec<RepoPath>,
    /// How many files changed, counting those not kept in `changed`.
    pub changed_count: usize,
    /// Notes of changed files that no longer fit the code on disk.
    pub stale: Vec<(RepoPath, NoteKind)>,
}

/// Most changed files an observation keeps.
pub const MAX_CHANGED_KEPT: usize = 50;

/// A session as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    /// Its id.
    pub id: SessionId,
    /// The harness it runs, from the URL its hooks call.
    pub harness: String,
    /// When it started, as an RFC 3339 time.
    pub started_at: String,
    /// When it ended, if it did.
    pub ended_at: Option<String>,
    /// The files when it started; dropped when it ends.
    pub files_at_start: Option<FileSnapshot>,
    /// The files when its last note was accepted, or when it started if
    /// none was; dropped when it ends.
    pub files_at_last_note: Option<FileSnapshot>,
    /// Whether Stop already blocked once.
    pub blocked: bool,
    /// How many notes were accepted in it.
    pub notes_saved: i64,
    /// What it did, as of its last Stop or its end.
    pub observation: Observation,
}

/// What a Stop hook answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopDecision {
    /// Let the agent stop.
    Allow,
    /// Keep the agent going, with this text as its next prompt.
    Block(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_letters_digits_and_dashes() {
        assert_eq!(SessionId::parse("4242").unwrap().as_str(), "4242");
        assert!(SessionId::parse("run-7A").is_ok());
    }

    #[test]
    fn refuses_empty_long_and_other_characters() {
        for value in ["", "a b", "a/b", "a\nb", "é", &"a".repeat(65)] {
            assert!(SessionId::parse(value).is_err(), "{value:?}");
        }
    }
}
