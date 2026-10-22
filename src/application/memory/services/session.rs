//! Sessions as the harness's hooks report them: a snapshot of the files at
//! the start and at the last note, so Stop can remind the agent once to
//! save what it learned. Nothing here writes a note or an event.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;

use crate::application::memory::domain::session::MAX_CHANGED_KEPT;
use crate::application::memory::domain::{
    FileSnapshot, NoteKind, NoteStatus, Observation, RepoPath, SessionId, StopDecision,
    StoredSession,
};
use crate::application::memory::ports::{Git, MemoryStore};
use crate::application::memory::services::applicable::applicable;

/// Sessions kept; older ones are deleted whenever a session is created.
pub const SESSIONS_KEPT: usize = 100;

/// Sessions that keep their file snapshots; older ones drop them whenever
/// a session is created, so at most this many rows hold a list of files.
pub const SNAPSHOTS_KEPT: usize = 10;

/// What Stop answers the first time a session ends a turn with files
/// changed since its last note.
pub const BLOCK_REASON: &str = "You changed files since the last memory note. If you learned something the code does not say (a file's purpose, a convention, a trap, a decision), save one to three notes with remember now; otherwise say there is nothing to save. This reminder comes once per session.";

/// The session the hooks of one run report: its id, the harness that
/// runs it and where the checkout is.
pub struct SessionContext {
    /// The session.
    pub id: SessionId,
    /// The harness, from the URL its hooks call.
    pub harness: String,
    /// The checkout.
    pub workspace: PathBuf,
    /// Now, as an RFC 3339 time.
    pub at: String,
}

/// Why a session starts, as the harness says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    /// The agent was just launched: a new run, even if an older row has
    /// the same id (the host may reuse a process id).
    Startup,
    /// The same run goes on, after a clear, a compaction or a resume.
    Continued,
}

/// Starts the session: records the files on disk as its start and its last
/// note. A `Continued` start keeps a running session as it is, so a clear
/// or a compaction neither resets its block nor its counts.
pub fn start(
    store: &mut dyn MemoryStore,
    git: &dyn Git,
    session: &SessionContext,
    kind: StartKind,
) -> Result<()> {
    if kind == StartKind::Continued && running(store, &session.id)?.is_some() {
        return Ok(());
    }
    let files = git.ls_files(&session.workspace)?;
    save_new(store, &new_session(session, files))
}

/// Records that a note of the session was accepted: the files on disk
/// become the files at its last note.
pub fn note_saved(
    store: &mut dyn MemoryStore,
    git: &dyn Git,
    session: &SessionContext,
) -> Result<()> {
    let files = git.ls_files(&session.workspace)?;
    let (mut stored, is_new) = match running(store, &session.id)? {
        Some(stored) => (stored, false),
        None => (new_session(session, files.clone()), true),
    };
    stored.files_at_last_note = Some(files);
    stored.notes_saved += 1;
    if is_new {
        save_new(store, &stored)
    } else {
        store.save_session(&stored)
    }
}

/// Decides whether the agent may stop: it is blocked once per session, the
/// first time the files differ from those at its last note. Either way the
/// observation is brought up to date. A session that never started starts
/// now, and may stop.
pub fn stop(
    store: &mut dyn MemoryStore,
    git: &dyn Git,
    session: &SessionContext,
) -> Result<StopDecision> {
    let files = git.ls_files(&session.workspace)?;
    let Some(mut stored) = running(store, &session.id)? else {
        save_new(store, &new_session(session, files))?;
        return Ok(StopDecision::Allow);
    };
    // A session whose snapshots were dropped keeps its last observation:
    // without a start, every file would look changed.
    if let Some(at_start) = &stored.files_at_start {
        stored.observation = observe(store, at_start, &files)?;
    }
    let changed_since_note = stored
        .files_at_last_note
        .as_ref()
        .is_some_and(|at_note| by_path(at_note) != by_path(&files));
    let decision = if !stored.blocked && changed_since_note {
        stored.blocked = true;
        StopDecision::Block(BLOCK_REASON.to_string())
    } else {
        StopDecision::Allow
    };
    store.save_session(&stored)?;
    Ok(decision)
}

