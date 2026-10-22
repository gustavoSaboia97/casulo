//! The Reconciler: decides what a validated proposal does to memory. It
//! compares only against the note with the same identity, so notes never
//! duplicate and never contradict each other silently.

use anyhow::Result;

use crate::application::memory::domain::event::{AcceptedNote, Event, NoteRef, NoteText};
use crate::application::memory::domain::{Author, NoteKind, NoteProposal, NoteTarget};
use crate::application::memory::ports::MemoryStore;

/// What a proposal did, as `remember` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A new note, or one that replaced a Writer's or was written by the user.
    Accepted,
    /// The same text was already there.
    Reinforced,
    /// A different text was already there; both are kept until settled.
    Conflict,
    /// A conflict was settled.
    Resolved,
}

impl Outcome {
    /// The name `remember` returns.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Reinforced => "reinforced",
            Self::Conflict => "conflict",
            Self::Resolved => "resolved",
        }
    }
}

/// The note that has the same identity as a proposal.
struct Existing {
    reference: NoteRef,
    text: NoteText,
    conflict: Option<NoteText>,
    reinforced: i64,
    author: Author,
}

/// Applies `proposal` (recorded as `proposal_id`) to memory. A proposal
/// already handled changes nothing, so handling one twice never counts it
/// twice.
pub fn reconcile(
    database: &mut dyn MemoryStore,
    proposal_id: i64,
    proposal: &NoteProposal,
    at: &str,
) -> Result<Outcome> {
    let existing = existing_note(database, &proposal.target, proposal.kind)?;
    let proposed = NoteText {
        summary: proposal.summary.clone(),
        body: proposal.body.clone(),
    };
    if database.proposal_done(proposal_id)? {
        return Ok(match &existing {
            Some(note) if same_text(&note.text, &proposed) => Outcome::Reinforced,
            Some(_) => Outcome::Conflict,
            None => Outcome::Accepted,
        });
    }
    let accept = Event::NoteAccepted {
        note: AcceptedNote {
            target: proposal.target.clone(),
            kind: proposal.kind,
            summary: proposal.summary.clone(),
            body: proposal.body.clone(),
            refs: proposal.refs.clone(),
            author: proposal.author.clone(),
        },
    };
    let (event, outcome) = match existing {
        None => (accept, Outcome::Accepted),
        // The user has the last word: their text replaces any other.
        Some(_) if proposal.author == Author::User => (accept, Outcome::Accepted),
        Some(note) if note.conflict.is_some() => {
            let other = note.conflict.clone().unwrap_or_else(|| note.text.clone());
            // Agreeing with one side makes it win; a new text replaces both.
            // The note's author is whoever stands behind the text that
            // stays: unchanged when its own text wins, else the proposer.
            let (winner, author) = if same_text(&note.text, &proposed) {
                (note.text, None)
            } else if same_text(&other, &proposed) {
                (other, Some(proposal.author.clone()))
            } else {
                (proposed, Some(proposal.author.clone()))
            };
            (
                Event::NoteResolved {
                    note: note.reference,
                    winner,
                    author,
                },
                Outcome::Resolved,
            )
        }
        Some(note) if same_text(&note.text, &proposed) => (
            Event::NoteReinforced {
                note: note.reference,
                reinforced: note.reinforced + 1,
            },
            Outcome::Reinforced,
        ),
        // A Writer's note is a draft: any agent or the user replaces it.
        Some(note) if note.author == Author::Writer && proposal.author != Author::Writer => {
            (accept, Outcome::Accepted)
        }
        Some(note) => (
            Event::NoteConflicted {
                note: note.reference,
                proposal_id,
                other: proposed,
            },
            Outcome::Conflict,
        ),
    };
    database.apply_for_proposal(&event, proposal_id, at)?;
    Ok(outcome)
}

fn existing_note(
    database: &dyn MemoryStore,
    target: &NoteTarget,
    kind: NoteKind,
) -> Result<Option<Existing>> {
    Ok(match target {
        NoteTarget::File { path, code_hash } => database
            .find_file_note(path, kind, code_hash)?
            .map(|note| Existing {
                reference: NoteRef::File(note.id),
                text: note.text,
                conflict: note.conflict,
                reinforced: note.reinforced,
                author: note.author,
            }),
        NoteTarget::Project { topic } => project(database, None, topic.as_str(), kind)?,
        NoteTarget::Branch { branch, topic } => {
            project(database, Some(branch), topic.as_str(), kind)?
        }
    })
}

