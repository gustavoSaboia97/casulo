//! `casulo compose`: check the services of the workspace's compose file.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::application::project::domain::ComposeSettings;
use crate::application::sandbox::domain::compose::{
    ComposeModel, compose_project_name, has_include,
};
use crate::application::sandbox::ports::Compose;
use crate::application::shared::domain::{CasuloHome, ProjectName};

/// A compose model that passed validation, and the exact JSON it came from,
/// which is what will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedCompose {
    /// What the services do, for the user to review.
    pub model: ComposeModel,
    /// The normalized model printed by `docker compose config`.
    pub json: String,
}

/// Validates the compose file of `project`: rejects `include` before Docker
/// reads the file, then checks the normalized model against the allowlist.
pub fn check(
    project: &ProjectName,
    workspace: &Path,
    settings: &ComposeSettings,
    runtime: &dyn Compose,
) -> Result<CheckedCompose> {
    let file = settings.file.on_host(workspace);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => bail!(
            "no compose file at {}; set `file` under [compose] with `casulo config --project {project}`",
            file.display()
        ),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", file.display())),
    };
    if has_include(&text) {
        bail!(
            "{} uses include, which casulo does not allow: Docker would fetch and read other files before casulo could check them; copy the services into this file",
            file.display()
        );
    }
    let json = runtime.compose_config(&compose_project_name(project), workspace, &file)?;
    let model = ComposeModel::validate(&json)
        .with_context(|| format!("{} cannot be started by casulo", file.display()))?;
    Ok(CheckedCompose { model, json })
}

/// Whether the user still has to approve a checked model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    /// It is exactly what the user approved last time.
    UpToDate,
    /// Nothing was approved yet for this project.
    FirstTime,
    /// It differs from the approved one; the text says how.
    Changed(String),
}

/// Compares `checked` with the model the user approved for `project`.
pub fn approval(
    home: &CasuloHome,
    project: &ProjectName,
    checked: &CheckedCompose,
) -> Result<Approval> {
    let path = approved_file(home, project);
    let approved = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Approval::FirstTime);
        }
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    if approved == checked.json {
        return Ok(Approval::UpToDate);
    }
    // An approved file that no longer validates (casulo got stricter) is
    // shown in full, like a first approval.
    match ComposeModel::validate(&approved) {
        Ok(previous) => Ok(Approval::Changed(checked.model.changes_since(&previous))),
        Err(_) => Ok(Approval::FirstTime),
    }
}

/// Records that the user approved `checked`, outside the workspace, so the
/// agent cannot approve its own changes.
pub fn approve(home: &CasuloHome, project: &ProjectName, checked: &CheckedCompose) -> Result<()> {
    let path = approved_file(home, project);
    let directory = home.compose_dir(project);
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    fs::write(&path, &checked.json).with_context(|| format!("cannot write {}", path.display()))
}

/// Starts the approved services in the background and waits until they are
/// healthy. The project directory is casulo's own `compose/` directory, not
/// the workspace, so no file from the repository (such as `.env`) is read:
/// the model on stdin already has everything resolved.
pub fn start(
    home: &CasuloHome,
    project: &ProjectName,
    checked: &CheckedCompose,
    runtime: &dyn Compose,
) -> Result<()> {
    let directory = home.compose_dir(project);
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    runtime.compose_up(&compose_project_name(project), &directory, &checked.json)
}

/// A compose subcommand that needs no compose file: it works on the
/// services already started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeAction {
    /// Stop and remove the services.
    Down,
    /// Show the state of the services.
    Ps,
}

impl ComposeAction {
    /// The compose subcommand, as the runtime's command line names it.
    pub fn subcommand(self) -> &'static str {
        match self {
            ComposeAction::Down => "down",
            ComposeAction::Ps => "ps",
        }
    }
}

/// Runs `action` for the project's services.
pub fn run_command(
    home: &CasuloHome,
    project: &ProjectName,
    action: ComposeAction,
    runtime: &dyn Compose,
) -> Result<()> {
    let directory = home.compose_dir(project);
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    runtime.compose_command(
        &compose_project_name(project),
        &directory,
        action.subcommand(),
    )
}

