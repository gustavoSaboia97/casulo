//! `memory.sqlite`: the event log, the catalog and the search tables.
//!
//! Every change is an event, logged and applied to the tables in one
//! transaction, so the tables never lag the log. This file opens the
//! database and migrates it; `store` reads and writes it as the memory's
//! `MemoryStore`, and `projection` says how each event changes the tables.

mod projection;
mod store;

use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction, TransactionBehavior};

/// The schema, one migration per entry. Migration `n` (counting from 1)
/// runs when `PRAGMA user_version` is below `n`. Never edit a released
/// migration; add a new one.
const MIGRATIONS: [&str; 4] = [
    r#"
CREATE TABLE events (id INTEGER PRIMARY KEY, at TEXT NOT NULL,
                     kind TEXT NOT NULL, payload TEXT NOT NULL);

CREATE TABLE files (path TEXT PRIMARY KEY, blob_hash TEXT NOT NULL);
CREATE TABLE branches (name TEXT PRIMARY KEY, head_commit TEXT NOT NULL);
CREATE TABLE branch_blobs (branch TEXT NOT NULL, path TEXT NOT NULL, blob_hash TEXT NOT NULL,
                           PRIMARY KEY (branch, path));
CREATE TABLE parsed (blob_hash TEXT PRIMARY KEY, edges TEXT NOT NULL, symbols TEXT NOT NULL);
CREATE TABLE notes (
  id INTEGER PRIMARY KEY, path TEXT NOT NULL, kind TEXT NOT NULL,
  summary TEXT NOT NULL, body TEXT NOT NULL, refs TEXT NOT NULL,
  code_hash TEXT NOT NULL, conflict TEXT, reinforced INTEGER NOT NULL,
  author TEXT NOT NULL, created_at TEXT NOT NULL,
  unreferenced_since TEXT,
  UNIQUE (path, kind, code_hash)
);
CREATE TABLE project_notes (
  id INTEGER PRIMARY KEY,
  branch TEXT NOT NULL,
  topic TEXT NOT NULL, kind TEXT NOT NULL,
  summary TEXT NOT NULL, body TEXT NOT NULL, refs TEXT NOT NULL,
  conflict TEXT, reinforced INTEGER NOT NULL,
  author TEXT NOT NULL, created_at TEXT NOT NULL,
  branch_gone_since TEXT,
  UNIQUE (branch, topic, kind)
);
CREATE TABLE proposals (id INTEGER PRIMARY KEY, event_id INTEGER NOT NULL, state TEXT NOT NULL);

CREATE TABLE edges (from_path TEXT NOT NULL, to_path TEXT NOT NULL, kind TEXT NOT NULL,
                    PRIMARY KEY (from_path, to_path, kind));
CREATE VIRTUAL TABLE code_fts USING fts5(path, symbols, tokenize='unicode61');
CREATE VIRTUAL TABLE notes_fts USING fts5(summary, body, tokenize='unicode61');
CREATE VIRTUAL TABLE project_notes_fts USING fts5(topic, summary, body, tokenize='unicode61');
CREATE TABLE note_vectors (
  note_type TEXT NOT NULL, note_id INTEGER NOT NULL,
  path TEXT, kind TEXT NOT NULL, code_hash TEXT, branch TEXT,
  embedder TEXT NOT NULL, dim INTEGER NOT NULL, vector BLOB NOT NULL,
  PRIMARY KEY (note_type, note_id, embedder)
);

CREATE TABLE timings (at TEXT NOT NULL, tool TEXT NOT NULL, stage TEXT NOT NULL,
                      micros INTEGER NOT NULL);
CREATE TABLE tool_calls (at TEXT NOT NULL, harness TEXT NOT NULL, tool TEXT NOT NULL);
"#,
    // The parse cache keeps what the parser found (modules, uses, symbols),
    // not resolved edges: edges depend on which other files exist, and are
    // recomputed from the cache whenever the file set changes.
    r#"
DROP TABLE parsed;
CREATE TABLE parsed (blob_hash TEXT PRIMARY KEY, code TEXT NOT NULL);
"#,
    // Sessions are what the harness's hooks report, kept apart from the
    // event log so nothing in them can become a note. Snapshots and the
    // observation are JSON.
    r#"
CREATE TABLE sessions (id TEXT PRIMARY KEY, harness TEXT NOT NULL,
                       started_at TEXT NOT NULL, ended_at TEXT,
                       files_at_start TEXT, files_at_last_note TEXT,
                       blocked INTEGER NOT NULL, notes_saved INTEGER NOT NULL,
                       observation TEXT NOT NULL);
CREATE INDEX sessions_by_start ON sessions (started_at);
ALTER TABLE tool_calls ADD COLUMN session TEXT NOT NULL DEFAULT '';
"#,
    // Porter stemming lets one form of a word find the others ("mounting"
    // finds `mask_mounts`). FTS5 cannot change a table's tokenizer, so each
    // table is built again; its rows, rowids included, are copied in SQL,
    // so nothing needs to be indexed again. Renaming also renames the
    // shadow tables FTS5 keeps.
    r#"
CREATE VIRTUAL TABLE code_fts_new USING fts5(path, symbols, tokenize='porter unicode61');
INSERT INTO code_fts_new (rowid, path, symbols) SELECT rowid, path, symbols FROM code_fts;
DROP TABLE code_fts;
ALTER TABLE code_fts_new RENAME TO code_fts;

CREATE VIRTUAL TABLE notes_fts_new USING fts5(summary, body, tokenize='porter unicode61');
INSERT INTO notes_fts_new (rowid, summary, body) SELECT rowid, summary, body FROM notes_fts;
DROP TABLE notes_fts;
ALTER TABLE notes_fts_new RENAME TO notes_fts;

CREATE VIRTUAL TABLE project_notes_fts_new USING fts5(topic, summary, body, tokenize='porter unicode61');
INSERT INTO project_notes_fts_new (rowid, topic, summary, body)
  SELECT rowid, topic, summary, body FROM project_notes_fts;
DROP TABLE project_notes_fts;
ALTER TABLE project_notes_fts_new RENAME TO project_notes_fts;

-- The primary key starts with from_path, so finding the files that link
-- to one scanned the whole table; search now does it for every hit.
CREATE INDEX edges_by_target ON edges (to_path);
"#,
];

