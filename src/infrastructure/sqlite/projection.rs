//! How each event changes the tables of `memory.sqlite`.

use anyhow::{Context, Result};
use rusqlite::{Transaction, params};

use super::GENERAL_BRANCH;
use crate::application::memory::domain::event::{AcceptedNote, Event, NoteRef};
use crate::application::memory::domain::{Author, NoteTarget};

/// Logs `event` and applies it to the tables inside `transaction`.
pub(super) fn log_and_apply(transaction: &Transaction, event: &Event, at: &str) -> Result<i64> {
    let payload = serde_json::to_string(event).context("cannot serialize the event")?;
    transaction.execute(
        "INSERT INTO events (at, kind, payload) VALUES (?1, ?2, ?3)",
        params![at, event.name(), payload],
    )?;
    let event_id = transaction.last_insert_rowid();
    apply_to_tables(transaction, event, event_id, at)
        .with_context(|| format!("cannot apply the {} event", event.name()))?;
    Ok(event_id)
}

/// Applies one event to the tables. Every statement is written so that
/// applying the same event again leaves the tables as they are.
fn apply_to_tables(
    transaction: &Transaction,
    event: &Event,
    event_id: i64,
    at: &str,
) -> Result<()> {
    match event {
        Event::FileIndexed {
            path,
            blob_hash,
            edges,
            symbols,
        } => {
            transaction.execute(
                "INSERT INTO files (path, blob_hash) VALUES (?1, ?2)
                 ON CONFLICT (path) DO UPDATE SET blob_hash = excluded.blob_hash",
                params![path.as_str(), blob_hash],
            )?;
            // Edges cited by notes (`see_also`) are not the indexer's to replace.
            transaction.execute(
                "DELETE FROM edges WHERE from_path = ?1 AND kind != 'see_also'",
                [path.as_str()],
            )?;
            for edge in edges {
                transaction.execute(
                    "INSERT OR IGNORE INTO edges (from_path, to_path, kind) VALUES (?1, ?2, ?3)",
                    params![path.as_str(), edge.to.as_str(), edge.kind.as_str()],
                )?;
            }
            transaction.execute("DELETE FROM code_fts WHERE path = ?1", [path.as_str()])?;
            transaction.execute(
                "INSERT INTO code_fts (path, symbols) VALUES (?1, ?2)",
                params![path.as_str(), symbols.join(" ")],
            )?;
        }
        Event::FileRemoved { path } => {
            transaction.execute("DELETE FROM files WHERE path = ?1", [path.as_str()])?;
            transaction.execute(
                "DELETE FROM edges WHERE from_path = ?1 OR to_path = ?1",
                [path.as_str()],
            )?;
            transaction.execute("DELETE FROM code_fts WHERE path = ?1", [path.as_str()])?;
        }
        Event::BranchesScanned { branches, moved } => {
            let names: Vec<&str> = branches
                .iter()
                .map(|branch| branch.branch.as_str())
                .collect();
            let names_json = serde_json::to_string(&names)?;
            transaction.execute(
                "DELETE FROM branches WHERE name NOT IN (SELECT value FROM json_each(?1))",
                [&names_json],
            )?;
            transaction.execute(
                "DELETE FROM branch_blobs WHERE branch NOT IN (SELECT value FROM json_each(?1))",
                [&names_json],
            )?;
            for branch in branches {
                transaction.execute(
                    "INSERT INTO branches (name, head_commit) VALUES (?1, ?2)
                     ON CONFLICT (name) DO UPDATE SET head_commit = excluded.head_commit",
                    params![branch.branch, branch.head_commit],
                )?;
            }
            for branch in moved {
                transaction.execute(
                    "DELETE FROM branch_blobs WHERE branch = ?1",
                    [&branch.branch],
                )?;
                for (path, hash) in &branch.files {
                    transaction.execute(
                        "INSERT OR REPLACE INTO branch_blobs (branch, path, blob_hash) VALUES (?1, ?2, ?3)",
                        params![branch.branch, path.as_str(), hash],
                    )?;
                }
            }
        }
        Event::NoteProposed { proposal_id, .. } => {
            transaction.execute(
                "INSERT OR IGNORE INTO proposals (id, event_id, state) VALUES (?1, ?2, 'pending')",
                params![proposal_id, event_id],
            )?;
        }
        Event::NoteAccepted { note } => {
            upsert_note(transaction, note, at)?;
        }
        Event::NoteReinforced { note, reinforced } => {
            transaction.execute(
                &format!(
                    "UPDATE {} SET reinforced = ?2 WHERE id = ?1",
                    note_table(*note)
                ),
                params![note_id(*note), reinforced],
            )?;
        }
        Event::NoteConflicted { note, other, .. } => {
            transaction.execute(
                &format!(
                    "UPDATE {} SET conflict = ?2 WHERE id = ?1",
                    note_table(*note)
                ),
                params![note_id(*note), serde_json::to_string(other)?],
            )?;
        }
        Event::NoteResolved {
            note,
            winner,
            author,
        } => {
            // `coalesce` keeps the author when the event names none.
            transaction.execute(
                &format!(
                    "UPDATE {} SET summary = ?2, body = ?3, conflict = NULL,
                     author = coalesce(?4, author) WHERE id = ?1",
                    note_table(*note)
                ),
                params![
                    note_id(*note),
                    winner.summary.as_str(),
                    winner.body.as_str(),
                    author.as_ref().map(Author::to_text)
                ],
            )?;
            refresh_note_text(transaction, *note)?;
        }
        Event::NotePromoted {
            branch_note_id,
            general,
        } => {
            delete_note(transaction, NoteRef::Project(*branch_note_id))?;
            upsert_note(transaction, general, at)?;
        }
        Event::NoteDeleted { note } => {
            delete_note(transaction, *note)?;
        }
    }
    Ok(())
}

