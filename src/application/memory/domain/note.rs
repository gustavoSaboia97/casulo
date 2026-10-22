//! Notes: what agents and the user know about the code that the code does
//! not say.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::event::NoteText;
use super::repo_path::RepoPath;
use super::topic::Topic;

/// Shortest summary accepted, so a summary says something.
const SUMMARY_MIN: usize = 10;
/// Longest summary accepted, so it fits on one line in a list.
const SUMMARY_MAX: usize = 150;
/// Longest body accepted, so notes stay notes and not documentation.
const BODY_MAX: usize = 2_000;

/// What a note says about its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    /// What a file is for.
    Purpose,
    /// A rule the code follows.
    Convention,
    /// A trap someone fell into.
    Gotcha,
    /// A choice and why it was made.
    Decision,
}

impl NoteKind {
    /// Every kind, in the order notes are shown.
    pub const ALL: [NoteKind; 4] = [
        Self::Purpose,
        Self::Convention,
        Self::Gotcha,
        Self::Decision,
    ];

    /// Parses the name used in MCP and in note files, such as `gotcha`.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "purpose" => Ok(Self::Purpose),
            "convention" => Ok(Self::Convention),
            "gotcha" => Ok(Self::Gotcha),
            "decision" => Ok(Self::Decision),
            _ => {
                bail!("invalid note kind \"{value}\": use purpose, convention, gotcha or decision")
            }
        }
    }

    /// The name used in MCP and in note files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Purpose => "purpose",
            Self::Convention => "convention",
            Self::Gotcha => "gotcha",
            Self::Decision => "decision",
        }
    }

    /// Whether a project or branch note may have this kind: only conventions
    /// and decisions apply to more than one file.
    pub fn allowed_for_project(self) -> bool {
        matches!(self, Self::Convention | Self::Decision)
    }
}

impl fmt::Display for NoteKind {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether a file's note fits the code checked out now. Computed for the
/// checkout, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteStatus {
    /// Written for the content on disk now.
    Ok,
    /// Written only for other versions of the file.
    Stale,
    /// Two different texts were proposed for the content on disk now.
    Conflict,
}

/// One stored version of a file note of a given `(path, kind)`: the content
/// it was written for and whether it is in conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteVersion {
    /// Git blob hash of the file content the note was written for.
    pub code_hash: String,
    /// Whether two texts were proposed for that content.
    pub in_conflict: bool,
}

/// The status of a `(path, kind)` for the file content now on disk
/// (`current_hash`), given every stored version of its note. `None` when the
/// file has no note of that kind at all.
pub fn note_status(versions: &[NoteVersion], current_hash: &str) -> Option<NoteStatus> {
    if let Some(current) = versions
        .iter()
        .find(|version| version.code_hash == current_hash)
    {
        return Some(if current.in_conflict {
            NoteStatus::Conflict
        } else {
            NoteStatus::Ok
        });
    }
    (!versions.is_empty()).then_some(NoteStatus::Stale)
}

/// How two files are linked in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// `use` of another module.
    Imports,
    /// `mod` declaring a child module.
    Contains,
    /// Cited in a note's refs.
    SeeAlso,
}

impl EdgeKind {
    /// The name stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imports => "imports",
            Self::Contains => "contains",
            Self::SeeAlso => "see_also",
        }
    }
}

/// Who wrote a note. Set by casulo, never claimed by the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    /// An agent, named after its harness, such as `claude`.
    Harness(String),
    /// The LLM that writes purpose notes for files without one.
    Writer,
    /// The person, through `casulo edit memory`.
    User,
}

impl Author {
    /// The text stored in the database: `harness:<name>`, `writer` or
    /// `user`, so a harness can never be mistaken for the user.
    pub fn to_text(&self) -> String {
        match self {
            Self::Harness(name) => format!("harness:{name}"),
            Self::Writer => "writer".to_string(),
            Self::User => "user".to_string(),
        }
    }

    /// Reads the text written by `to_text`.
    pub fn from_text(text: &str) -> Result<Self> {
        match text {
            "writer" => Ok(Self::Writer),
            "user" => Ok(Self::User),
            _ => match text.strip_prefix("harness:") {
                Some(name) if !name.is_empty() => Ok(Self::Harness(name.to_string())),
                _ => bail!("invalid note author \"{text}\""),
            },
        }
    }
}

/// Where a note belongs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteTarget {
    /// One file, for one version of its content.
    File {
        /// The file.
        path: RepoPath,
        /// Git blob hash of the content the note was written for.
        code_hash: String,
    },
    /// The whole project, on every branch.
    Project {
        /// The note's name.
        topic: Topic,
    },
    /// The whole project, only while this branch is checked out.
    Branch {
        /// The branch name.
        branch: String,
        /// The note's name.
        topic: Topic,
    },
}

/// The one-line summary of a note, 10 to 150 characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NoteSummary(String);

