//! What the user does with notes: list them, edit them as note files, and
//! promote a branch note to a general one. The user's text always wins in
//! the Reconciler, so an edit also settles a conflict.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::application::memory::domain::event::{AcceptedNote, Event};
use crate::application::memory::domain::{
    Author, NoteKind, NoteProposal, NoteStatus, NoteTarget, RepoPath, Topic,
};
use crate::application::memory::ports::{Git, MemoryStore};
use crate::application::memory::services::applicable::applicable;
use crate::application::memory::services::inbox::{NoteContent, check_content, record};
use crate::application::memory::services::note_files::{NoteSection, parse_note_file};
use crate::application::memory::services::reconciler::reconcile;

/// How a listed file note relates to the checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// Written for the content on disk.
    Ok,
    /// Written only for other versions of a checked-out file.
    Stale,
    /// Two texts were proposed for the content on disk.
    Conflict,
    /// A version kept for another branch (only with `all_versions`).
    OtherVersion,
}

/// One line of `casulo list memory note`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRow {
    /// The file.
    pub path: RepoPath,
    /// What the note says.
    pub kind: NoteKind,
    /// How it relates to the checkout.
    pub state: RowState,
    /// Who wrote it.
    pub author: Author,
    /// Its summary.
    pub summary: String,
}

/// One line per file and kind in the checkout, with the note that applies:
/// the one for the content on disk, else the newest one, marked stale.
/// With `all_versions`, every stored version is listed, including notes
/// kept only for other branches.
pub fn list_file_notes(database: &dyn MemoryStore, all_versions: bool) -> Result<Vec<NoteRow>> {
    let mut rows = Vec::new();
    for (path, kind) in database.noted_paths()? {
        let current = database.file_hash(&path)?;
        let notes: Vec<_> = database
            .file_notes(&path)?
            .into_iter()
            .filter(|note| note.kind == kind)
            .collect();
        if all_versions {
            for note in &notes {
                let state = match &current {
                    Some(hash) if &note.code_hash == hash && note.conflict.is_some() => {
                        RowState::Conflict
                    }
                    Some(hash) if &note.code_hash == hash => RowState::Ok,
                    _ => RowState::OtherVersion,
                };
                rows.push(NoteRow {
                    path: path.clone(),
                    kind,
                    state,
                    author: note.author.clone(),
                    summary: note.text.summary.as_str().to_string(),
                });
            }
            continue;
        }
        // Without the file in the checkout there is no status, so no row.
        let Some((note, Some(status))) = applicable(&notes, kind, current.as_deref()) else {
            continue;
        };
        let state = match status {
            NoteStatus::Ok => RowState::Ok,
            NoteStatus::Stale => RowState::Stale,
            NoteStatus::Conflict => RowState::Conflict,
        };
        rows.push(NoteRow {
            path: path.clone(),
            kind,
            state,
            author: note.author.clone(),
            summary: note.text.summary.as_str().to_string(),
        });
    }
    Ok(rows)
}

/// Applies the note file of `path` as the user edited it. Every section is
/// checked first, and nothing is applied if one is invalid. A changed
/// section replaces that note's text (and settles its conflict); a section
/// without a note id is a new note for the content on disk now. Returns how
/// many notes changed.
pub fn apply_file_edit(
    database: &mut dyn MemoryStore,
    workspace: &Path,
    git: &dyn Git,
    path: &RepoPath,
    edited: &str,
    at: &str,
) -> Result<usize> {
    let file = parse_note_file(edited)?;
    if file.header.get("path").and_then(|value| value.as_str()) != Some(path.as_str()) {
        bail!("the header must keep path = \"{path}\"; edit another file with its own command");
    }
    let stored: BTreeMap<i64, _> = database
        .file_notes(path)?
        .into_iter()
        .map(|note| (note.id, note))
        .collect();
    let mut proposals = Vec::new();
    for section in &file.sections {
        let code_hash = match section.note_id {
            Some(id) => {
                let note = stored.get(&id).with_context(|| {
                    format!("note {id} is not a note of {path}; do not change the note comments")
                })?;
                if note.kind != section.kind {
                    bail!(
                        "note {id} is a {} note; write a new section to add a {} note",
                        note.kind,
                        section.kind
                    );
                }
                let unchanged = note.text.summary.as_str() == section.summary
                    && note.text.body.as_str() == section.body
                    && note
                        .refs
                        .iter()
                        .map(|path| path.to_string())
                        .collect::<Vec<_>>()
                        == section.refs
                    && note.conflict.is_some() == section.conflict.is_some();
                if unchanged {
                    continue;
                }
                note.code_hash.clone()
            }
            None => git.hash_file(workspace, path).with_context(|| {
                format!("cannot hash {path}; a new note needs the file in the checkout")
            })?,
        };
        proposals.push(user_proposal(
            database,
            NoteTarget::File {
                path: path.clone(),
                code_hash,
            },
            section,
        )?);
    }
    apply_all(database, &proposals, at)
}

