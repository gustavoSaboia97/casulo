//! What `casulo describe memory` shows: how much memory holds and how much
//! of it fits the code checked out now, and how the last sessions used it.

use anyhow::Result;

use crate::application::memory::domain::NoteStatus;
use crate::application::memory::ports::MemoryStore;
use crate::application::memory::services::applicable::applicable;
use crate::application::memory::services::session::SESSIONS_KEPT;

/// A count of what memory holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryState {
    /// Files of the checkout that are indexed.
    pub files: i64,
    /// Links in the code graph.
    pub edges: i64,
    /// Stored file notes, for every version of every file.
    pub file_notes: i64,
    /// `(path, kind)` pairs of the checkout whose note fits the code on disk.
    pub ok: usize,
    /// Pairs whose notes were written only for other versions.
    pub stale: usize,
    /// Pairs whose note for the code on disk is in conflict.
    pub conflicts: usize,
    /// Project and branch notes.
    pub project_notes: i64,
    /// Events in the log.
    pub events: i64,
    /// Sessions kept, newest first up to `SESSIONS_KEPT`.
    pub sessions: usize,
    /// Kept sessions that changed at least one file.
    pub sessions_changing_files: usize,
    /// Kept sessions that saved at least one note.
    pub sessions_saving_notes: usize,
    /// Kept sessions that Stop blocked once to ask for notes.
    pub sessions_blocked: usize,
}

/// Counts what `database` holds. A note's status is computed for the
/// checkout; notes of files that are not checked out count only as stored.
pub fn memory_state(database: &dyn MemoryStore) -> Result<MemoryState> {
    let mut state = MemoryState {
        files: database.files()?.len() as i64,
        edges: database.edge_count()?,
        file_notes: database.file_note_count()?,
        project_notes: database.project_note_count()?,
        events: database.event_count()?,
        ..MemoryState::default()
    };
    let sessions = database.recent_sessions(SESSIONS_KEPT)?;
    state.sessions = sessions.len();
    state.sessions_changing_files = sessions
        .iter()
        .filter(|session| session.observation.changed_count > 0)
        .count();
    state.sessions_saving_notes = sessions
        .iter()
        .filter(|session| session.notes_saved > 0)
        .count();
    state.sessions_blocked = sessions.iter().filter(|session| session.blocked).count();
    for (path, kind) in database.noted_paths()? {
        let Some(current_hash) = database.file_hash(&path)? else {
            continue;
        };
        let notes = database.file_notes(&path)?;
        let status = applicable(&notes, kind, Some(&current_hash)).and_then(|(_, status)| status);
        match status {
            Some(NoteStatus::Ok) => state.ok += 1,
            Some(NoteStatus::Stale) => state.stale += 1,
            Some(NoteStatus::Conflict) => state.conflicts += 1,
            None => {}
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::{AcceptedNote, Event, NoteRef, NoteText};
    use crate::application::memory::domain::{
        Author, NoteBody, NoteKind, NoteSummary, NoteTarget, Observation, RepoPath, SessionId,
        StoredSession,
    };
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-09T12:00:00Z";

    fn indexed(path: &str, hash: &str) -> Event {
        Event::FileIndexed {
            path: RepoPath::parse(path).unwrap(),
            blob_hash: hash.to_string(),
            edges: Vec::new(),
            symbols: Vec::new(),
        }
    }

    fn note(path: &str, hash: &str, kind: NoteKind) -> Event {
        Event::NoteAccepted {
            note: AcceptedNote {
                target: NoteTarget::File {
                    path: RepoPath::parse(path).unwrap(),
                    code_hash: hash.to_string(),
                },
                kind,
                summary: NoteSummary::parse("A note about this file").unwrap(),
                body: NoteBody::parse("").unwrap(),
                refs: Vec::new(),
                author: Author::Writer,
            },
        }
    }

    #[test]
    fn empty_memory_counts_nothing() {
        let database = Database::open_in_memory().unwrap();

        assert_eq!(memory_state(&database).unwrap(), MemoryState::default());
    }

    #[test]
    fn counts_ok_stale_and_conflicting_notes_of_the_checkout() {
        let mut database = Database::open_in_memory().unwrap();
        for event in [
            indexed("src/a.rs", "a2"),
            indexed("src/b.rs", "b1"),
            note("src/a.rs", "a1", NoteKind::Purpose),
            note("src/a.rs", "a2", NoteKind::Gotcha),
            note("src/b.rs", "b0", NoteKind::Purpose),
            note("src/gone.rs", "g1", NoteKind::Purpose),
        ] {
            database.apply(&event, AT).unwrap();
        }
        database
            .apply(
                &Event::NoteConflicted {
                    note: NoteRef::File(2),
                    proposal_id: 1,
                    other: NoteText {
                        summary: NoteSummary::parse("Another text for it").unwrap(),
                        body: NoteBody::parse("").unwrap(),
                    },
                },
                AT,
            )
            .unwrap();

        let state = memory_state(&database).unwrap();

        assert_eq!(state.files, 2);
        assert_eq!(state.file_notes, 4);
        assert_eq!((state.ok, state.stale, state.conflicts), (0, 2, 1));
        assert_eq!(state.events, 7);
    }

    fn session(id: &str, changed_count: usize, notes_saved: i64, blocked: bool) -> StoredSession {
        StoredSession {
            id: SessionId::parse(id).unwrap(),
            harness: "claude".to_string(),
            started_at: AT.to_string(),
            ended_at: Some(AT.to_string()),
            files_at_start: None,
            files_at_last_note: None,
            blocked,
            notes_saved,
            observation: Observation {
                changed: Vec::new(),
                changed_count,
                stale: Vec::new(),
            },
        }
    }

    #[test]
    fn counts_what_the_kept_sessions_did() {
        // Given a session that only read, one blocked that then saved a
        // note, and one that saved a note without a block
        let mut database = Database::open_in_memory().unwrap();
        for kept in [
            session("1", 0, 0, false),
            session("2", 3, 1, true),
            session("3", 1, 2, false),
        ] {
            database.save_session(&kept).unwrap();
        }

        // When memory's state is counted
        let state = memory_state(&database).unwrap();

        // Then each count says how many sessions did it
        assert_eq!(
            (
                state.sessions,
                state.sessions_changing_files,
                state.sessions_saving_notes,
                state.sessions_blocked
            ),
            (3, 2, 2, 1)
        );
    }
}
