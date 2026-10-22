//! The user's side of the project memory: editing notes in their editor
//! and promoting a branch's topic. Every change goes through the memory
//! services' checks as a note by the user, then the note files are rewritten.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::memory::domain::{RepoPath, Topic};
use crate::application::memory::ports::{Clock, Git, MemoryStore};
use crate::application::memory::services::{note_files, user_notes};
use crate::application::shared::domain::{CasuloHome, EditorCommand};
use crate::application::shared::ports::Editor;

/// The memory of one project, opened.
pub struct OpenMemory {
    /// The project's store. A trait object has no size known at compile
    /// time, so a struct keeps it behind a pointer, a `Box`.
    pub database: Box<dyn MemoryStore>,
    /// The project's workspace.
    pub workspace: PathBuf,
    /// Where the note files go, `memory/notes/`.
    pub notes_dir: PathBuf,
}

/// The note file of `path` to edit: its notes, or a template for a first one.
pub fn note_file_text(database: &dyn MemoryStore, path: &RepoPath) -> Result<String> {
    Ok(match note_files::render_file_notes(database, path)? {
        Some(text) => text,
        None => format!(
            "+++\npath = \"{path}\"\n+++\n\n## purpose\n**Say in one line what this file is for**\n"
        ),
    })
}

/// The note file of `topic`, general or of `branch`, to edit: its notes,
/// or a template for a first one.
pub fn topic_file_text(
    database: &dyn MemoryStore,
    branch: Option<&str>,
    topic: &Topic,
) -> Result<String> {
    Ok(
        match note_files::render_topic(database, branch, topic.as_str())? {
            Some(text) => text,
            None => format!(
                "+++\ntopic = \"{topic}\"\n+++\n\n## convention\n**Say the convention in one line**\n"
            ),
        },
    )
}