/// Applies a topic's note file as the user edited it: a general note, or a
/// note of `branch`. Returns how many notes changed.
pub fn apply_topic_edit(
    database: &mut dyn MemoryStore,
    branch: Option<&str>,
    topic: &Topic,
    edited: &str,
    at: &str,
) -> Result<usize> {
    let file = parse_note_file(edited)?;
    if file.header.get("topic").and_then(|value| value.as_str()) != Some(topic.as_str()) {
        bail!("the header must keep topic = \"{topic}\"");
    }
    let stored = database.project_notes(branch)?;
    let mut proposals = Vec::new();
    for section in &file.sections {
        if !section.kind.allowed_for_project() {
            bail!(
                "a project note is a convention or a decision, not a {}",
                section.kind
            );
        }
        let existing = stored.iter().find(|note| {
            note.topic == topic.as_str()
                && note.kind == section.kind
                && note.branch.as_deref() == branch
        });
        if let Some(note) = existing {
            let unchanged = note.text.summary.as_str() == section.summary
                && note.text.body.as_str() == section.body
                && note.conflict.is_some() == section.conflict.is_some();
            if unchanged {
                continue;
            }
        }
        let target = match branch {
            Some(branch) => NoteTarget::Branch {
                branch: branch.to_string(),
                topic: topic.clone(),
            },
            None => NoteTarget::Project {
                topic: topic.clone(),
            },
        };
        proposals.push(user_proposal(database, target, section)?);
    }
    apply_all(database, &proposals, at)
}

fn user_proposal(
    database: &dyn MemoryStore,
    target: NoteTarget,
    section: &NoteSection,
) -> Result<NoteProposal> {
    let NoteContent {
        summary,
        body,
        refs,
    } = check_content(
        database,
        &Author::User,
        &section.summary,
        &section.body,
        &section.refs,
        "See",
    )?;
    Ok(NoteProposal {
        target,
        kind: section.kind,
        summary,
        body,
        refs,
        author: Author::User,
    })
}

fn apply_all(
    database: &mut dyn MemoryStore,
    proposals: &[NoteProposal],
    at: &str,
) -> Result<usize> {
    for proposal in proposals {
        let id = record(database, proposal, at)?;
        reconcile(database, id, proposal, at)?;
    }
    Ok(proposals.len())
}

