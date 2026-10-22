//! Which stored note of a file applies to the checkout. A file keeps one
//! note per kind for each version of its content, so every reader picks
//! the same one the same way.

use crate::application::memory::domain::{
    NoteKind, NoteStatus, NoteVersion, StoredFileNote, note_status,
};

/// Among the notes of one file (oldest first), the `kind` note that applies
/// to the content `current`: the one written for that content, else the
/// newest one. Also returns its status for that content, `None` when the
/// file is not in the checkout. `None` when the file has no `kind` note.
pub fn applicable(
    notes: &[StoredFileNote],
    kind: NoteKind,
    current: Option<&str>,
) -> Option<(StoredFileNote, Option<NoteStatus>)> {
    let of_kind: Vec<&StoredFileNote> = notes.iter().filter(|note| note.kind == kind).collect();
    let shown = current
        .and_then(|hash| of_kind.iter().find(|note| note.code_hash == hash))
        .or_else(|| of_kind.last())?;
    let status = current.and_then(|hash| {
        let versions: Vec<NoteVersion> = of_kind
            .iter()
            .map(|note| NoteVersion {
                code_hash: note.code_hash.clone(),
                in_conflict: note.conflict.is_some(),
            })
            .collect();
        note_status(&versions, hash)
    });
    Some(((*shown).clone(), status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::NoteText;
    use crate::application::memory::domain::{Author, NoteBody, NoteSummary, RepoPath};

    fn note(id: i64, kind: NoteKind, code_hash: &str, conflict: bool) -> StoredFileNote {
        let text = NoteText {
            summary: NoteSummary::parse(&format!("Note number {id} of the file")).unwrap(),
            body: NoteBody::parse("").unwrap(),
        };
        StoredFileNote {
            id,
            path: RepoPath::parse("src/main.rs").unwrap(),
            kind,
            text: text.clone(),
            refs: Vec::new(),
            code_hash: code_hash.to_string(),
            conflict: conflict.then_some(text),
            reinforced: 0,
            author: Author::Harness("claude".to_string()),
        }
    }

    #[test]
    fn the_note_for_the_content_on_disk_wins_over_a_newer_one() {
        let notes = [
            note(1, NoteKind::Convention, "a1", false),
            note(2, NoteKind::Convention, "b2", false),
            note(3, NoteKind::Gotcha, "a1", false),
        ];

        let (shown, status) = applicable(&notes, NoteKind::Convention, Some("a1")).unwrap();

        assert_eq!((shown.id, status), (1, Some(NoteStatus::Ok)));
    }

    #[test]
    fn otherwise_the_newest_is_shown_stale() {
        let notes = [
            note(1, NoteKind::Convention, "a1", false),
            note(2, NoteKind::Convention, "b2", false),
        ];

        let (shown, status) = applicable(&notes, NoteKind::Convention, Some("c3")).unwrap();

        assert_eq!((shown.id, status), (2, Some(NoteStatus::Stale)));
    }

    #[test]
    fn a_conflict_on_the_current_content_says_so() {
        let notes = [note(1, NoteKind::Convention, "a1", true)];

        let (_, status) = applicable(&notes, NoteKind::Convention, Some("a1")).unwrap();

        assert_eq!(status, Some(NoteStatus::Conflict));
    }

    #[test]
    fn without_a_checkout_the_newest_is_shown_without_status() {
        let notes = [
            note(1, NoteKind::Convention, "a1", false),
            note(2, NoteKind::Convention, "b2", false),
        ];

        let (shown, status) = applicable(&notes, NoteKind::Convention, None).unwrap();

        assert_eq!((shown.id, status), (2, None));
        assert!(applicable(&notes, NoteKind::Purpose, Some("a1")).is_none());
    }
}
