//! What the command line reads from outside its arguments: environment
//! variables, the current directory, the disk and the terminal.

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::application::doctor::domain::HostOs;
use crate::application::project::domain::ProjectSource;
use crate::application::sandbox::domain::HostUser;
use crate::application::shared::domain::editor::{self, CASULO_EDITOR_VARIABLE};
use crate::application::shared::domain::{CasuloHome, EditorCommand, EditorSettings, ProjectName};
use crate::infrastructure::config_file;

/// Name of the environment variable that overrides the casulo home directory.
pub const HOME_OVERRIDE_VARIABLE: &str = "CASULO_HOME";

/// The repository URL or the directory given to `casulo new`, made absolute
/// against the current directory. A URL is checked without repeating it in
/// the error.
pub fn project_source(source: &str) -> Result<ProjectSource> {
    match ProjectSource::parse(source)? {
        ProjectSource::Directory(directory) => {
            let current_dir = env::current_dir().context("cannot read the current directory")?;
            Ok(ProjectSource::Directory(current_dir.join(directory)))
        }
        repository => Ok(repository),
    }
}

/// The user that should own files the agent writes: the owner of the
/// workspace. Only on Linux, where the container shares the host's user
/// ids; never root, so the agent does not run as root in the container.
pub fn workspace_owner(workspace: &Path) -> Result<Option<HostUser>> {
    let metadata = fs::metadata(workspace).with_context(|| {
        format!(
            "cannot read {}; was the project cloned?",
            workspace.display()
        )
    })?;
    Ok(owner_on_linux(&metadata))
}

#[cfg(target_os = "linux")]
fn owner_on_linux(metadata: &fs::Metadata) -> Option<HostUser> {
    // `MetadataExt` gives the Unix owner of a file; it only exists on Unix.
    use std::os::unix::fs::MetadataExt;
    let user = HostUser {
        uid: metadata.uid(),
        gid: metadata.gid(),
    };
    (user.uid != 0).then_some(user)
}

#[cfg(not(target_os = "linux"))]
fn owner_on_linux(_metadata: &fs::Metadata) -> Option<HostUser> {
    None
}

/// Names from `names` that are set in the environment, for masking in `--dry-run`.
pub fn names_set_in_environment(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| env::var_os(name).is_some())
        .cloned()
        .collect()
}

/// The project to work on: the one given, or the one whose directory
/// contains the current directory.
pub fn resolve_project(home: &CasuloHome, explicit: Option<ProjectName>) -> Result<ProjectName> {
    if let Some(project) = explicit {
        if !home.project_dir(&project).is_dir() {
            bail!(
                "project \"{project}\" does not exist; create it with `casulo new <git-url|dir>`"
            );
        }
        return Ok(project);
    }
    let current_dir = env::current_dir().context("cannot read the current directory")?;
    project_containing(home, &current_dir)
}

/// The project whose directory contains `current_dir`: its folder under the
/// casulo home, or the host directory it was created from.
// Apart from `resolve_project`, so a test can pass a directory instead of
// changing the current directory of the whole test process.
fn project_containing(home: &CasuloHome, current_dir: &Path) -> Result<ProjectName> {
    // Canonical paths, so a symbolic link (such as /var and /private/var on
    // macOS) does not hide that the current directory is inside a project.
    let current_dir = fs::canonicalize(current_dir).unwrap_or(current_dir.to_path_buf());
    let canonical_home = match fs::canonicalize(home.root()) {
        Ok(root) => CasuloHome::resolve(Some(root.into_os_string()), None),
        Err(_) => None,
    };
    if let Some(project) = canonical_home.and_then(|home| home.project_containing(&current_dir)) {
        return Ok(project);
    }
    directory_project_containing(home, &current_dir).context(
        "not inside a casulo project; run this from a project's workspace or pass --project <name>",
    )
}

/// The project whose workspace is a host directory containing `path`
/// (canonical), for projects created with `casulo new <dir>`.
fn directory_project_containing(home: &CasuloHome, path: &Path) -> Option<ProjectName> {
    let entries = fs::read_dir(home.projects_dir()).ok()?;
    let mut names: Vec<ProjectName> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(str::to_string))
        .filter_map(|name| ProjectName::parse(&name).ok())
        .collect();
    names.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    names.into_iter().find(|project| {
        let workspace = config_file::load_project_config(&home.project_config_file(project))
            .ok()
            .and_then(|config| config.workspace);
        workspace
            .and_then(|workspace| fs::canonicalize(workspace).ok())
            .is_some_and(|workspace| path.starts_with(workspace))
    })
}

