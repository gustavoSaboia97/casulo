//! What `casulo describe memory note <path>` shows: a file's neighbors in
//! the graph and its notes, with their status for the checkout.

use anyhow::{Result, bail};

use crate::application::memory::domain::event::Edge;
use crate::application::memory::domain::{Author, NoteKind, NoteStatus, RepoPath};
use crate::application::memory::ports::MemoryStore;
use crate::application::memory::services::applicable::applicable;

/// A file as memory knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDetails {
    /// The file.
    pub path: RepoPath,
    /// Blob hash of its content in the checkout, `None` when it is not
    /// checked out (only notes kept for other branches remain).
    pub blob_hash: Option<String>,
    /// Files it links to.
    pub links_to: Vec<Edge>,
    /// Files that link to it; `to` is the other file.
    pub linked_from: Vec<Edge>,
    /// One line per note kind.
    pub notes: Vec<NoteLine>,
}

/// The note of one kind that applies to the checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLine {
    /// What it says.
    pub kind: NoteKind,
    /// Its status for the checkout; `None` when the file is not checked out.
    pub status: Option<NoteStatus>,
    /// Who wrote it.
    pub author: Author,
    /// Its summary.
    pub summary: String,
}

/// The details of `path`. Fails when memory knows nothing about it.
pub fn file_details(database: &dyn MemoryStore, path: &RepoPath) -> Result<FileDetails> {
    let blob_hash = database.file_hash(path)?;
    let notes = database.file_notes(path)?;
    if blob_hash.is_none() && notes.is_empty() {
        bail!("memory knows nothing about {path}; check the path, or run `casulo index memory`");
    }
    let mut lines = Vec::new();
    for kind in NoteKind::ALL {
        let Some((shown, status)) = applicable(&notes, kind, blob_hash.as_deref()) else {
            continue;
        };
        lines.push(NoteLine {
            kind,
            status,
            author: shown.author.clone(),
            summary: shown.text.summary.as_str().to_string(),
        });
    }
    Ok(FileDetails {
        path: path.clone(),
        links_to: database.edges_from(path)?,
        linked_from: database.edges_to(path)?,
        blob_hash,
        notes: lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::{AcceptedNote, Event};
    use crate::application::memory::domain::{EdgeKind, NoteBody, NoteSummary, NoteTarget};
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-09T12:00:00Z";

    fn path(value: &str) -> RepoPath {
        RepoPath::parse(value).unwrap()
    }

    fn indexed(file: &str, hash: &str, to: &[(&str, EdgeKind)]) -> Event {
        Event::FileIndexed {
            path: path(file),
            blob_hash: hash.to_string(),
            edges: to
                .iter()
                .map(|(other, kind)| Edge {
                    to: path(other),
                    kind: *kind,
                })
                .collect(),
            symbols: Vec::new(),
        }
    }

    fn note(file: &str, hash: &str, kind: NoteKind, summary: &str) -> Event {
        Event::NoteAccepted {
            note: AcceptedNote {
                target: NoteTarget::File {
                    path: path(file),
                    code_hash: hash.to_string(),
                },
                kind,
                summary: NoteSummary::parse(summary).unwrap(),
                body: NoteBody::parse("").unwrap(),
                refs: Vec::new(),
                author: Author::Harness("codex".to_string()),
            },
        }
    }

    fn database() -> Database {
        let mut database = Database::open_in_memory().unwrap();
        for event in [
            indexed("src/main.rs", "m1", &[("src/cli.rs", EdgeKind::Contains)]),
            indexed("src/cli.rs", "c2", &[]),
            note(
                "src/cli.rs",
                "c1",
                NoteKind::Purpose,
                "Parses the command line",
            ),
            note(
                "src/cli.rs",
                "c2",
                NoteKind::Gotcha,
                "Clap repeats invalid values",
            ),
            note(
                "src/gone.rs",
                "g1",
                NoteKind::Purpose,
                "Kept for another branch",
            ),
        ] {
            database.apply(&event, AT).unwrap();
        }
        database
    }

    #[test]
    fn shows_neighbors_both_ways_and_notes_with_status() {
        let details = file_details(&database(), &path("src/cli.rs")).unwrap();

        assert_eq!(details.blob_hash.as_deref(), Some("c2"));
        assert!(details.links_to.is_empty());
        assert_eq!(details.linked_from[0].to, path("src/main.rs"));
        let statuses: Vec<_> = details
            .notes
            .iter()
            .map(|line| (line.kind, line.status))
            .collect();
        assert_eq!(
            statuses,
            [
                (NoteKind::Purpose, Some(NoteStatus::Stale)),
                (NoteKind::Gotcha, Some(NoteStatus::Ok))
            ]
        );
    }

    #[test]
    fn a_file_not_checked_out_shows_its_notes_without_status() {
        let details = file_details(&database(), &path("src/gone.rs")).unwrap();

        assert_eq!(details.blob_hash, None);
        assert_eq!(details.notes[0].status, None);
    }

    #[test]
    fn an_unknown_file_is_an_error_that_says_what_to_do() {
        let error = file_details(&database(), &path("src/nothing.rs")).unwrap_err();

        assert!(error.to_string().contains("casulo index memory"));
    }
}
