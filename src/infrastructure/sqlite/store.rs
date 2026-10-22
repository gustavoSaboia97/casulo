//! `Database` as the memory's store: reads, and the writes that log and
//! apply each change in one transaction.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::projection::log_and_apply;
use super::{Database, GENERAL_BRANCH};
use crate::application::memory::domain::event::{BranchHead, Edge, Event, NoteText};
use crate::application::memory::domain::rust_modules::ParsedCode;
use crate::application::memory::domain::{
    Author, EdgeKind, FileSnapshot, NoteBody, NoteKind, NoteProposal, NoteSummary, RepoPath,
    SessionId, StoredFileNote, StoredProjectNote, StoredSession,
};
use crate::application::memory::ports::MemoryStore;

// The trait in `application/memory/ports.rs` documents each method; the
// comments here only add what SQLite does.
impl MemoryStore for Database {
    fn file_hash(&self, path: &RepoPath) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT blob_hash FROM files WHERE path = ?1",
                [path.as_str()],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn files(&self) -> Result<Vec<(RepoPath, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT path, blob_hash FROM files ORDER BY path")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?;
        rows.map(|row| {
            let (path, hash) = row?;
            Ok((RepoPath::parse(&path)?, hash))
        })
        .collect()
    }

    fn edges_from(&self, path: &RepoPath) -> Result<Vec<Edge>> {
        edges(
            &self.connection,
            "SELECT to_path, kind FROM edges WHERE from_path = ?1 ORDER BY to_path, kind",
            path,
        )
    }

    fn edges_to(&self, path: &RepoPath) -> Result<Vec<Edge>> {
        edges(
            &self.connection,
            "SELECT from_path, kind FROM edges WHERE to_path = ?1 ORDER BY from_path, kind",
            path,
        )
    }

    fn file_notes(&self, path: &RepoPath) -> Result<Vec<StoredFileNote>> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, kind, summary, body, refs, code_hash, conflict, reinforced, author
             FROM notes WHERE path = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([path.as_str()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        rows.map(|row| {
            let (id, path, kind, summary, body, refs, code_hash, conflict, reinforced, author) =
                row?;
            Ok(StoredFileNote {
                id,
                path: RepoPath::parse(&path)?,
                kind: NoteKind::parse(&kind)?,
                text: note_text(&summary, &body)?,
                refs: serde_json::from_str(&refs)?,
                code_hash,
                conflict: conflict
                    .map(|text| serde_json::from_str(&text))
                    .transpose()?,
                reinforced,
                author: Author::from_text(&author)?,
            })
        })
        .collect()
    }

    fn project_notes(&self, branch: Option<&str>) -> Result<Vec<StoredProjectNote>> {
        let mut statement = self.connection.prepare(
            "SELECT id, branch, topic, kind, summary, body, conflict, author, reinforced FROM project_notes
             WHERE branch = '' OR branch = ?1 ORDER BY topic, branch, kind",
        )?;
        let rows = statement.query_map([branch.unwrap_or(GENERAL_BRANCH)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        rows.map(|row| {
            let (id, branch, topic, kind, summary, body, conflict, author, reinforced) = row?;
            Ok(StoredProjectNote {
                id,
                branch: (!branch.is_empty()).then_some(branch),
                topic,
                kind: NoteKind::parse(&kind)?,
                text: note_text(&summary, &body)?,
                conflict: conflict
                    .map(|text| serde_json::from_str(&text))
                    .transpose()?,
                reinforced,
                author: Author::from_text(&author)?,
            })
        })
        .collect()
    }

    fn find_file_note(
        &self,
        path: &RepoPath,
        kind: NoteKind,
        code_hash: &str,
    ) -> Result<Option<StoredFileNote>> {
        Ok(self
            .file_notes(path)?
            .into_iter()
            .find(|note| note.kind == kind && note.code_hash == code_hash))
    }

    fn find_project_note(
        &self,
        branch: Option<&str>,
        topic: &str,
        kind: NoteKind,
    ) -> Result<Option<StoredProjectNote>> {
        Ok(self.project_notes(branch)?.into_iter().find(|note| {
            note.branch.as_deref() == branch && note.topic == topic && note.kind == kind
        }))
    }

    fn proposal_done(&self, id: i64) -> Result<bool> {
        let state: Option<String> = self
            .connection
            .query_row("SELECT state FROM proposals WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(state.as_deref() == Some("done"))
    }

    fn branches(&self) -> Result<Vec<BranchHead>> {
        let mut statement = self
            .connection
            .prepare("SELECT name, head_commit FROM branches ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(BranchHead {
                branch: row.get(0)?,
                head_commit: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn edge_count(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM edges", [], |row| row.get(0))?)
    }

    fn file_note_count(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))?)
    }

    fn noted_paths(&self) -> Result<Vec<(RepoPath, NoteKind)>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT path, kind FROM notes ORDER BY path, kind")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (path, kind) = row?;
            Ok((RepoPath::parse(&path)?, NoteKind::parse(&kind)?))
        })
        .collect()
    }