/// Ends the session: writes its end time and drops its snapshots, so only
/// running sessions keep a list of files. It reads no files, because the
/// harness gives this hook little time; the observation is the one of the
/// last Stop.
pub fn end(store: &mut dyn MemoryStore, session: &SessionContext) -> Result<()> {
    let Some(mut stored) = running(store, &session.id)? else {
        return Ok(());
    };
    stored.ended_at = Some(session.at.clone());
    stored.files_at_start = None;
    stored.files_at_last_note = None;
    store.save_session(&stored)
}

/// Saves a session that was not stored yet, then prunes, so no path that
/// creates a row can grow the table past its cap.
fn save_new(store: &mut dyn MemoryStore, session: &StoredSession) -> Result<()> {
    store.save_session(session)?;
    store.prune_sessions(SESSIONS_KEPT, SNAPSHOTS_KEPT)
}

/// The session `id`, unless it never started or already ended.
fn running(store: &dyn MemoryStore, id: &SessionId) -> Result<Option<StoredSession>> {
    Ok(store
        .session(id)?
        .filter(|stored| stored.ended_at.is_none()))
}

fn new_session(session: &SessionContext, files: FileSnapshot) -> StoredSession {
    StoredSession {
        id: session.id.clone(),
        harness: session.harness.clone(),
        started_at: session.at.clone(),
        ended_at: None,
        files_at_start: Some(files.clone()),
        files_at_last_note: Some(files),
        blocked: false,
        notes_saved: 0,
        observation: Observation::default(),
    }
}

/// `git ls-files` lists in no promised order, so snapshots are compared by path.
fn by_path(files: &FileSnapshot) -> BTreeMap<&RepoPath, &str> {
    files
        .iter()
        .map(|(path, hash)| (path, hash.as_str()))
        .collect()
}