fn project(
    database: &dyn MemoryStore,
    branch: Option<&str>,
    topic: &str,
    kind: NoteKind,
) -> Result<Option<Existing>> {
    Ok(database
        .find_project_note(branch, topic, kind)?
        .map(|note| Existing {
            reference: NoteRef::Project(note.id),
            text: note.text,
            conflict: note.conflict,
            reinforced: note.reinforced,
            author: note.author,
        }))
}

/// Two texts are the same when they differ only in whitespace and case.
fn same_text(left: &NoteText, right: &NoteText) -> bool {
    let normalize = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    normalize(left.summary.as_str()) == normalize(right.summary.as_str())
        && normalize(left.body.as_str()) == normalize(right.body.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::{NoteBody, NoteSummary, RepoPath, Topic};
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-09T12:00:00Z";

    fn file_target(hash: &str) -> NoteTarget {
        NoteTarget::File {
            path: RepoPath::parse("src/main.rs").unwrap(),
            code_hash: hash.to_string(),
        }
    }

    fn proposal(target: NoteTarget, summary: &str, author: Author) -> NoteProposal {
        NoteProposal {
            target,
            kind: NoteKind::Convention,
            summary: NoteSummary::parse(summary).unwrap(),
            body: NoteBody::parse("").unwrap(),
            refs: Vec::new(),
            author,
        }
    }

    fn claude() -> Author {
        Author::Harness("claude".to_string())
    }

    fn codex() -> Author {
        Author::Harness("codex".to_string())
    }

    /// Records a proposal like the Inbox does, then reconciles it.
    fn remember(database: &mut Database, proposal: &NoteProposal) -> Outcome {
        let id = database.record_proposal(proposal, AT).unwrap();
        reconcile(database, id, proposal, AT).unwrap()
    }

    fn summaries(database: &Database) -> Vec<(String, Option<String>, i64)> {
        database
            .file_notes(&RepoPath::parse("src/main.rs").unwrap())
            .unwrap()
            .into_iter()
            .map(|note| {
                (
                    note.text.summary.as_str().to_string(),
                    note.conflict
                        .map(|other| other.summary.as_str().to_string()),
                    note.reinforced,
                )
            })
            .collect()
    }

    #[test]
    fn a_new_identity_is_accepted() {
        let mut database = Database::open_in_memory().unwrap();

        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "Runs read-only always", claude()),
        );

        assert_eq!(outcome, Outcome::Accepted);
        assert_eq!(
            summaries(&database),
            [("Runs read-only always".to_string(), None, 0)]
        );
    }

    #[test]
    fn the_same_text_reinforces_ignoring_whitespace_and_case() {
        let mut database = Database::open_in_memory().unwrap();
        remember(
            &mut database,
            &proposal(file_target("a1"), "Runs read-only always", claude()),
        );

        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "runs  READ-ONLY always", codex()),
        );

        assert_eq!(outcome, Outcome::Reinforced);
        assert_eq!(
            summaries(&database),
            [("Runs read-only always".to_string(), None, 1)]
        );
    }

    #[test]
    fn an_agent_or_the_user_replaces_a_writer_note() {
        let mut database = Database::open_in_memory().unwrap();
        remember(
            &mut database,
            &proposal(file_target("a1"), "Draft from the writer", Author::Writer),
        );

        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "What the agent learned", claude()),
        );

        assert_eq!(outcome, Outcome::Accepted);
        assert_eq!(
            summaries(&database),
            [("What the agent learned".to_string(), None, 0)]
        );
    }

    #[test]
    fn a_different_text_keeps_both_in_conflict() {
        let mut database = Database::open_in_memory().unwrap();
        remember(
            &mut database,
            &proposal(file_target("a1"), "Runs read-only always", claude()),
        );

        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "May write to a cache", codex()),
        );

        assert_eq!(outcome, Outcome::Conflict);
        assert_eq!(
            summaries(&database),
            [(
                "Runs read-only always".to_string(),
                Some("May write to a cache".to_string()),
                0
            )]
        );
    }

    #[test]
    fn agreeing_with_one_side_resolves_in_its_favor() {
        for (side, winner) in [
            ("Runs read-only always", "Runs read-only always"),
            ("May write to a cache", "May write to a cache"),
            ("A third, settled text", "A third, settled text"),
        ] {
            let mut database = Database::open_in_memory().unwrap();
            remember(
                &mut database,
                &proposal(file_target("a1"), "Runs read-only always", claude()),
            );
            remember(
                &mut database,
                &proposal(file_target("a1"), "May write to a cache", codex()),
            );

            let outcome = remember(&mut database, &proposal(file_target("a1"), side, claude()));

            assert_eq!(outcome, Outcome::Resolved);
            assert_eq!(summaries(&database), [(winner.to_string(), None, 0)]);
        }
    }

    #[test]
    fn the_note_takes_the_author_of_the_text_that_wins() {
        let mut database = Database::open_in_memory().unwrap();
        let author = |database: &Database| {
            database
                .file_notes(&RepoPath::parse("src/main.rs").unwrap())
                .unwrap()[0]
                .author
                .clone()
        };
        remember(
            &mut database,
            &proposal(file_target("a1"), "Containers run read-only", claude()),
        );
        remember(
            &mut database,
            &proposal(file_target("a1"), "Containers may write to /data", codex()),
        );

        // codex insists on its text: it wins, and the note is codex's now.
        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "Containers may write to /data", codex()),
        );

        assert_eq!(outcome, Outcome::Resolved);
        assert_eq!(author(&database), codex());

        // A later conflict settled in favor of the note's own text keeps its author.
        remember(
            &mut database,
            &proposal(file_target("a1"), "Containers never write", claude()),
        );
        remember(
            &mut database,
            &proposal(file_target("a1"), "Containers may write to /data", claude()),
        );
        assert_eq!(author(&database), codex());
    }

    #[test]
    fn the_user_always_has_the_last_word() {
        let mut database = Database::open_in_memory().unwrap();
        remember(
            &mut database,
            &proposal(file_target("a1"), "Runs read-only always", claude()),
        );

        let outcome = remember(
            &mut database,
            &proposal(file_target("a1"), "Written by the user", Author::User),
        );

        assert_eq!(outcome, Outcome::Accepted);
        assert_eq!(
            summaries(&database),
            [("Written by the user".to_string(), None, 0)]
        );
    }

    #[test]
    fn a_proposal_for_one_version_never_touches_another() {
        let mut database = Database::open_in_memory().unwrap();
        remember(
            &mut database,
            &proposal(
                file_target("main-version"),
                "Text for the main branch",
                claude(),
            ),
        );

        let outcome = remember(
            &mut database,
            &proposal(
                file_target("feature-version"),
                "Text for the feature branch",
                codex(),
            ),
        );

        assert_eq!(outcome, Outcome::Accepted);
        assert_eq!(
            summaries(&database),
            [
                ("Text for the main branch".to_string(), None, 0),
                ("Text for the feature branch".to_string(), None, 0)
            ]
        );
    }

    #[test]
    fn handling_a_proposal_twice_changes_nothing() {
        let mut database = Database::open_in_memory().unwrap();
        let first = proposal(file_target("a1"), "Runs read-only always", claude());
        let id = database.record_proposal(&first, AT).unwrap();
        reconcile(&mut database, id, &first, AT).unwrap();

        reconcile(&mut database, id, &first, AT).unwrap();

        assert_eq!(
            summaries(&database),
            [("Runs read-only always".to_string(), None, 0)]
        );
    }

    #[test]
    fn a_branch_note_never_shows_on_another_branch() {
        let mut database = Database::open_in_memory().unwrap();
        let branch_note = proposal(
            NoteTarget::Branch {
                branch: "podman".to_string(),
                topic: Topic::parse("runtime").unwrap(),
            },
            "This branch moves to Podman",
            codex(),
        );

        remember(&mut database, &branch_note);

        assert_eq!(database.project_notes(Some("podman")).unwrap().len(), 1);
        assert!(database.project_notes(Some("main")).unwrap().is_empty());
        assert!(database.project_notes(None).unwrap().is_empty());
    }
}
