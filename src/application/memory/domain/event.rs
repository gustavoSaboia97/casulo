//! Events: every change to memory arrives as one. Each event carries
//! everything needed to apply it, and applying it twice gives the same
//! result, so a retried event never duplicates or double counts anything.

use serde::{Deserialize, Serialize};

use super::note::{Author, EdgeKind, NoteBody, NoteKind, NoteProposal, NoteSummary, NoteTarget};
use super::repo_path::RepoPath;
use super::topic::Topic;

/// One change to memory.
// `tag`/`content` store each event as `{"kind": "...", "payload": {...}}`,
// which keeps the kind readable in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum Event {
    /// A file of the checkout was read: its content hash, its links and its
    /// top-level symbols. Replaces what was known about the file.
    FileIndexed {
        /// The file.
        path: RepoPath,
        /// Git blob hash of its content on disk.
        blob_hash: String,
        /// Files it links to.
        edges: Vec<Edge>,
        /// Names of its top-level functions, structs, enums and traits.
        symbols: Vec<String>,
    },
    /// A file left the checkout.
    FileRemoved {
        /// The file.
        path: RepoPath,
    },
    /// The local branches and their heads. Branches not listed are gone.
    BranchesScanned {
        /// Every local branch and its head commit.
        branches: Vec<BranchHead>,
        /// The files of each branch whose head moved since the last scan.
        moved: Vec<BranchFiles>,
    },
    /// The Inbox validated a proposal and queued it for the Reconciler.
    NoteProposed {
        /// The proposal's id.
        proposal_id: i64,
        /// The validated proposal.
        proposal: NoteProposal,
    },
    /// A note was written: added, or the same identity replaced.
    NoteAccepted {
        /// The note.
        note: AcceptedNote,
    },
    /// The same note was proposed again; the count is the new total.
    NoteReinforced {
        /// The note.
        note: NoteRef,
        /// How many times it has been reinforced, after this event.
        reinforced: i64,
    },
    /// A different text was proposed for the same identity; both are kept.
    NoteConflicted {
        /// The note.
        note: NoteRef,
        /// The proposal that conflicts.
        proposal_id: i64,
        /// The other text.
        other: NoteText,
    },
    /// A conflict was settled; this text wins.
    NoteResolved {
        /// The note.
        note: NoteRef,
        /// The text that stays.
        winner: NoteText,
        /// Who stands behind the winning text now, when that changes it;
        /// `None` keeps the note's author. Missing in older event logs.
        #[serde(default)]
        author: Option<Author>,
    },
    /// A branch note became a general project note.
    NotePromoted {
        /// The branch note, which is removed.
        branch_note_id: i64,
        /// The general note that replaces it.
        general: AcceptedNote,
    },
    /// A note was removed.
    NoteDeleted {
        /// The note.
        note: NoteRef,
    },
}

impl Event {
    /// The event's name, as stored in the log's `kind` column.
    pub fn name(&self) -> &'static str {
        match self {
            Self::FileIndexed { .. } => "file_indexed",
            Self::FileRemoved { .. } => "file_removed",
            Self::BranchesScanned { .. } => "branches_scanned",
            Self::NoteProposed { .. } => "note_proposed",
            Self::NoteAccepted { .. } => "note_accepted",
            Self::NoteReinforced { .. } => "note_reinforced",
            Self::NoteConflicted { .. } => "note_conflicted",
            Self::NoteResolved { .. } => "note_resolved",
            Self::NotePromoted { .. } => "note_promoted",
            Self::NoteDeleted { .. } => "note_deleted",
        }
    }
}

/// A link from the indexed file to another one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// The file linked to.
    pub to: RepoPath,
    /// How.
    pub kind: EdgeKind,
}

/// A local branch and its head commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchHead {
    /// Branch name.
    pub branch: String,
    /// Commit its head points to.
    pub head_commit: String,
}

/// The files of a branch, for a branch whose head moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchFiles {
    /// Branch name.
    pub branch: String,
    /// Each file and its blob hash at the branch head.
    pub files: Vec<(RepoPath, String)>,
}

/// Which note an event is about. File notes and project notes live in
/// different tables, so the id alone is not enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteRef {
    /// A file note, by id.
    File(i64),
    /// A project or branch note, by id.
    Project(i64),
}

/// The text of a note: its summary and body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteText {
    /// One line.
    pub summary: NoteSummary,
    /// The details.
    pub body: NoteBody,
}

/// A note as written to the tables. Its identity is the target plus the
/// kind, so accepting it again updates the same row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedNote {
    /// Where it belongs.
    pub target: NoteTarget,
    /// What it says.
    pub kind: NoteKind,
    /// One line.
    pub summary: NoteSummary,
    /// The details.
    pub body: NoteBody,
    /// Related files.
    pub refs: Vec<RepoPath>,
    /// Who wrote it.
    pub author: Author,
}

impl AcceptedNote {
    /// The topic of a project or branch note, `None` for a file note.
    pub fn topic(&self) -> Option<&Topic> {
        match &self.target {
            NoteTarget::File { .. } => None,
            NoteTarget::Project { topic } | NoteTarget::Branch { topic, .. } => Some(topic),
        }
    }
}