/// What changed between `at_start` and `now`, and which notes of the
/// changed files no longer fit the content on disk. The stored index may
/// lag the disk, so the hashes come from `now`.
fn observe(
    store: &dyn MemoryStore,
    at_start: &FileSnapshot,
    now: &FileSnapshot,
) -> Result<Observation> {
    let before = by_path(at_start);
    let after = by_path(now);
    let mut changed: Vec<RepoPath> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .map(|path| (*path).clone())
        .collect();
    changed.sort();
    changed.dedup();
    let mut stale = Vec::new();
    for path in &changed {
        let Some(hash) = after.get(path) else {
            continue;
        };
        let notes = store.file_notes(path)?;
        for kind in NoteKind::ALL {
            if let Some((_, Some(NoteStatus::Stale))) = applicable(&notes, kind, Some(hash)) {
                stale.push((path.clone(), kind));
            }
        }
    }
    let changed_count = changed.len();
    changed.truncate(MAX_CHANGED_KEPT);
    Ok(Observation {
        changed,
        changed_count,
        stale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::{AcceptedNote, Event};
    use crate::application::memory::domain::{Author, NoteBody, NoteSummary, NoteTarget};
    use crate::application::memory::test_support::ListingGit;
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-17T12:00:00Z";

    fn id() -> SessionId {
        SessionId::parse("4242").unwrap()
    }

    fn context(id: &str) -> SessionContext {
        SessionContext {
            id: SessionId::parse(id).unwrap(),
            harness: "claude".to_string(),
            workspace: PathBuf::from("workspace"),
            at: AT.to_string(),
        }
    }

    fn path(value: &str) -> RepoPath {
        RepoPath::parse(value).unwrap()
    }

    fn stored(database: &Database) -> StoredSession {
        database.session(&id()).unwrap().unwrap()
    }

    fn block() -> StopDecision {
        StopDecision::Block(BLOCK_REASON.to_string())
    }

    #[test]
    fn a_session_that_changes_nothing_is_not_blocked() {
        // Given a session started on one file
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();

        // When it stops without a change
        let decision = stop(&mut database, &git, &context("4242")).unwrap();

        // Then it may stop
        assert_eq!(decision, StopDecision::Allow);
    }

    #[test]
    fn a_change_without_a_note_blocks_once() {
        // Given a session that changed a file and saved no note
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        git.set(&[("src/main.rs", "a2")]);

        // When it stops twice
        let decisions = [
            stop(&mut database, &git, &context("4242")).unwrap(),
            stop(&mut database, &git, &context("4242")).unwrap(),
        ];

        // Then only the first stop is blocked
        assert_eq!(decisions, [block(), StopDecision::Allow]);
    }

    #[test]
    fn a_note_after_the_last_change_is_never_blocked() {
        // Given a session that changed a file, then saved a note
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        git.set(&[("src/main.rs", "a2")]);
        note_saved(&mut database, &git, &context("4242")).unwrap();

        // When it stops
        let decision = stop(&mut database, &git, &context("4242")).unwrap();

        // Then it may stop
        assert_eq!(decision, StopDecision::Allow);
    }

    #[test]
    fn a_change_after_the_last_note_blocks_once() {
        // Given a session that saved a note, then changed a file
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        note_saved(&mut database, &git, &context("4242")).unwrap();
        git.set(&[("src/main.rs", "a1"), ("src/cli.rs", "c1")]);

        // When it stops
        let decision = stop(&mut database, &git, &context("4242")).unwrap();

        // Then it is blocked
        assert_eq!(decision, block());
    }

    #[test]
    fn a_stop_without_a_start_starts_the_session_and_allows() {
        // Given no session
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);

        // When it stops
        let decision = stop(&mut database, &git, &context("4242")).unwrap();

        // Then it may stop, and the session now exists
        assert_eq!(decision, StopDecision::Allow);
        assert!(!stored(&database).blocked);
    }

    #[test]
    fn starting_a_running_session_again_keeps_it() {
        // Given a running session that changed a file and was blocked
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        git.set(&[("src/main.rs", "a2")]);
        stop(&mut database, &git, &context("4242")).unwrap();
        let before = stored(&database);

        // When it starts again, as after a /clear
        start(&mut database, &git, &context("4242"), StartKind::Continued).unwrap();

        // Then its snapshots and its block are kept
        assert_eq!(stored(&database), before);
    }

    #[test]
    fn a_launch_replaces_a_session_that_never_ended() {
        // Given a session with the same id that was blocked and never ended
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        git.set(&[("src/main.rs", "a2")]);
        stop(&mut database, &git, &context("4242")).unwrap();

        // When a new run with that id is launched
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();

        // Then it starts fresh from the files now
        let fresh = stored(&database);
        assert!(!fresh.blocked);
        assert_eq!(
            fresh.files_at_start,
            Some(vec![(path("src/main.rs"), "a2".to_string())])
        );
    }

    #[test]
    fn continuing_an_ended_session_starts_fresh() {
        // Given a session that was blocked and ended
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        git.set(&[("src/main.rs", "a2")]);
        stop(&mut database, &git, &context("4242")).unwrap();
        end(&mut database, &context("4242")).unwrap();

        // When a start that is not a launch comes with that id
        start(&mut database, &git, &context("4242"), StartKind::Continued).unwrap();

        // Then the session is new and running
        let fresh = stored(&database);
        assert_eq!((fresh.blocked, fresh.ended_at), (false, None));
    }

    #[test]
    fn stops_of_new_sessions_never_grow_past_the_caps() {
        // Given a memory without sessions
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1")]);

        // When 200 Stops come, each from a session never seen
        let ids: Vec<String> = (0..200).map(|number| number.to_string()).collect();
        let stops: Result<Vec<StopDecision>> = ids
            .iter()
            .map(|id| stop(&mut database, &git, &context(id)))
            .collect();

        // Then at most 100 rows are kept, and at most 10 hold snapshots
        stops.unwrap();
        let kept: Vec<StoredSession> = ids
            .iter()
            .filter_map(|id| database.session(&SessionId::parse(id).unwrap()).unwrap())
            .collect();
        let with_snapshots = kept
            .iter()
            .filter(|session| {
                session.files_at_start.is_some() || session.files_at_last_note.is_some()
            })
            .count();
        assert_eq!(
            (kept.len(), with_snapshots),
            (SESSIONS_KEPT, SNAPSHOTS_KEPT)
        );
    }

    #[test]
    fn starting_a_session_keeps_only_the_newest() {
        // Given as many sessions as are kept
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[]);
        for number in 0..SESSIONS_KEPT {
            start(
                &mut database,
                &git,
                &context(&number.to_string()),
                StartKind::Startup,
            )
            .unwrap();
        }

        // When one more starts
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();

        // Then the oldest is gone
        assert_eq!(database.recent_sessions(1000).unwrap().len(), SESSIONS_KEPT);
        assert!(
            database
                .session(&SessionId::parse("0").unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ending_keeps_the_last_observation_and_drops_the_snapshots() {
        // Given a note on a file, and a session that changed it, saved one
        // note elsewhere and stopped
        let mut database = Database::open_in_memory().unwrap();
        database
            .apply(
                &Event::NoteAccepted {
                    note: AcceptedNote {
                        target: NoteTarget::File {
                            path: path("src/main.rs"),
                            code_hash: "a1".to_string(),
                        },
                        kind: NoteKind::Purpose,
                        summary: NoteSummary::parse("Parses the arguments").unwrap(),
                        body: NoteBody::parse("").unwrap(),
                        refs: Vec::new(),
                        author: Author::Harness("claude".to_string()),
                    },
                },
                AT,
            )
            .unwrap();
        let git = ListingGit::new(&[("src/main.rs", "a1"), ("src/old.rs", "o1")]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        note_saved(&mut database, &git, &context("4242")).unwrap();
        git.set(&[("src/main.rs", "a2"), ("src/new.rs", "n1")]);
        stop(&mut database, &git, &context("4242")).unwrap();

        // When it ends
        end(&mut database, &context("4242")).unwrap();

        // Then the observation of that stop says what changed and what went stale
        let ended = stored(&database);
        assert_eq!(
            ended.observation,
            Observation {
                changed: vec![path("src/main.rs"), path("src/new.rs"), path("src/old.rs")],
                changed_count: 3,
                stale: vec![(path("src/main.rs"), NoteKind::Purpose)],
            }
        );
        assert_eq!(ended.ended_at.as_deref(), Some(AT));
        assert_eq!(
            (ended.files_at_start, ended.files_at_last_note),
            (None, None)
        );
        assert_eq!(ended.notes_saved, 1);
    }

    #[test]
    fn an_observation_keeps_at_most_fifty_files_and_counts_all() {
        // Given a session that adds sixty files
        let mut database = Database::open_in_memory().unwrap();
        let git = ListingGit::new(&[]);
        start(&mut database, &git, &context("4242"), StartKind::Startup).unwrap();
        let names: Vec<String> = (0..60)
            .map(|number| format!("src/f{number:02}.rs"))
            .collect();
        let files: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "h")).collect();
        git.set(&files);

        // When it stops
        stop(&mut database, &git, &context("4242")).unwrap();

        // Then fifty are kept and sixty are counted
        let observation = stored(&database).observation;
        assert_eq!(
            (observation.changed.len(), observation.changed_count),
            (MAX_CHANGED_KEPT, 60)
        );
    }
}