/// Turns the notes of `topic` on `branch` into general project notes, for
/// when a branch's decision becomes the project's. Returns how many notes
/// were promoted.
pub fn promote_topic(
    database: &mut dyn MemoryStore,
    branch: &str,
    topic: &Topic,
    at: &str,
) -> Result<usize> {
    let notes: Vec<_> = database
        .project_notes(Some(branch))?
        .into_iter()
        .filter(|note| note.topic == topic.as_str() && note.branch.as_deref() == Some(branch))
        .collect();
    if notes.is_empty() {
        bail!(
            "branch {branch} has no {topic} note to promote; `casulo list memory topic` shows them"
        );
    }
    for note in &notes {
        database.apply(
            &Event::NotePromoted {
                branch_note_id: note.id,
                general: AcceptedNote {
                    target: NoteTarget::Project {
                        topic: topic.clone(),
                    },
                    kind: note.kind,
                    summary: note.text.summary.clone(),
                    body: note.text.body.clone(),
                    refs: Vec::new(),
                    author: note.author.clone(),
                },
            },
            at,
        )?;
    }
    Ok(notes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::{NoteRef, NoteText};
    use crate::application::memory::domain::{NoteBody, NoteSummary};
    use crate::application::memory::services::note_files::{render_file_notes, render_topic};
    use crate::application::memory::test_support::FakeGit;
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-10T12:00:00Z";

    fn path() -> RepoPath {
        RepoPath::parse("src/main.rs").unwrap()
    }

    fn file_note(hash: &str, summary: &str, author: Author) -> Event {
        Event::NoteAccepted {
            note: AcceptedNote {
                target: NoteTarget::File {
                    path: path(),
                    code_hash: hash.to_string(),
                },
                kind: NoteKind::Convention,
                summary: NoteSummary::parse(summary).unwrap(),
                body: NoteBody::parse("").unwrap(),
                refs: Vec::new(),
                author,
            },
        }
    }

    fn database() -> Database {
        let mut database = Database::open_in_memory().unwrap();
        database
            .apply(
                &Event::FileIndexed {
                    path: path(),
                    blob_hash: "disk".to_string(),
                    edges: Vec::new(),
                    symbols: Vec::new(),
                },
                AT,
            )
            .unwrap();
        database
    }

    #[test]
    fn list_shows_the_note_that_applies_or_every_version() {
        let mut database = database();
        database
            .apply(
                &file_note("old", "Text for the old version", Author::Writer),
                AT,
            )
            .unwrap();
        database
            .apply(
                &file_note("gone-file", "Unused here at all", Author::Writer),
                AT,
            )
            .unwrap();

        let rows = list_file_notes(&database, false).unwrap();
        let all = list_file_notes(&database, true).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, RowState::Stale);
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|row| row.state == RowState::OtherVersion));
    }

    #[test]
    fn editing_a_section_replaces_its_text_and_settles_a_conflict() {
        let mut database = database();
        database
            .apply(
                &file_note(
                    "disk",
                    "Runs read-only always",
                    Author::Harness("claude".into()),
                ),
                AT,
            )
            .unwrap();
        database
            .apply(
                &Event::NoteConflicted {
                    note: NoteRef::File(1),
                    proposal_id: 9,
                    other: NoteText {
                        summary: NoteSummary::parse("May write to a cache").unwrap(),
                        body: NoteBody::parse("").unwrap(),
                    },
                },
                AT,
            )
            .unwrap();
        let rendered = render_file_notes(&database, &path()).unwrap().unwrap();
        let conflict_start = rendered.find("\n### conflict").unwrap();
        let edited = rendered[..conflict_start].replace(
            "Runs read-only always",
            "Runs read-only, settled by the user",
        ) + "\n";

        let changed = apply_file_edit(
            &mut database,
            Path::new("."),
            &FakeGit,
            &path(),
            &edited,
            AT,
        )
        .unwrap();

        assert_eq!(changed, 1);
        let notes = database.file_notes(&path()).unwrap();
        assert_eq!(
            notes[0].text.summary.as_str(),
            "Runs read-only, settled by the user"
        );
        assert_eq!(notes[0].conflict, None);
        assert_eq!(notes[0].author, Author::User);
    }

    #[test]
    fn an_unchanged_file_changes_nothing_and_a_new_section_adds_a_note() {
        let mut database = database();
        database
            .apply(
                &file_note("disk", "Runs read-only always", Author::Writer),
                AT,
            )
            .unwrap();
        let rendered = render_file_notes(&database, &path()).unwrap().unwrap();

        assert_eq!(
            apply_file_edit(
                &mut database,
                Path::new("."),
                &FakeGit,
                &path(),
                &rendered,
                AT
            )
            .unwrap(),
            0
        );

        let added = format!("{rendered}\n## gotcha\n**Mounts nested in state go last**\n");
        assert_eq!(
            apply_file_edit(&mut database, Path::new("."), &FakeGit, &path(), &added, AT).unwrap(),
            1
        );
        assert_eq!(database.file_notes(&path()).unwrap().len(), 2);
    }

    #[test]
    fn an_invalid_edit_applies_nothing() {
        let mut database = database();
        database
            .apply(
                &file_note("disk", "Runs read-only always", Author::Writer),
                AT,
            )
            .unwrap();
        let rendered = render_file_notes(&database, &path()).unwrap().unwrap();
        let edited = rendered.replace("Runs read-only always", "Changed text of the first note")
            + "\n## gotcha\n**short**\n";

        let error = apply_file_edit(
            &mut database,
            Path::new("."),
            &FakeGit,
            &path(),
            &edited,
            AT,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("10 to 150"));
        assert_eq!(
            database.file_notes(&path()).unwrap()[0]
                .text
                .summary
                .as_str(),
            "Runs read-only always"
        );
    }

    #[test]
    fn topics_are_edited_and_promoted() {
        let mut database = database();
        let topic = Topic::parse("runtime").unwrap();
        let text = "+++\ntopic = \"runtime\"\n+++\n\n## decision\n**This branch moves to Podman**\nIgnore the old adapter.\n";

        apply_topic_edit(&mut database, Some("podman"), &topic, text, AT).unwrap();
        assert!(
            render_topic(&database, Some("podman"), "runtime")
                .unwrap()
                .is_some()
        );

        let promoted = promote_topic(&mut database, "podman", &topic, AT).unwrap();

        assert_eq!(promoted, 1);
        assert!(
            render_topic(&database, Some("podman"), "runtime")
                .unwrap()
                .is_none()
        );
        let general = database.project_notes(None).unwrap();
        assert_eq!(
            general[0].text.summary.as_str(),
            "This branch moves to Podman"
        );
        assert_eq!(general[0].branch, None);
        assert!(promote_topic(&mut database, "podman", &topic, AT).is_err());
    }
}