/// The branch column of a general project note.
const GENERAL_BRANCH: &str = "";

/// The memory database of one project.
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Opens the database at `path`, creating it and its tables if needed.
    pub fn open(path: &Path) -> Result<Self> {
        let connection =
            Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        Self::prepare(connection).with_context(|| format!("cannot prepare {}", path.display()))
    }

    /// A database that lives only in memory, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(connection: Connection) -> Result<Self> {
        // The memory server reads with one connection while the background
        // indexer writes with another; a writer waits instead of failing.
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        // WAL needs shared memory, which the host and a Docker VM do not share.
        let mode: String =
            connection.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("delete") && !mode.eq_ignore_ascii_case("memory") {
            bail!("cannot set the journal mode to DELETE (got {mode})");
        }
        let mut database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: usize = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?
            as usize;
        if version > MIGRATIONS.len() {
            bail!("the memory database is from a newer casulo (schema {version}); update casulo");
        }
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
            let transaction = self.connection.transaction()?;
            transaction.execute_batch(migration)?;
            // `user_version` cannot be a bound parameter; the value is ours.
            transaction.execute_batch(&format!("PRAGMA user_version = {}", index + 1))?;
            transaction.commit()?;
        }
        Ok(())
    }

    /// A transaction that takes the write lock at once: one that started
    /// as a read could not upgrade while another connection writes.
    fn write_transaction(&mut self) -> Result<Transaction<'_>> {
        Ok(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::NoteProposal;
    use crate::application::memory::domain::event::BranchFiles;
    use crate::application::memory::domain::event::{AcceptedNote, BranchHead, Edge, NoteRef};
    use crate::application::memory::domain::event::{Event, NoteText};
    use crate::application::memory::domain::{Author, NoteKind, RepoPath, Topic};
    use crate::application::memory::domain::{EdgeKind, NoteBody, NoteSummary, NoteTarget};
    use crate::application::memory::domain::{Observation, SessionId, StoredSession};
    use crate::application::memory::ports::MemoryStore;

    const AT: &str = "2026-10-09T12:00:00Z";

    fn path(value: &str) -> RepoPath {
        RepoPath::parse(value).unwrap()
    }

    fn text(summary: &str) -> NoteText {
        NoteText {
            summary: NoteSummary::parse(summary).unwrap(),
            body: NoteBody::parse("Details.").unwrap(),
        }
    }

    #[test]
    fn two_connections_never_get_the_same_proposal_id() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        Database::open(&file).unwrap();
        let proposal = NoteProposal {
            target: NoteTarget::File {
                path: path("src/main.rs"),
                code_hash: "a1".to_string(),
            },
            kind: NoteKind::Convention,
            summary: NoteSummary::parse("Containers run read-only").unwrap(),
            body: NoteBody::parse("").unwrap(),
            refs: Vec::new(),
            author: Author::Harness("claude".to_string()),
        };

        // Like the server and the Writer, each on its own connection.
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let file = file.clone();
                let proposal = proposal.clone();
                std::thread::spawn(move || {
                    let mut database = Database::open(&file).unwrap();
                    (0..20)
                        .map(|_| database.record_proposal(&proposal, AT).unwrap())
                        .collect::<Vec<i64>>()
                })
            })
            .collect();
        let mut ids: Vec<i64> = writers
            .into_iter()
            .flat_map(|writer| writer.join().unwrap())
            .collect();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), 40);
        let database = Database::open(&file).unwrap();
        let pending: i64 = database
            .connection
            .query_row("SELECT count(*) FROM proposals", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pending, 40);
    }

    #[test]
    fn applying_an_outcome_marks_its_proposal_done() {
        let mut database = Database::open_in_memory().unwrap();
        let proposal = NoteProposal {
            target: NoteTarget::File {
                path: path("src/main.rs"),
                code_hash: "a1".to_string(),
            },
            kind: NoteKind::Convention,
            summary: NoteSummary::parse("Containers run read-only").unwrap(),
            body: NoteBody::parse("").unwrap(),
            refs: Vec::new(),
            author: Author::Harness("claude".to_string()),
        };
        let id = database.record_proposal(&proposal, AT).unwrap();
        assert!(!database.proposal_done(id).unwrap());

        database
            .apply_for_proposal(&file_note("a1", "Containers run read-only"), id, AT)
            .unwrap();
        assert!(database.proposal_done(id).unwrap());
    }

    fn file_note(code_hash: &str, summary: &str) -> Event {
        Event::NoteAccepted {
            note: AcceptedNote {
                target: NoteTarget::File {
                    path: path("src/main.rs"),
                    code_hash: code_hash.to_string(),
                },
                kind: NoteKind::Convention,
                summary: NoteSummary::parse(summary).unwrap(),
                body: NoteBody::parse("Details.").unwrap(),
                refs: vec![path("src/cli.rs")],
                author: Author::Harness("claude".to_string()),
            },
        }
    }

    fn indexed(file: &str, hash: &str, to: &[&str]) -> Event {
        Event::FileIndexed {
            path: path(file),
            blob_hash: hash.to_string(),
            edges: to
                .iter()
                .map(|other| Edge {
                    to: path(other),
                    kind: EdgeKind::Imports,
                })
                .collect(),
            symbols: vec!["main".to_string(), "run_command".to_string()],
        }
    }

    /// Every table except the log, as text, to compare states. The rowid
    /// of `code_fts` is left out: indexing a file again gives it a new one.
    fn tables(database: &Database) -> String {
        dump(
            &database.connection,
            &[
                "SELECT path, blob_hash FROM files ORDER BY 1",
                "SELECT name, head_commit FROM branches ORDER BY 1",
                "SELECT branch, path, blob_hash FROM branch_blobs ORDER BY 1, 2",
                "SELECT blob_hash, code FROM parsed ORDER BY 1",
                "SELECT id, path, kind, summary, body, refs, code_hash, conflict, reinforced, author FROM notes ORDER BY 1",
                "SELECT id, branch, topic, kind, summary, body, conflict, reinforced, author FROM project_notes ORDER BY 1",
                "SELECT id, state FROM proposals ORDER BY 1",
                "SELECT from_path, to_path, kind FROM edges ORDER BY 1, 2, 3",
                "SELECT path, symbols FROM code_fts ORDER BY 1",
                "SELECT rowid, summary, body FROM notes_fts ORDER BY 1",
                "SELECT rowid, topic, summary FROM project_notes_fts ORDER BY 1",
            ],
        )
    }

    /// The rows each query returns, as text, one block per query.
    fn dump(connection: &Connection, queries: &[&str]) -> String {
        let mut dump = String::new();
        for query in queries {
            let mut statement = connection.prepare(query).unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    let mut values = Vec::new();
                    for index in 0..columns {
                        let value: rusqlite::types::Value = row.get(index)?;
                        values.push(format!("{value:?}"));
                    }
                    Ok(values.join("|"))
                })
                .unwrap();
            for row in rows {
                dump.push_str(&row.unwrap());
                dump.push('\n');
            }
            dump.push_str("--\n");
        }
        dump
    }

    fn every_event() -> Vec<Event> {
        vec![
            indexed("src/main.rs", "a1", &["src/cli.rs"]),
            indexed("src/cli.rs", "b1", &[]),
            Event::BranchesScanned {
                branches: vec![BranchHead {
                    branch: "main".to_string(),
                    head_commit: "c1".to_string(),
                }],
                moved: vec![BranchFiles {
                    branch: "main".to_string(),
                    files: vec![(path("src/main.rs"), "a1".to_string())],
                }],
            },
            file_note("a1", "Every container runs read-only"),
            Event::NoteReinforced {
                note: NoteRef::File(1),
                reinforced: 3,
            },
            Event::NoteConflicted {
                note: NoteRef::File(1),
                proposal_id: 7,
                other: text("Containers may write to a cache"),
            },
            Event::NoteResolved {
                note: NoteRef::File(1),
                winner: text("Containers run read-only, always"),
                author: Some(Author::Harness("codex".to_string())),
            },
            Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::Branch {
                        branch: "podman".to_string(),
                        topic: Topic::parse("runtime").unwrap(),
                    },
                    kind: NoteKind::Decision,
                    summary: NoteSummary::parse("This branch moves to Podman").unwrap(),
                    body: NoteBody::parse("").unwrap(),
                    refs: Vec::new(),
                    author: Author::Harness("codex".to_string()),
                },
            },
            Event::FileRemoved {
                path: path("src/cli.rs"),
            },
        ]
    }

    #[test]
    fn applies_and_reads_back() {
        let mut database = Database::open_in_memory().unwrap();
        for event in every_event() {
            database.apply(&event, AT).unwrap();
        }

        assert_eq!(
            database.files().unwrap(),
            [(path("src/main.rs"), "a1".to_string())]
        );
        assert!(
            database
                .edges_from(&path("src/main.rs"))
                .unwrap()
                .is_empty()
        );
        let notes = database.file_notes(&path("src/main.rs")).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, text("Containers run read-only, always"));
        assert_eq!(notes[0].conflict, None);
        assert_eq!(notes[0].reinforced, 3);
        assert_eq!(notes[0].author, Author::Harness("codex".to_string()));
        assert_eq!(database.project_notes(None).unwrap().len(), 0);
        let on_branch = database.project_notes(Some("podman")).unwrap();
        assert_eq!(on_branch[0].branch.as_deref(), Some("podman"));
        assert_eq!(database.branches().unwrap()[0].head_commit, "c1");
        assert_eq!(database.event_count().unwrap(), 9);
    }

    #[test]
    fn applying_an_event_twice_changes_nothing() {
        let mut once = Database::open_in_memory().unwrap();
        let mut twice = Database::open_in_memory().unwrap();
        for event in every_event() {
            once.apply(&event, AT).unwrap();
            twice.apply(&event, AT).unwrap();
            twice.apply(&event, AT).unwrap();
        }

        assert_eq!(tables(&once), tables(&twice));
    }

    #[test]
    fn a_file_note_is_unique_per_path_kind_and_content() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .apply(&file_note("a1", "First text of the note"), AT)
            .unwrap();
        database
            .apply(&file_note("a1", "Second text of the note"), AT)
            .unwrap();
        database
            .apply(&file_note("a2", "Text for another version"), AT)
            .unwrap();

        let notes = database.file_notes(&path("src/main.rs")).unwrap();

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].text.summary.as_str(), "Second text of the note");
    }

    #[test]
    fn a_project_note_is_unique_per_branch_topic_and_kind() {
        let mut database = Database::open_in_memory().unwrap();
        let general = |summary: &str| Event::NoteAccepted {
            note: AcceptedNote {
                target: NoteTarget::Project {
                    topic: Topic::parse("errors").unwrap(),
                },
                kind: NoteKind::Convention,
                summary: NoteSummary::parse(summary).unwrap(),
                body: NoteBody::parse("").unwrap(),
                refs: Vec::new(),
                author: Author::User,
            },
        };
        database
            .apply(&general("We use anyhow everywhere"), AT)
            .unwrap();
        database
            .apply(&general("We use anyhow with context"), AT)
            .unwrap();

        let notes = database.project_notes(None).unwrap();

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text.summary.as_str(), "We use anyhow with context");
        assert_eq!(notes[0].branch, None);
    }

    #[test]
    fn reopening_keeps_the_data_and_runs_no_migration_twice() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        let mut database = Database::open(&file).unwrap();
        database
            .apply(&indexed("src/main.rs", "a1", &[]), AT)
            .unwrap();
        let before = (tables(&database), search_rows(&database.connection));
        drop(database);

        let database = Database::open(&file).unwrap();

        assert_eq!(database.files().unwrap().len(), 1);
        assert_eq!(user_version(&database), MIGRATIONS.len() as i64);
        assert_eq!(
            (tables(&database), search_rows(&database.connection)),
            before
        );
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        Connection::open(&file)
            .unwrap()
            .execute_batch("PRAGMA user_version = 99")
            .unwrap();

        let error = Database::open(&file).err().unwrap();

        assert!(format!("{error:#}").contains("newer casulo"));
    }

    fn session(id: &str, started_at: &str) -> StoredSession {
        StoredSession {
            id: SessionId::parse(id).unwrap(),
            harness: "claude".to_string(),
            started_at: started_at.to_string(),
            ended_at: None,
            files_at_start: Some(vec![(path("src/main.rs"), "a1".to_string())]),
            files_at_last_note: Some(Vec::new()),
            blocked: false,
            notes_saved: 0,
            observation: Observation::default(),
        }
    }

    /// A database at `file` built by the first `version` migrations only,
    /// as an older casulo left it.
    fn at_version(file: &Path, version: usize) -> Connection {
        let connection = Connection::open(file).unwrap();
        for (index, migration) in MIGRATIONS.iter().take(version).enumerate() {
            connection.execute_batch(migration).unwrap();
            connection
                .execute_batch(&format!("PRAGMA user_version = {}", index + 1))
                .unwrap();
        }
        connection
    }

    #[test]
    fn a_database_from_a_newer_casulo_says_to_update_casulo() {
        // Given a database whose schema is newer than this casulo knows
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        Connection::open(&file)
            .unwrap()
            .execute_batch(&format!("PRAGMA user_version = {}", MIGRATIONS.len() + 1))
            .unwrap();

        // When it is opened
        let error = format!("{:#}", Database::open(&file).err().unwrap());

        // Then the error says to update casulo, and names no command that does not exist
        assert!(error.contains("update casulo"), "{error}");
        assert!(!error.contains("index memory"), "{error}");
    }

    #[test]
    fn a_database_at_version_2_migrates_and_keeps_its_notes() {
        // Given a database built by the first two migrations, with a note
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        let mut connection = at_version(&file, 2);
        let transaction = connection.transaction().unwrap();
        projection::log_and_apply(
            &transaction,
            &file_note("a1", "Containers run read-only"),
            AT,
        )
        .unwrap();
        transaction.commit().unwrap();
        drop(connection);

        // When it is opened by this casulo
        let mut database = Database::open(&file).unwrap();

        // Then the note is kept and the new tables work
        assert_eq!(database.file_notes(&path("src/main.rs")).unwrap().len(), 1);
        database
            .record_tool_call(
                AT,
                "claude",
                Some(&SessionId::parse("7").unwrap()),
                "recall",
            )
            .unwrap();
        database.save_session(&session("7", AT)).unwrap();
    }

    /// A file with the symbol `mask_mounts`, a note of it and a project
    /// note, each saying "mount" in some form.
    fn mount_events() -> Vec<Event> {
        vec![
            Event::FileIndexed {
                path: path("src/mask.rs"),
                blob_hash: "m1".to_string(),
                edges: Vec::new(),
                symbols: vec!["mask_mounts".to_string()],
            },
            Event::FileIndexed {
                path: path("src/run_spec.rs"),
                blob_hash: "r1".to_string(),
                edges: Vec::new(),
                symbols: vec!["Mount".to_string()],
            },
            Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::File {
                        path: path("src/mask.rs"),
                        code_hash: "m1".to_string(),
                    },
                    kind: NoteKind::Gotcha,
                    summary: NoteSummary::parse("Each secret gets an empty mount").unwrap(),
                    body: NoteBody::parse("").unwrap(),
                    refs: Vec::new(),
                    author: Author::Harness("claude".to_string()),
                },
            },
            Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::Project {
                        topic: Topic::parse("mounts").unwrap(),
                    },
                    kind: NoteKind::Convention,
                    summary: NoteSummary::parse("Every mount is read-only").unwrap(),
                    body: NoteBody::parse("").unwrap(),
                    refs: Vec::new(),
                    author: Author::User,
                },
            },
        ]
    }

    /// A database at version 3 holding `mount_events`.
    fn at_version_3_with_mounts(file: &Path) -> Connection {
        let mut connection = at_version(file, 3);
        let transaction = connection.transaction().unwrap();
        for event in mount_events() {
            projection::log_and_apply(&transaction, &event, AT).unwrap();
        }
        transaction.commit().unwrap();
        connection
    }

    /// Every row of the three search tables, with its rowid.
    fn search_rows(connection: &Connection) -> String {
        dump(
            connection,
            &[
                "SELECT rowid, path, symbols FROM code_fts ORDER BY 1",
                "SELECT rowid, summary, body FROM notes_fts ORDER BY 1",
                "SELECT rowid, topic, summary, body FROM project_notes_fts ORDER BY 1",
            ],
        )
    }

    /// What "mounting" finds in each search table; the files in path
    /// order, since only what is found matters here, not its rank.
    fn found_by_mounting(database: &Database) -> (Vec<RepoPath>, Vec<i64>, Vec<i64>) {
        let query = "\"mounting\"*";
        let mut code = database.search_code(query, 20).unwrap();
        code.sort();
        let file_notes = database
            .search_file_notes(query, 20)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        (
            code,
            file_notes,
            database.search_project_notes(query, None, 20).unwrap(),
        )
    }

    /// How SQLite runs the query of `edges_to`.
    fn edges_to_plan(database: &Database) -> String {
        let mut statement = database
            .connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT from_path, kind FROM edges WHERE to_path = ?1 ORDER BY from_path, kind",
            )
            .unwrap();
        let details = statement
            .query_map(["src/main.rs"], |row| row.get::<_, String>(3))
            .unwrap();
        details
            .map(|detail| detail.unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_files_that_link_to_one_are_found_through_an_index() {
        // Given a new database
        let database = Database::open_in_memory().unwrap();

        // When SQLite plans the query of `edges_to`
        let plan = edges_to_plan(&database);

        // Then it searches the index by target, not the whole table
        assert!(plan.contains("USING INDEX edges_by_target"), "{plan}");
    }

    fn user_version(database: &Database) -> i64 {
        database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn a_database_at_version_3_gets_stemmed_search_and_keeps_its_rows() {
        // Given a database at version 3 with a file, a note and a project
        // note about mounts, and its tables as they were
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        let old = Database {
            connection: at_version_3_with_mounts(&file),
        };
        let (tables_before, search_before) = (tables(&old), search_rows(&old.connection));
        drop(old);

        // When it is opened by this casulo
        let database = Database::open(&file).unwrap();

        // Then it is at version 4, every row and rowid is kept, and another
        // form of "mount" finds all three
        assert_eq!(user_version(&database), 4);
        assert!(edges_to_plan(&database).contains("edges_by_target"));
        assert_eq!(tables(&database), tables_before);
        assert_eq!(search_rows(&database.connection), search_before);
        assert_eq!(
            found_by_mounting(&database),
            (
                vec![path("src/mask.rs"), path("src/run_spec.rs")],
                vec![1],
                vec![1]
            )
        );
    }

    #[test]
    fn a_migrated_database_keeps_its_search_tables_in_step() {
        // Given a database migrated from version 3, with mounts
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("memory.sqlite");
        drop(at_version_3_with_mounts(&file));
        let mut database = Database::open(&file).unwrap();

        // When the note is edited, the project note deleted, one file
        // indexed again with new symbols and the other removed
        for event in [
            Event::NoteResolved {
                note: NoteRef::File(1),
                winner: text("Each secret gets an empty volume"),
                author: None,
            },
            Event::NoteDeleted {
                note: NoteRef::Project(1),
            },
            Event::FileIndexed {
                path: path("src/mask.rs"),
                blob_hash: "m2".to_string(),
                edges: Vec::new(),
                symbols: vec!["mask_volumes".to_string()],
            },
            Event::FileRemoved {
                path: path("src/run_spec.rs"),
            },
        ] {
            database.apply(&event, AT).unwrap();
        }

        // Then "mounting" finds nothing left, and the new words are found
        assert_eq!(
            found_by_mounting(&database),
            (Vec::new(), Vec::new(), Vec::new())
        );
        assert_eq!(
            database.search_code("\"volume\"*", 20).unwrap(),
            [path("src/mask.rs")]
        );
        assert_eq!(
            database.search_file_notes("\"volumes\"*", 20).unwrap(),
            [(1, path("src/mask.rs"))]
        );
    }

    #[test]
    fn a_saved_session_is_read_back() {
        // Given a session that blocked once and saw a change
        let mut database = Database::open_in_memory().unwrap();
        let mut saved = session("4242", AT);
        saved.blocked = true;
        saved.notes_saved = 2;
        saved.observation = Observation {
            changed: vec![path("src/main.rs")],
            changed_count: 1,
            stale: vec![(path("src/main.rs"), NoteKind::Purpose)],
        };
        database.save_session(&session("4242", AT)).unwrap();

        // When it is saved again with new values
        database.save_session(&saved).unwrap();

        // Then the newest values are read back
        assert_eq!(
            database
                .session(&SessionId::parse("4242").unwrap())
                .unwrap(),
            Some(saved)
        );
    }

    #[test]
    fn pruning_keeps_only_the_newest_sessions_and_snapshots() {
        // Given three sessions started one after the other, with snapshots
        let mut database = Database::open_in_memory().unwrap();
        for (id, at) in [
            ("1", "2026-10-01T00:00:00Z"),
            ("2", "2026-10-02T00:00:00Z"),
            ("3", "2026-10-03T00:00:00Z"),
        ] {
            database.save_session(&session(id, at)).unwrap();
        }

        // When all but two are pruned, and all but one keep snapshots
        database.prune_sessions(2, 1).unwrap();

        // Then the two newest are left, newest first, and only the newest
        // has its snapshots
        let ids: Vec<String> = database
            .recent_sessions(10)
            .unwrap()
            .into_iter()
            .map(|kept| kept.id.to_string())
            .collect();
        assert_eq!(ids, ["3", "2"]);
        let snapshots = |id: &str| {
            let kept = database
                .session(&SessionId::parse(id).unwrap())
                .unwrap()
                .unwrap();
            (
                kept.files_at_start.is_some(),
                kept.files_at_last_note.is_some(),
            )
        };
        assert_eq!(
            (snapshots("3"), snapshots("2")),
            ((true, true), (false, false))
        );
    }

    #[test]
    fn recent_sessions_leave_out_the_snapshots() {
        // Given a session with snapshots
        let mut database = Database::open_in_memory().unwrap();
        database.save_session(&session("1", AT)).unwrap();

        // When the recent sessions are read
        let recent = database.recent_sessions(10).unwrap();

        // Then their snapshots are not loaded
        assert_eq!(
            (
                recent[0].files_at_start.clone(),
                recent[0].files_at_last_note.clone()
            ),
            (None, None)
        );
    }

    #[test]
    fn only_the_newest_tool_calls_are_kept() {
        // Given more tool calls than are kept, the first one of codex
        let mut database = Database::open_in_memory().unwrap();
        database
            .record_tool_call(AT, "codex", None, "recall")
            .unwrap();
        for _ in 0..TOOL_CALLS_KEPT_IN_TEST {
            database
                .record_tool_call(AT, "claude", None, "recall")
                .unwrap();
        }

        // When they are counted
        let (count, codex): (i64, i64) = database
            .connection
            .query_row(
                "SELECT count(*), count(*) FILTER (WHERE harness = 'codex') FROM tool_calls",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        // Then the oldest is gone
        assert_eq!((count, codex), (TOOL_CALLS_KEPT_IN_TEST as i64, 0));
    }

    /// How many tool calls the store keeps, as its port promises.
    const TOOL_CALLS_KEPT_IN_TEST: usize = 10_000;

    #[test]
    fn events_round_trip_through_json() {
        for event in every_event() {
            let json = serde_json::to_string(&event).unwrap();
            let back: Event = serde_json::from_str(&json).unwrap();
            assert_eq!(back, event);
        }
    }
}