fn approved_file(home: &CasuloHome, project: &ProjectName) -> std::path::PathBuf {
    home.compose_dir(project).join("approved.json")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;

    /// A runtime whose `compose config` prints a fixed model.
    struct FakeRuntime {
        json: String,
        calls: RefCell<Vec<(String, PathBuf, PathBuf)>>,
    }

    impl Compose for FakeRuntime {
        fn compose_config(
            &self,
            project_name: &str,
            workspace: &Path,
            file: &Path,
        ) -> Result<String> {
            self.calls.borrow_mut().push((
                project_name.to_string(),
                workspace.to_path_buf(),
                file.to_path_buf(),
            ));
            Ok(self.json.clone())
        }
        fn compose_up(
            &self,
            _project_name: &str,
            _project_directory: &Path,
            _json: &str,
        ) -> Result<()> {
            bail!("unused")
        }
        fn compose_command(
            &self,
            _project_name: &str,
            _project_directory: &Path,
            _command: &str,
        ) -> Result<()> {
            bail!("unused")
        }
    }

    fn runtime(json: &str) -> FakeRuntime {
        FakeRuntime {
            json: json.to_string(),
            calls: RefCell::new(Vec::new()),
        }
    }

    const REDIS: &str = r#"{"name":"casulo-app","networks":{"default":{"name":"casulo-app_default","ipam":{}}},"services":{"cache":{"image":"redis:7","networks":{"default":null}}}}"#;

    fn app() -> ProjectName {
        ProjectName::parse("app").unwrap()
    }

    #[test]
    fn checks_the_normalized_model_of_the_file() {
        let workspace = tempfile::tempdir().unwrap();
        let file = workspace.path().join("docker-compose.yml");
        fs::write(&file, "services:\n  cache:\n    image: redis:7\n").unwrap();
        let runtime = runtime(REDIS);

        let checked = check(
            &app(),
            workspace.path(),
            &ComposeSettings::default(),
            &runtime,
        )
        .unwrap();

        assert_eq!(checked.json, REDIS);
        assert_eq!(checked.model.services[0].image, "redis:7");
        assert_eq!(
            runtime.calls.borrow()[0],
            (
                "casulo-app".to_string(),
                workspace.path().to_path_buf(),
                file
            )
        );
    }

    fn checked(json: &str) -> CheckedCompose {
        CheckedCompose {
            model: ComposeModel::validate(json).unwrap(),
            json: json.to_string(),
        }
    }

    #[test]
    fn approval_is_asked_first_and_after_every_change() {
        let directory = tempfile::tempdir().unwrap();
        let home = CasuloHome::resolve(Some(directory.path().into()), None).unwrap();
        let redis = checked(REDIS);
        let redis_8 = checked(&REDIS.replace("redis:7", "redis:8"));

        assert_eq!(
            approval(&home, &app(), &redis).unwrap(),
            Approval::FirstTime
        );
        approve(&home, &app(), &redis).unwrap();
        assert_eq!(approval(&home, &app(), &redis).unwrap(), Approval::UpToDate);
        assert_eq!(
            approval(&home, &app(), &redis_8).unwrap(),
            Approval::Changed(
                "~ service cache\n-   image redis:7\n+   image redis:8\n".to_string()
            )
        );
    }

    #[test]
    fn nothing_is_stored_until_approved() {
        let directory = tempfile::tempdir().unwrap();
        let home = CasuloHome::resolve(Some(directory.path().into()), None).unwrap();

        approval(&home, &app(), &checked(REDIS)).unwrap();

        assert!(!home.compose_dir(&app()).exists());
    }

    #[test]
    fn rejects_include_before_docker_reads_the_file() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(
            workspace.path().join("docker-compose.yml"),
            "include:\n  - oci://example.com/evil:latest\n",
        )
        .unwrap();
        let runtime = runtime(REDIS);

        let error = check(
            &app(),
            workspace.path(),
            &ComposeSettings::default(),
            &runtime,
        )
        .unwrap_err();

        assert!(error.to_string().contains("uses include"));
        assert!(runtime.calls.borrow().is_empty());
    }

    #[test]
    fn missing_file_says_how_to_choose_another() {
        let workspace = tempfile::tempdir().unwrap();

        let error = check(
            &app(),
            workspace.path(),
            &ComposeSettings::default(),
            &runtime(REDIS),
        )
        .unwrap_err();

        assert!(error.to_string().contains("casulo config --project app"));
    }

    #[test]
    fn rejected_model_names_the_file() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(
            workspace.path().join("docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        let privileged = r#"{"services":{"x":{"image":"alpine","privileged":true}}}"#;

        let error = check(
            &app(),
            workspace.path(),
            &ComposeSettings::default(),
            &runtime(privileged),
        )
        .unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains("docker-compose.yml cannot be started by casulo"));
        assert!(message.contains("privileged is not allowed"));
    }

    #[test]
    fn actions_name_the_compose_subcommands() {
        assert_eq!(ComposeAction::Down.subcommand(), "down");
        assert_eq!(ComposeAction::Ps.subcommand(), "ps");
    }
}