/// Finds the casulo home from `CASULO_HOME` and the user home directory.
pub fn casulo_home_from_environment() -> Result<CasuloHome> {
    match CasuloHome::resolve(env::var_os(HOME_OVERRIDE_VARIABLE), env::home_dir()) {
        Some(home) => Ok(home),
        None => bail!(
            "cannot find your home directory; set {HOME_OVERRIDE_VARIABLE} to choose where casulo keeps its files"
        ),
    }
}

/// Detects the operating system, and the distribution on Linux.
pub fn host_os_from_environment() -> HostOs {
    // A missing os-release only means the distribution is unknown.
    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    HostOs::from_parts(env::consts::OS, &os_release)
}

/// The editor to open files in: `CASULO_EDITOR`, then the `editor` key of
/// the global config (`configured`), then `VISUAL`, then `EDITOR`.
pub fn editor_from_environment(configured: Option<String>) -> EditorCommand {
    let settings = EditorSettings {
        casulo_editor: env::var(CASULO_EDITOR_VARIABLE).ok(),
        config: configured,
        visual: env::var("VISUAL").ok(),
        editor: env::var("EDITOR").ok(),
    };
    editor::choose_editor(&settings, env::consts::OS)
}

/// Asks a yes/no question on stderr. Only `y` or `yes` counts as yes.
pub fn ask(input: &mut dyn BufRead, question: &str) -> Result<bool> {
    let answer = read_answer(input, question)?.to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Reads one answer. End of input (no terminal) counts as an empty answer,
/// so nothing runs without a person saying yes.
pub fn read_answer(input: &mut dyn BufRead, question: &str) -> Result<String> {
    eprint!("{question}");
    io::stderr().flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        // Nobody typed anything; end the prompt line before the next message.
        eprintln!();
    }
    Ok(answer.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> ProjectName {
        ProjectName::parse(text).expect("valid name")
    }

    /// A casulo home inside `root`, with the folder of project `app`.
    fn home_with_app(root: &Path) -> CasuloHome {
        let home = CasuloHome::resolve(Some(root.join("home").into_os_string()), None)
            .expect("a home from an explicit directory");
        fs::create_dir_all(home.project_dir(&name("app"))).unwrap();
        home
    }

    #[test]
    fn an_explicit_project_must_exist() {
        let root = tempfile::tempdir().unwrap();
        let home = home_with_app(root.path());

        assert_eq!(
            resolve_project(&home, Some(name("app"))).unwrap(),
            name("app")
        );
        let error = resolve_project(&home, Some(name("other"))).unwrap_err();
        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn finds_the_project_whose_folder_contains_the_directory() {
        let root = tempfile::tempdir().unwrap();
        let home = home_with_app(root.path());
        let inside = home.project_dir(&name("app")).join("workspace").join("src");
        fs::create_dir_all(&inside).unwrap();

        assert_eq!(project_containing(&home, &inside).unwrap(), name("app"));
    }

    #[test]
    fn finds_the_project_created_from_a_host_directory() {
        let root = tempfile::tempdir().unwrap();
        let home = home_with_app(root.path());
        let workspace = root.path().join("code").join("app");
        let inside = workspace.join("src");
        fs::create_dir_all(&inside).unwrap();
        // A literal TOML string, so a Windows path needs no escaping.
        let config = format!("workspace = '{}'\n", workspace.display());
        fs::write(home.project_config_file(&name("app")), config).unwrap();

        assert_eq!(project_containing(&home, &inside).unwrap(), name("app"));
    }

    #[test]
    fn fails_outside_every_project() {
        let root = tempfile::tempdir().unwrap();
        let home = home_with_app(root.path());
        let outside = root.path().join("elsewhere");
        fs::create_dir_all(&outside).unwrap();

        let error = project_containing(&home, &outside).unwrap_err();
        assert!(
            error.to_string().contains("not inside a casulo project"),
            "{error}"
        );
    }

    #[test]
    fn only_yes_confirms() {
        for (answer, expected) in [
            ("y\n", true),
            ("YES\n", true),
            ("\n", false),
            ("n\n", false),
            ("", false),
        ] {
            let mut input = answer.as_bytes();

            assert_eq!(ask(&mut input, "").unwrap(), expected, "{answer:?}");
        }
    }
}
