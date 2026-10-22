//! Traits for the outside world, implemented in `crate::infrastructure` and
//! replaced by fakes in tests.

use std::path::Path;

use anyhow::Result;

use crate::application::memory::domain::event::{BranchHead, Edge, Event};
use crate::application::memory::domain::rust_modules::ParsedCode;
use crate::application::memory::domain::{
    NoteKind, NoteProposal, RepoPath, SessionId, StoredFileNote, StoredProjectNote, StoredSession,
};

/// The parts of git memory needs. Everything is local; nothing here
/// touches the network.
pub trait Git {
    /// Tracked files and untracked files that are not ignored, each with the
    /// blob hash of its content **on disk** (not the staged or committed
    /// one). Symbolic links, submodules and deleted files are left out.
    fn ls_files(&self, directory: &Path) -> Result<Vec<(RepoPath, String)>>;

    /// The blob hash of one file's content on disk.
    fn hash_file(&self, directory: &Path, path: &RepoPath) -> Result<String>;

    /// The checked-out branch, or `None` when detached.
    fn current_branch(&self, directory: &Path) -> Result<Option<String>>;

    /// Every local branch and its head commit.
    fn local_branches(&self, directory: &Path) -> Result<Vec<BranchHead>>;

    /// Every file of `commit` and its blob hash.
    fn ls_tree(&self, directory: &Path, commit: &str) -> Result<Vec<(RepoPath, String)>>;
}

/// Reads source code. Returns `None` for a language it does not know.
pub trait CodeParser {
    /// What `source`, the content of `path`, declares and imports.
    fn parse(&self, path: &RepoPath, source: &str) -> Option<Result<ParsedCode>>;
}

/// The current time, so tests can control it.
pub trait Clock {
    /// Now, as an RFC 3339 UTC time such as `2026-10-09T12:00:00Z`.
    fn now(&self) -> String;
}

/// Where the memory of one project is kept: the files and links of the
/// checkout, the notes, and the log of every change. Reads never change it;
/// every change to memory's content is an `Event`, logged and applied at once.
pub trait MemoryStore {
    /// The blob hash indexed for `path`, if the file is in the checkout.
    fn file_hash(&self, path: &RepoPath) -> Result<Option<String>>;

    /// Every file of the checkout and its blob hash, sorted by path.
    fn files(&self) -> Result<Vec<(RepoPath, String)>>;

    /// Links from `path` to other files, sorted.
    fn edges_from(&self, path: &RepoPath) -> Result<Vec<Edge>>;

    /// Links from other files to `path`, sorted; `to` is the other file.
    fn edges_to(&self, path: &RepoPath) -> Result<Vec<Edge>>;

    /// Every stored note of `path`, oldest first.
    fn file_notes(&self, path: &RepoPath) -> Result<Vec<StoredFileNote>>;

    /// General project notes, and the notes of `branch` when given, sorted
    /// by topic.
    fn project_notes(&self, branch: Option<&str>) -> Result<Vec<StoredProjectNote>>;

    /// The file note with this identity, if one exists.
    fn find_file_note(
        &self,
        path: &RepoPath,
        kind: NoteKind,
        code_hash: &str,
    ) -> Result<Option<StoredFileNote>>;

    /// The project note with this identity, if one exists. `branch` is
    /// `None` for a general note.
    fn find_project_note(
        &self,
        branch: Option<&str>,
        topic: &str,
        kind: NoteKind,
    ) -> Result<Option<StoredProjectNote>>;

    /// Whether the Reconciler already handled proposal `id`.
    fn proposal_done(&self, id: i64) -> Result<bool>;

    /// The local branches as last scanned, sorted by name.
    fn branches(&self) -> Result<Vec<BranchHead>>;

    /// How many links the graph has.
    fn edge_count(&self) -> Result<i64>;

    /// How many file notes are stored, for every version of every file.
    fn file_note_count(&self) -> Result<i64>;

    /// Every `(path, kind)` that has at least one note, sorted.
    fn noted_paths(&self) -> Result<Vec<(RepoPath, NoteKind)>>;

    /// How many project notes are stored, general and of every branch.
    fn project_note_count(&self) -> Result<i64>;

    /// Files whose path or symbol names match `query` (an FTS5 query built
    /// by `fts_query`), best first.
    fn search_code(&self, query: &str, limit: usize) -> Result<Vec<RepoPath>>;

    /// File notes whose text matches `query`, best first, as `(note id, path)`.
    fn search_file_notes(&self, query: &str, limit: usize) -> Result<Vec<(i64, RepoPath)>>;

    /// Ids of project notes, general or of `branch`, whose topic or text
    /// matches `query`, best first.
    fn search_project_notes(
        &self,
        query: &str,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<i64>>;

    /// What the parser found in the content with `blob_hash`, if it was
    /// parsed before.
    fn cached_parse(&self, blob_hash: &str) -> Result<Option<ParsedCode>>;

    /// Links from `path` computed from the code (not the ones cited by notes).
    fn code_edges_from(&self, path: &RepoPath) -> Result<Vec<Edge>>;

    /// How many events are in the log.
    fn event_count(&self) -> Result<i64>;

    /// Logs `event`, which happened at `at` (an RFC 3339 time), and applies it.
    /// Returns the event's id in the log.
    fn apply(&mut self, event: &Event, at: &str) -> Result<i64>;

    /// Records `proposal` as a `NoteProposed` event with a new proposal id,
    /// and returns that id. Two writers at once never get the same one.
    fn record_proposal(&mut self, proposal: &NoteProposal, at: &str) -> Result<i64>;

    /// Applies `event`, the outcome of proposal `proposal_id`, and marks the
    /// proposal handled at once, so it can never be applied twice.
    fn apply_for_proposal(&mut self, event: &Event, proposal_id: i64, at: &str) -> Result<()>;

    /// Remembers what the parser found in the content with `blob_hash`, so
    /// the same content is never parsed again. The cache is not memory's
    /// content, so it is not an event.
    fn cache_parse(&mut self, blob_hash: &str, code: &ParsedCode) -> Result<()>;

    /// The session `id`, if it was ever started and is still kept.
    fn session(&self, id: &SessionId) -> Result<Option<StoredSession>>;

    /// The `limit` newest sessions, newest first, without their file
    /// snapshots (`None`), which only the hooks need.
    fn recent_sessions(&self, limit: usize) -> Result<Vec<StoredSession>>;

    /// Saves `session`, replacing the one with its id. Sessions are not
    /// memory's content, so this is not an event.
    fn save_session(&mut self, session: &StoredSession) -> Result<()>;

    /// Deletes every session but the `keep` newest, and drops the file
    /// snapshots of every session but the `keep_snapshots` newest.
    fn prune_sessions(&mut self, keep: usize, keep_snapshots: usize) -> Result<()>;

    /// Logs that `harness` called `tool` at `at`, in `session` when known.
    /// Only the newest 10,000 calls are kept.
    fn record_tool_call(
        &mut self,
        at: &str,
        harness: &str,
        session: Option<&SessionId>,
        tool: &str,
    ) -> Result<()>;
}