fn note_table(note: NoteRef) -> &'static str {
    match note {
        NoteRef::File(_) => "notes",
        NoteRef::Project(_) => "project_notes",
    }
}

fn note_id(note: NoteRef) -> i64 {
    match note {
        NoteRef::File(id) | NoteRef::Project(id) => id,
    }
}

/// Writes a note for its identity, replacing the text of the same identity
/// and clearing any conflict, then refreshes its search text.
fn upsert_note(transaction: &Transaction, note: &AcceptedNote, at: &str) -> Result<()> {
    let refs = serde_json::to_string(&note.refs)?;
    let author = note.author.to_text();
    let (summary, body, kind) = (
        note.summary.as_str(),
        note.body.as_str(),
        note.kind.as_str(),
    );
    let id: i64 = match &note.target {
        NoteTarget::File { path, code_hash } => transaction.query_row(
            "INSERT INTO notes (path, kind, summary, body, refs, code_hash, conflict, reinforced, author, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, ?7, ?8)
             ON CONFLICT (path, kind, code_hash) DO UPDATE SET
               summary = excluded.summary, body = excluded.body, refs = excluded.refs,
               author = excluded.author, conflict = NULL
             RETURNING id",
            params![path.as_str(), kind, summary, body, refs, code_hash, author, at],
            |row| row.get(0),
        )?,
        NoteTarget::Project { topic } => {
            upsert_project_note(transaction, GENERAL_BRANCH, topic.as_str(), note, &refs, &author, at)?
        }
        NoteTarget::Branch { branch, topic } => {
            upsert_project_note(transaction, branch, topic.as_str(), note, &refs, &author, at)?
        }
    };
    let reference = match note.target {
        NoteTarget::File { .. } => NoteRef::File(id),
        _ => NoteRef::Project(id),
    };
    refresh_note_text(transaction, reference)
}

fn upsert_project_note(
    transaction: &Transaction,
    branch: &str,
    topic: &str,
    note: &AcceptedNote,
    refs: &str,
    author: &str,
    at: &str,
) -> Result<i64> {
    Ok(transaction.query_row(
        "INSERT INTO project_notes (branch, topic, kind, summary, body, refs, conflict, reinforced, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 0, ?7, ?8)
         ON CONFLICT (branch, topic, kind) DO UPDATE SET
           summary = excluded.summary, body = excluded.body, refs = excluded.refs,
           author = excluded.author, conflict = NULL
         RETURNING id",
        params![
            branch,
            topic,
            note.kind.as_str(),
            note.summary.as_str(),
            note.body.as_str(),
            refs,
            author,
            at
        ],
        |row| row.get(0),
    )?)
}

/// Rewrites the search row of a note from its current text.
fn refresh_note_text(transaction: &Transaction, note: NoteRef) -> Result<()> {
    match note {
        NoteRef::File(id) => {
            transaction.execute("DELETE FROM notes_fts WHERE rowid = ?1", [id])?;
            transaction.execute(
                "INSERT INTO notes_fts (rowid, summary, body) SELECT id, summary, body FROM notes WHERE id = ?1",
                [id],
            )?;
        }
        NoteRef::Project(id) => {
            transaction.execute("DELETE FROM project_notes_fts WHERE rowid = ?1", [id])?;
            transaction.execute(
                "INSERT INTO project_notes_fts (rowid, topic, summary, body)
                 SELECT id, topic, summary, body FROM project_notes WHERE id = ?1",
                [id],
            )?;
        }
    }
    Ok(())
}

fn delete_note(transaction: &Transaction, note: NoteRef) -> Result<()> {
    let (table, search, vector_type) = match note {
        NoteRef::File(_) => ("notes", "notes_fts", "file"),
        NoteRef::Project(_) => ("project_notes", "project_notes_fts", "project"),
    };
    let id = note_id(note);
    transaction.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])?;
    transaction.execute(&format!("DELETE FROM {search} WHERE rowid = ?1"), [id])?;
    transaction.execute(
        "DELETE FROM note_vectors WHERE note_type = ?1 AND note_id = ?2",
        params![vector_type, id],
    )?;
    Ok(())
}