impl NoteSummary {
    /// Parses a summary: one line, trimmed, 10 to 150 characters.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let length = value.chars().count();
        if value.contains(['\n', '\r']) {
            bail!("the summary must be one line; put details in the body");
        }
        if !(SUMMARY_MIN..=SUMMARY_MAX).contains(&length) {
            bail!(
                "the summary has {length} characters; use {SUMMARY_MIN} to {SUMMARY_MAX} and put details in the body"
            );
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the summary as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The body of a note, up to 2,000 characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NoteBody(String);

impl NoteBody {
    /// Parses a body: trimmed, up to 2,000 characters, possibly empty.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let length = value.chars().count();
        if length > BODY_MAX {
            bail!("the body has {length} characters; keep it under {BODY_MAX}");
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the body as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NoteSummary {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<NoteSummary> for String {
    fn from(summary: NoteSummary) -> Self {
        summary.0
    }
}

impl TryFrom<String> for NoteBody {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<NoteBody> for String {
    fn from(body: NoteBody) -> Self {
        body.0
    }
}

/// What `remember` carries once the Inbox has validated it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteProposal {
    /// Where the note belongs.
    pub target: NoteTarget,
    /// What it says. Project and branch notes are only conventions or decisions.
    pub kind: NoteKind,
    /// One line.
    pub summary: NoteSummary,
    /// The details.
    pub body: NoteBody,
    /// Up to 10 related files, each one an indexed file.
    pub refs: Vec<RepoPath>,
    /// Who wrote it.
    pub author: Author,
}

/// A file note as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFileNote {
    /// Its id.
    pub id: i64,
    /// The file.
    pub path: RepoPath,
    /// What it says.
    pub kind: NoteKind,
    /// Its text.
    pub text: NoteText,
    /// Related files.
    pub refs: Vec<RepoPath>,
    /// Git blob hash of the content it was written for.
    pub code_hash: String,
    /// The other text, when two were proposed for the same content.
    pub conflict: Option<NoteText>,
    /// How many times the same text was proposed again.
    pub reinforced: i64,
    /// Who wrote it.
    pub author: Author,
}

/// A project or branch note as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProjectNote {
    /// Its id.
    pub id: i64,
    /// The branch, or `None` for a general note.
    pub branch: Option<String>,
    /// Its name.
    pub topic: String,
    /// What it says.
    pub kind: NoteKind,
    /// Its text.
    pub text: NoteText,
    /// The other text, when two were proposed.
    pub conflict: Option<NoteText>,
    /// How many times the same text was proposed again.
    pub reinforced: i64,
    /// Who wrote it.
    pub author: Author,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(code_hash: &str, in_conflict: bool) -> NoteVersion {
        NoteVersion {
            code_hash: code_hash.to_string(),
            in_conflict,
        }
    }

    #[test]
    fn status_is_ok_for_a_note_on_the_current_content() {
        let versions = [version("old", false), version("now", false)];

        assert_eq!(note_status(&versions, "now"), Some(NoteStatus::Ok));
    }

    #[test]
    fn status_is_stale_when_only_other_versions_have_notes() {
        let versions = [version("old", false), version("older", true)];

        assert_eq!(note_status(&versions, "now"), Some(NoteStatus::Stale));
    }

    #[test]
    fn status_is_conflict_when_the_current_note_is_in_conflict() {
        let versions = [version("old", false), version("now", true)];

        assert_eq!(note_status(&versions, "now"), Some(NoteStatus::Conflict));
    }

    #[test]
    fn no_status_without_notes() {
        assert_eq!(note_status(&[], "now"), None);
    }

    #[test]
    fn kinds_round_trip_and_reject_unknown_names() {
        for kind in NoteKind::ALL {
            assert_eq!(NoteKind::parse(kind.as_str()).unwrap(), kind);
        }
        assert!(NoteKind::parse("todo").is_err());
        assert!(NoteKind::parse("Purpose").is_err());
    }

    #[test]
    fn only_conventions_and_decisions_apply_to_the_project() {
        let allowed: Vec<NoteKind> = NoteKind::ALL
            .into_iter()
            .filter(|kind| kind.allowed_for_project())
            .collect();

        assert_eq!(allowed, [NoteKind::Convention, NoteKind::Decision]);
    }

    #[test]
    fn summary_is_one_line_of_10_to_150_characters() {
        assert_eq!(
            NoteSummary::parse("  Containers run read-only  ")
                .unwrap()
                .as_str(),
            "Containers run read-only"
        );
        assert!(NoteSummary::parse(&"é".repeat(150)).is_ok());
        for value in [
            "too short",
            &"a".repeat(151),
            "first line\nsecond line",
            "          ",
        ] {
            assert!(NoteSummary::parse(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn authors_round_trip_and_a_harness_is_never_the_user() {
        for author in [
            Author::Harness("claude".to_string()),
            Author::Writer,
            Author::User,
        ] {
            assert_eq!(Author::from_text(&author.to_text()).unwrap(), author);
        }
        assert_eq!(
            Author::Harness("user".to_string()).to_text(),
            "harness:user"
        );
        assert!(Author::from_text("harness:").is_err());
        assert!(Author::from_text("admin").is_err());
    }

    #[test]
    fn body_is_at_most_2000_characters() {
        assert!(NoteBody::parse("").is_ok());
        assert!(NoteBody::parse(&"a".repeat(2_000)).is_ok());
        assert!(NoteBody::parse(&"a".repeat(2_001)).is_err());
    }
}
