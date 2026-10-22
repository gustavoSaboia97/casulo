//! `casulo config`: open a `casulo.toml` in the user's editor and check it
//! when the editor closes.

use anyhow::{Context, Result, bail};

use crate::application::project::ports::ConfigStore;
use crate::application::shared::domain::{CasuloHome, EditorCommand, ProjectName};
use crate::application::shared::ports::Editor;

/// Opens the global `casulo.toml`, creating it from the commented template
/// first when it does not exist, then checks that it still parses.
pub fn edit_global_config(
    home: &CasuloHome,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
    config_store: &dyn ConfigStore,
) -> Result<()> {
    let path = home.global_config_file();
    config_store.create_global_config(&path)?;
    editor.edit(editor_command, &path)?;
    config_store
        .load_global_config(&path)
        .with_context(|| saved_with_error(""))?;
    Ok(())
}

/// Opens the `casulo.toml` of `project`, then checks that it still parses.
pub fn edit_project_config(
    home: &CasuloHome,
    project: &ProjectName,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
    config_store: &dyn ConfigStore,
) -> Result<()> {
    let path = home.project_config_file(project);
    if !path.is_file() {
        bail!("project \"{project}\" does not exist; create it with `casulo new <git-url>`");
    }
    editor.edit(editor_command, &path)?;
    let again = format!(" --project {project}");
    config_store
        .load_project_config(&path)
        .with_context(|| saved_with_error(&again))?;
    Ok(())
}

/// What to do about a parse error found right after editing, so a
/// typo is fixed now instead of failing the next `casulo run`.
fn saved_with_error(arguments: &str) -> String {
    format!("the file was saved with an error; run `casulo config{arguments}` to fix it")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::application::shared::domain::EditorSource;
    use crate::infrastructure::config_file::{self, ConfigFiles};

    /// An editor that records the file it opened and replaces its text.
    struct FakeEditor {
        new_text: Option<String>,
        opened: RefCell<Vec<PathBuf>>,
    }

    impl FakeEditor {
        fn writing(new_text: Option<&str>) -> Self {
            Self {
                new_text: new_text.map(str::to_string),
                opened: RefCell::new(Vec::new()),
            }
        }
    }

    impl Editor for FakeEditor {
        fn edit(&self, _editor: &EditorCommand, file: &Path) -> Result<()> {
            self.opened.borrow_mut().push(file.to_path_buf());
            if let Some(text) = &self.new_text {
                fs::write(file, text)?;
            }
            Ok(())
        }
    }

    fn home_in(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path())), None).expect("override is set")
    }

    fn vi() -> EditorCommand {
        EditorCommand::parse("vi", EditorSource::Default).expect("has a program")
    }

    #[test]
    fn creates_the_global_file_from_the_template_before_opening_it() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = FakeEditor::writing(None);

        edit_global_config(&home, &vi(), &editor, &ConfigFiles).unwrap();

        assert_eq!(*editor.opened.borrow(), [home.global_config_file()]);
        assert_eq!(
            fs::read_to_string(home.global_config_file()).unwrap(),
            config_file::global_config_text()
        );
    }

    #[test]
    fn reports_an_invalid_global_file_with_its_path() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = FakeEditor::writing(Some("runtime = \n"));

        let error = edit_global_config(&home, &vi(), &editor, &ConfigFiles).unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains(&home.global_config_file().display().to_string()));
        assert!(message.contains("saved with an error; run `casulo config` to fix it"));
    }

    #[test]
    fn checks_the_project_file_after_editing() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let project = ProjectName::parse("app").unwrap();
        let path = home.project_config_file(&project);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "repository = \"https://example.com/app.git\"\n").unwrap();

        edit_project_config(
            &home,
            &project,
            &vi(),
            &FakeEditor::writing(None),
            &ConfigFiles,
        )
        .unwrap();
        let error = edit_project_config(
            &home,
            &project,
            &vi(),
            &FakeEditor::writing(Some("harnes = \"codex\"\n")),
            &ConfigFiles,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("run `casulo config --project app` to fix it"));
    }

    #[test]
    fn missing_project_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = FakeEditor::writing(None);

        let error = edit_project_config(
            &home,
            &ProjectName::parse("nope").unwrap(),
            &vi(),
            &editor,
            &ConfigFiles,
        )
        .unwrap_err();

        assert!(error.to_string().contains("does not exist"));
        assert!(editor.opened.borrow().is_empty());
    }
}