/// Opens `text` in the user's editor, as a copy under `edits/` named
/// `file_name`. Returns the saved text and the copy, or `None` when it was
/// not changed.
pub fn edit_copy(
    home: &CasuloHome,
    file_name: &str,
    text: &str,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<Option<(String, PathBuf)>> {
    let edits_dir = home.edits_dir();
    fs::create_dir_all(&edits_dir)
        .with_context(|| format!("cannot create {}", edits_dir.display()))?;
    let copy = edits_dir.join(file_name);
    fs::write(&copy, text).with_context(|| format!("cannot write {}", copy.display()))?;
    editor.edit(editor_command, &copy)?;
    let saved =
        fs::read_to_string(&copy).with_context(|| format!("cannot read {}", copy.display()))?;
    if saved == text {
        let _ = fs::remove_file(&copy);
        return Ok(None);
    }
    Ok(Some((saved, copy)))
}

/// Saves the user's edit of the note file of `path`, then rewrites that
/// file. Returns how many notes changed. When it fails, the edited `copy`
/// is kept so nothing typed is lost.
pub fn save_note_edit(
    memory: &mut OpenMemory,
    git: &dyn Git,
    clock: &dyn Clock,
    path: &RepoPath,
    edited: &str,
    copy: &Path,
) -> Result<usize> {
    // `*` takes the store out of its `Box`, so `&mut *` lends the
    // `dyn MemoryStore` itself; `&mut memory.database` would lend the `Box`.
    let applied = user_notes::apply_file_edit(
        &mut *memory.database,
        &memory.workspace,
        git,
        path,
        edited,
        &clock.now(),
    );
    let changed = keep_copy_on_error(applied, copy)?;
    note_files::sync_file_notes(&memory.notes_dir, &*memory.database, path)?;
    Ok(changed)
}

/// Saves the user's edit of the note file of `topic`, general or of
/// `branch`, then rewrites that file. Returns how many notes changed.
pub fn save_topic_edit(
    memory: &mut OpenMemory,
    clock: &dyn Clock,
    branch: Option<&str>,
    topic: &Topic,
    edited: &str,
    copy: &Path,
) -> Result<usize> {
    let applied =
        user_notes::apply_topic_edit(&mut *memory.database, branch, topic, edited, &clock.now());
    let changed = keep_copy_on_error(applied, copy)?;
    note_files::sync_topic(&memory.notes_dir, &*memory.database, branch, topic.as_str())?;
    Ok(changed)
}

/// Turns the notes of `topic` on `branch` into general ones, then rewrites
/// both note files. Returns how many notes were promoted.
pub fn promote_topic(
    memory: &mut OpenMemory,
    clock: &dyn Clock,
    branch: &str,
    topic: &Topic,
) -> Result<usize> {
    let promoted = user_notes::promote_topic(&mut *memory.database, branch, topic, &clock.now())?;
    for scope in [Some(branch), None] {
        note_files::sync_topic(&memory.notes_dir, &*memory.database, scope, topic.as_str())?;
    }
    Ok(promoted)
}

/// Removes the edited copy once its changes are saved; on an error keeps it
/// and says where it is.
fn keep_copy_on_error(result: Result<usize>, copy: &Path) -> Result<usize> {
    match result {
        Ok(changed) => {
            let _ = fs::remove_file(copy);
            Ok(changed)
        }
        Err(error) => Err(error.context(format!(
            "nothing was saved; your edit is kept in {}",
            copy.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;

    use super::*;
    use crate::infrastructure::sqlite::Database;

    /// An editor that writes `text` into the file it opens.
    struct TypingEditor {
        text: Option<String>,
        opened: RefCell<Vec<PathBuf>>,
    }

    impl Editor for TypingEditor {
        fn edit(&self, _editor: &EditorCommand, file: &Path) -> Result<()> {
            self.opened.borrow_mut().push(file.to_path_buf());
            if let Some(text) = &self.text {
                fs::write(file, text)?;
            }
            Ok(())
        }
    }

    fn editor(text: Option<&str>) -> TypingEditor {
        TypingEditor {
            text: text.map(str::to_string),
            opened: RefCell::new(Vec::new()),
        }
    }

    fn home(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap()
    }

    fn command() -> EditorCommand {
        EditorCommand::parse(
            "vi",
            crate::application::shared::domain::editor::EditorSource::ConfigKey,
        )
        .unwrap()
    }

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> String {
            "2026-10-11T12:00:00Z".to_string()
        }
    }

    #[test]
    fn an_unchanged_copy_is_removed_and_reported_as_none() {
        let directory = tempfile::tempdir().unwrap();
        let editor = editor(None);

        let edited =
            edit_copy(&home(&directory), "note.md", "text\n", &command(), &editor).unwrap();

        assert_eq!(edited, None);
        assert!(!editor.opened.borrow()[0].exists());
    }

    #[test]
    fn a_changed_copy_is_returned_and_kept_until_saved() {
        let directory = tempfile::tempdir().unwrap();
        let editor = editor(Some("changed\n"));

        let (saved, copy) = edit_copy(&home(&directory), "note.md", "text\n", &command(), &editor)
            .unwrap()
            .unwrap();

        assert_eq!(saved, "changed\n");
        assert!(copy.starts_with(home(&directory).edits_dir()));
        assert!(copy.exists());
    }

    #[test]
    fn a_failed_save_keeps_the_copy_and_says_where() {
        let directory = tempfile::tempdir().unwrap();
        let copy = directory.path().join("edit.md");
        fs::write(&copy, "typed").unwrap();
        let mut memory = OpenMemory {
            database: Box::new(Database::open_in_memory().unwrap()),
            workspace: directory.path().to_path_buf(),
            notes_dir: directory.path().join("notes"),
        };

        let error = save_topic_edit(
            &mut memory,
            &FixedClock,
            None,
            &Topic::parse("errors").unwrap(),
            "not a note file",
            &copy,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("your edit is kept in"));
        assert!(copy.exists());
    }

    #[test]
    fn a_new_topic_starts_from_a_template() {
        let database = Database::open_in_memory().unwrap();

        let text = topic_file_text(&database, None, &Topic::parse("errors").unwrap()).unwrap();

        assert!(text.starts_with("+++\ntopic = \"errors\"\n+++\n"));
        assert!(text.contains("## convention"));
    }
}