    fn project_note_count(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM project_notes", [], |row| row.get(0))?)
    }

    fn search_code(&self, query: &str, limit: usize) -> Result<Vec<RepoPath>> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM code_fts WHERE code_fts MATCH ?1 ORDER BY rank LIMIT ?2")?;
        let rows =
            statement.query_map(params![query, limit as i64], |row| row.get::<_, String>(0))?;
        rows.map(|row| RepoPath::parse(&row?)).collect()
    }

    fn search_file_notes(&self, query: &str, limit: usize) -> Result<Vec<(i64, RepoPath)>> {
        let mut statement = self.connection.prepare(
            "SELECT notes.id, notes.path FROM notes_fts JOIN notes ON notes.id = notes_fts.rowid
             WHERE notes_fts MATCH ?1 ORDER BY notes_fts.rank LIMIT ?2",
        )?;
        let rows = statement.query_map(params![query, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, path) = row?;
            Ok((id, RepoPath::parse(&path)?))
        })
        .collect()
    }

    fn search_project_notes(
        &self,
        query: &str,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT project_notes.id FROM project_notes_fts
             JOIN project_notes ON project_notes.id = project_notes_fts.rowid
             WHERE project_notes_fts MATCH ?1 AND (project_notes.branch = '' OR project_notes.branch = ?2)
             ORDER BY project_notes_fts.rank LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![query, branch.unwrap_or(GENERAL_BRANCH), limit as i64],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn cached_parse(&self, blob_hash: &str) -> Result<Option<ParsedCode>> {
        let code: Option<String> = self
            .connection
            .query_row(
                "SELECT code FROM parsed WHERE blob_hash = ?1",
                [blob_hash],
                |row| row.get(0),
            )
            .optional()?;
        code.map(|code| serde_json::from_str(&code).context("invalid entry in the parse cache"))
            .transpose()
    }

    fn code_edges_from(&self, path: &RepoPath) -> Result<Vec<Edge>> {
        Ok(self
            .edges_from(path)?
            .into_iter()
            .filter(|edge| edge.kind != EdgeKind::SeeAlso)
            .collect())
    }

    fn event_count(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))?)
    }

    /// Logs `event`, which happened at `at` (an RFC 3339 time), and applies it
    /// to the tables, in one transaction. Returns the event's id in the log.
    fn apply(&mut self, event: &Event, at: &str) -> Result<i64> {
        let transaction = self.write_transaction()?;
        let event_id = log_and_apply(&transaction, event, at)?;
        transaction.commit()?;
        Ok(event_id)
    }

    /// Records `proposal` as a `NoteProposed` event with a new proposal id,
    /// and returns that id. The id is taken in the same transaction, so two
    /// connections writing at once never get the same one.
    fn record_proposal(&mut self, proposal: &NoteProposal, at: &str) -> Result<i64> {
        let transaction = self.write_transaction()?;
        let proposal_id: i64 = transaction.query_row(
            "SELECT coalesce(max(id), 0) + 1 FROM proposals",
            [],
            |row| row.get(0),
        )?;
        let event = Event::NoteProposed {
            proposal_id,
            proposal: proposal.clone(),
        };
        log_and_apply(&transaction, &event, at)?;
        transaction.commit()?;
        Ok(proposal_id)
    }

    /// Applies `event`, the outcome of proposal `proposal_id`, and marks the
    /// proposal handled in the same transaction, so a crash in between can
    /// never apply it twice.
    fn apply_for_proposal(&mut self, event: &Event, proposal_id: i64, at: &str) -> Result<()> {
        let transaction = self.write_transaction()?;
        log_and_apply(&transaction, event, at)?;
        transaction.execute(
            "UPDATE proposals SET state = 'done' WHERE id = ?1",
            [proposal_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Remembers what the parser found in the content with `blob_hash`, so
    /// the same content is never parsed again. The cache is not memory's
    /// content, so it is written directly, not through an event.
    fn cache_parse(&mut self, blob_hash: &str, code: &ParsedCode) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO parsed (blob_hash, code) VALUES (?1, ?2)",
            params![blob_hash, serde_json::to_string(code)?],
        )?;
        Ok(())
    }

    fn session(&self, id: &SessionId) -> Result<Option<StoredSession>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"
        ))?;
        let mut rows = statement.query_map([id.as_str()], session_row)?;
        rows.next().map(|row| stored_session(row?)).transpose()
    }

    fn recent_sessions(&self, limit: usize) -> Result<Vec<StoredSession>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS_WITHOUT_SNAPSHOTS} FROM sessions
             ORDER BY started_at DESC, rowid DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map([limit as i64], session_row)?;
        rows.map(|row| stored_session(row?)).collect()
    }

    /// Saves `session`; an upsert keeps the row's `rowid`, so sessions that
    /// started at the same second stay in the order they were first saved.
    fn save_session(&mut self, session: &StoredSession) -> Result<()> {
        self.connection.execute(
            "INSERT INTO sessions (id, harness, started_at, ended_at, files_at_start,
                                   files_at_last_note, blocked, notes_saved, observation)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (id) DO UPDATE SET
               harness = excluded.harness, started_at = excluded.started_at,
               ended_at = excluded.ended_at, files_at_start = excluded.files_at_start,
               files_at_last_note = excluded.files_at_last_note, blocked = excluded.blocked,
               notes_saved = excluded.notes_saved, observation = excluded.observation",
            params![
                session.id.as_str(),
                session.harness,
                session.started_at,
                session.ended_at,
                snapshot_json(session.files_at_start.as_ref())?,
                snapshot_json(session.files_at_last_note.as_ref())?,
                session.blocked,
                session.notes_saved,
                serde_json::to_string(&session.observation)?,
            ],
        )?;
        Ok(())
    }

    fn prune_sessions(&mut self, keep: usize, keep_snapshots: usize) -> Result<()> {
        let transaction = self.write_transaction()?;
        transaction.execute(
            "DELETE FROM sessions WHERE id NOT IN
               (SELECT id FROM sessions ORDER BY started_at DESC, rowid DESC LIMIT ?1)",
            [keep as i64],
        )?;
        transaction.execute(
            "UPDATE sessions SET files_at_start = NULL, files_at_last_note = NULL
             WHERE (files_at_start IS NOT NULL OR files_at_last_note IS NOT NULL)
               AND id NOT IN (SELECT id FROM sessions ORDER BY started_at DESC, rowid DESC LIMIT ?1)",
            [keep_snapshots as i64],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn record_tool_call(
        &mut self,
        at: &str,
        harness: &str,
        session: Option<&SessionId>,
        tool: &str,
    ) -> Result<()> {
        let transaction = self.write_transaction()?;
        transaction.execute(
            "INSERT INTO tool_calls (at, harness, tool, session) VALUES (?1, ?2, ?3, ?4)",
            params![at, harness, tool, session.map_or("", SessionId::as_str)],
        )?;
        // Rows are only appended, so the newest have the highest rowids.
        transaction.execute(
            "DELETE FROM tool_calls WHERE rowid <= (SELECT max(rowid) FROM tool_calls) - ?1",
            [TOOL_CALLS_KEPT],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

/// Tool calls kept in the log.
const TOOL_CALLS_KEPT: i64 = 10_000;

/// The columns `session_row` reads, in its order.
const SESSION_COLUMNS: &str = "id, harness, started_at, ended_at, files_at_start, \
                               files_at_last_note, blocked, notes_saved, observation";

/// `SESSION_COLUMNS` with NULL for the snapshots, which can be large.
const SESSION_COLUMNS_WITHOUT_SNAPSHOTS: &str = "id, harness, started_at, ended_at, NULL, NULL, \
                                                 blocked, notes_saved, observation";

/// One row of `sessions`, as SQLite gives it.
type SessionRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    i64,
    String,
);

fn session_row(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn stored_session(row: SessionRow) -> Result<StoredSession> {
    let (
        id,
        harness,
        started_at,
        ended_at,
        at_start,
        at_last_note,
        blocked,
        notes_saved,
        observation,
    ) = row;
    Ok(StoredSession {
        id: SessionId::parse(&id)?,
        harness,
        started_at,
        ended_at,
        files_at_start: snapshot(at_start)?,
        files_at_last_note: snapshot(at_last_note)?,
        blocked,
        notes_saved,
        observation: serde_json::from_str(&observation)
            .context("invalid observation in the database")?,
    })
}

fn snapshot_json(files: Option<&FileSnapshot>) -> Result<Option<String>> {
    Ok(files.map(serde_json::to_string).transpose()?)
}

fn snapshot(json: Option<String>) -> Result<Option<FileSnapshot>> {
    json.map(|text| serde_json::from_str(&text).context("invalid file snapshot in the database"))
        .transpose()
}

fn edges(connection: &Connection, query: &str, path: &RepoPath) -> Result<Vec<Edge>> {
    let mut statement = connection.prepare(query)?;
    let rows = statement.query_map([path.as_str()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (other, kind) = row?;
        Ok(Edge {
            to: RepoPath::parse(&other)?,
            kind: edge_kind(&kind)?,
        })
    })
    .collect()
}

fn note_text(summary: &str, body: &str) -> Result<NoteText> {
    Ok(NoteText {
        summary: NoteSummary::parse(summary)?,
        body: NoteBody::parse(body)?,
    })
}

fn edge_kind(text: &str) -> Result<EdgeKind> {
    match text {
        "imports" => Ok(EdgeKind::Imports),
        "contains" => Ok(EdgeKind::Contains),
        "see_also" => Ok(EdgeKind::SeeAlso),
        _ => bail!("unknown edge kind \"{text}\" in the database"),
    }
}
