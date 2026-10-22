//! Models and value objects of memory. Everything here is already valid once built.

pub mod event;
pub mod note;
pub mod repo_path;
pub mod rust_modules;
pub mod session;
pub mod topic;

pub use note::{
    Author, EdgeKind, NoteBody, NoteKind, NoteProposal, NoteStatus, NoteSummary, NoteTarget,
    NoteVersion, StoredFileNote, StoredProjectNote, note_status,
};
pub use repo_path::RepoPath;
pub use session::{FileSnapshot, Observation, SessionId, StopDecision, StoredSession};
pub use topic::Topic;
