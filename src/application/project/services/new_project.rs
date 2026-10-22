//! `casulo new`: create a project from a repository to clone, or from a
//! directory on the host used in place.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::application::project::domain::config::default_mask;
use crate::application::project::domain::{GitHost, GitIdentity, ProjectSource};
use crate::application::project::ports::{ConfigStore, Git};
use crate::application::shared::domain::{CasuloHome, ProjectName, WorkspacePath};

/// What `casulo new` did, for the CLI to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    /// The project's name.
    pub name: ProjectName,
    /// The workspace on the host.
    pub workspace: PathBuf,
    /// Whether casulo ran `git init` in a directory that had no repository.
    pub initialized_git: bool,
    /// Problems with the directory's git setup, each with the command that
    /// fixes it. Empty for a cloned repository.
    pub warnings: Vec<String>,
}

/// Creates `projects/<name>/` and its config. A repository is cloned into
/// its `workspace/`; a directory, which must be absolute, is used in place
/// and gets `git init` when it has no repository. The name defaults to the
/// repository or directory name.
///
/// Fails if the project already exists. On failure only what casulo created
/// under `projects/` is removed, never the user's directory.
pub fn create_project(
    home: &CasuloHome,
    source: &ProjectSource,
    name: Option<ProjectName>,
    identity: &GitIdentity,
    git: &dyn Git,
    config_store: &dyn ConfigStore,
) -> Result<NewProject> {
    let source = match source {
        ProjectSource::Repository(_) => source.clone(),
        ProjectSource::Directory(directory) => {
            ProjectSource::Directory(prepare_directory(directory)?)
        }
    };
    let name = match name {
        Some(name) => name,
        None => suggested_name(&source)?,
    };
    let project_dir = home.project_dir(&name);
    if let Some(projects_dir) = project_dir.parent() {
        fs::create_dir_all(projects_dir)
            .with_context(|| format!("cannot create {}", projects_dir.display()))?;
    }
    // `create_dir` (not `create_dir_all`) fails when the directory exists, so
    // two runs can never share or overwrite a project.
    match fs::create_dir(&project_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => bail!(
            "project \"{name}\" already exists in {}; choose another name with --name",
            project_dir.display()
        ),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot create {}", project_dir.display()));
        }
    }

    let result = fill_project(home, &source, &name, identity, git, config_store);
    if result.is_err() {
        // Best effort: the original error matters more than a failed cleanup.
        let _ = fs::remove_dir_all(&project_dir);
    }
    result
}

fn fill_project(
    home: &CasuloHome,
    source: &ProjectSource,
    name: &ProjectName,
    identity: &GitIdentity,
    git: &dyn Git,
    config_store: &dyn ConfigStore,
) -> Result<NewProject> {
    let path = home.project_config_file(name);
    match source {
        ProjectSource::Repository(url) => {
            let workspace = home.workspace_dir(name);
            git.clone_repository(url, &workspace)?;
            write_project_config(config_store, &path, source, &default_mask(), None)?;
            Ok(NewProject {
                name: name.clone(),
                workspace,
                initialized_git: false,
                warnings: Vec::new(),
            })
        }
        ProjectSource::Directory(directory) => {
            let initialized_git = !directory.join(".git").exists();
            if initialized_git {
                git.init(directory)?;
            }
            // Read now, before any agent runs here: later the agent can edit
            // the repository's config and point the remote anywhere.
            let host = remote_host(directory, git)?;
            write_project_config(
                config_store,
                &path,
                source,
                &directory_mask(),
                host.as_ref(),
            )?;
            Ok(NewProject {
                name: name.clone(),
                workspace: directory.clone(),
                initialized_git,
                warnings: git_setup_warnings(directory, identity, git)?,
            })
        }
    }
}

/// The host of the remote the agent would push to (`origin`, else the first
/// one), if there is a remote and its URL has a host.
fn remote_host(directory: &Path, git: &dyn Git) -> Result<Option<GitHost>> {
    let remotes = git.remotes(directory)?;
    let Some(remote) = default_remote(&remotes) else {
        return Ok(None);
    };
    let url = git.remote_url(directory, remote)?;
    Ok(url.as_deref().and_then(GitHost::from_url))
}

/// `origin` when the repository has it, else its first remote.
fn default_remote(remotes: &[String]) -> Option<&String> {
    remotes
        .iter()
        .find(|remote| remote.as_str() == "origin")
        .or(remotes.first())
}

/// Writes a new project's `casulo.toml`, failing when one is already there.
fn write_project_config(
    config_store: &dyn ConfigStore,
    path: &Path,
    source: &ProjectSource,
    mask: &[WorkspacePath],
    host: Option<&GitHost>,
) -> Result<()> {
    if !config_store.create_project_config(path, source, mask, host)? {
        bail!(
            "{} already exists; choose another name with --name",
            path.display()
        );
    }
    Ok(())
}

/// The default mask, plus `.git/hooks`: with the user's real directory
/// mounted, a hook written by the agent would run on the host the next time
/// the user runs git there.
pub fn directory_mask() -> Vec<WorkspacePath> {
    let mut mask = default_mask();
    mask.push(WorkspacePath::parse(".git/hooks").expect("the hooks path is a valid path"));
    mask
}

/// Creates the directory if needed and returns its canonical path, so the
/// config holds one stable absolute path.
fn prepare_directory(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let canonical = fs::canonicalize(directory)
        .with_context(|| format!("cannot read {}", directory.display()))?;
    Ok(without_verbatim_prefix(canonical))
}

/// On Windows, `canonicalize` returns `\\?\C:\...`, which other programs
/// such as docker do not accept; the plain `C:\...` means the same.
#[cfg(windows)]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.chars().nth(1) == Some(':') => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(not(windows))]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

fn suggested_name(source: &ProjectSource) -> Result<ProjectName> {
    match source {
        ProjectSource::Repository(url) => url.suggested_project_name(),
        ProjectSource::Directory(directory) => directory
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(ProjectName::slug_of)
            .with_context(|| {
                format!(
                    "cannot make a project name from {}; choose one with --name",
                    directory.display()
                )
            }),
    }
}

/// Checks how the agent's commits in `directory` would leave the machine,
/// and returns a warning with the fixing command for each gap. A
/// local-only project is valid, so none of these is an error. Nothing here
/// touches the network.
pub fn git_setup_warnings(
    directory: &Path,
    identity: &GitIdentity,
    git: &dyn Git,
) -> Result<Vec<String>> {
    let shown = directory.display();
    let mut warnings = Vec::new();
    let remotes = git.remotes(directory)?;
    let branch = git.current_branch(directory)?;
    let upstream = git.upstream(directory)?;
    let tracked_remote = upstream.as_deref().and_then(|upstream| {
        remotes
            .iter()
            .find(|remote| upstream.starts_with(&format!("{remote}/")))
    });
    let default_remote = default_remote(&remotes);

    match (remotes.len(), tracked_remote, default_remote) {
        (0, _, _) => warnings.push(format!(
            "no remote: the agent's commits stay on this machine. To add one: git -C {shown} remote add origin <url>"
        )),
        (1, _, _) => {}
        (_, Some(tracked), _) => warnings.push(format!(
            "several remotes ({}); the current branch tracks {tracked}",
            remotes.join(", ")
        )),
        (_, None, Some(default)) => warnings.push(format!(
            "several remotes ({}) and the current branch tracks none; {default} would be used",
            remotes.join(", ")
        )),
        (_, None, None) => {}
    }
    if let (Some(branch), None, Some(remote)) = (&branch, &upstream, default_remote) {
        warnings.push(format!(
            "branch {branch} has no upstream. After the first commit: git -C {shown} push -u {remote} {branch}"
        ));
    }
    if identity.name.is_none() || identity.email.is_none() {
        warnings.push(
            "commits made by the agent will have no author. Set [git] name and email with `casulo config`"
                .to_string(),
        );
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;

    use super::*;
    use crate::application::project::domain::RepositoryUrl;
    use crate::infrastructure::config_file::{self, ConfigFiles};

    /// Records calls and answers like git would, or fails to clone.
    struct FakeGit {
        fail: bool,
        remotes: Vec<String>,
        branch: Option<String>,
        upstream: Option<String>,
        // `RefCell` lets a method that takes `&self` still record calls, like
        // mutating a list attribute on a Python object.
        clones: RefCell<Vec<(String, PathBuf)>>,
        inits: RefCell<Vec<PathBuf>>,
    }

    impl FakeGit {
        fn new(fail: bool) -> Self {
            Self {
                fail,
                remotes: Vec::new(),
                branch: Some("main".to_string()),
                upstream: None,
                clones: RefCell::new(Vec::new()),
                inits: RefCell::new(Vec::new()),
            }
        }

        fn with_remotes(remotes: &[&str], upstream: Option<&str>) -> Self {
            Self {
                remotes: remotes.iter().map(|remote| remote.to_string()).collect(),
                upstream: upstream.map(str::to_string),
                ..Self::new(false)
            }
        }
    }

    impl Git for FakeGit {
        fn clone_repository(&self, url: &RepositoryUrl, destination: &Path) -> Result<()> {
            self.clones
                .borrow_mut()
                .push((url.to_string(), destination.to_path_buf()));
            if self.fail {
                bail!("git clone failed");
            }
            fs::create_dir_all(destination.join(".git"))?;
            Ok(())
        }

        fn init(&self, directory: &Path) -> Result<()> {
            self.inits.borrow_mut().push(directory.to_path_buf());
            if self.fail {
                bail!("git init failed");
            }
            fs::create_dir_all(directory.join(".git"))?;
            Ok(())
        }

        fn remotes(&self, _directory: &Path) -> Result<Vec<String>> {
            Ok(self.remotes.clone())
        }

        fn remote_url(&self, _directory: &Path, remote: &str) -> Result<Option<String>> {
            // Each remote gets its own host, so tests see which one was read.
            Ok(self
                .remotes
                .iter()
                .any(|known| known == remote)
                .then(|| format!("git@{remote}.example.com:you/app.git")))
        }

        fn current_branch(&self, _directory: &Path) -> Result<Option<String>> {
            Ok(self.branch.clone())
        }

        fn upstream(&self, _directory: &Path) -> Result<Option<String>> {
            Ok(self.upstream.clone())
        }
    }

    fn home(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path().join("casulo"))), None).unwrap()
    }

    fn url() -> RepositoryUrl {
        RepositoryUrl::parse("https://github.com/you/My_Project.git").unwrap()
    }

    fn repository() -> ProjectSource {
        ProjectSource::Repository(url())
    }

    fn identity() -> GitIdentity {
        GitIdentity {
            name: Some("Ada".to_string()),
            email: Some("ada@example.com".to_string()),
        }
    }

    #[test]
    fn clones_into_workspace_and_writes_config() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let git = FakeGit::new(false);

        let created =
            create_project(&home, &repository(), None, &identity(), &git, &ConfigFiles).unwrap();

        assert_eq!(created.name.as_str(), "my-project");
        assert_eq!(created.workspace, home.workspace_dir(&created.name));
        assert_eq!(
            git.clones.borrow().as_slice(),
            [(url().to_string(), home.workspace_dir(&created.name))]
        );
        let config =
            config_file::load_project_config(&home.project_config_file(&created.name)).unwrap();
        assert_eq!(config.repository, Some(url()));
        assert_eq!(config.harness, None);
        assert_eq!(config.mask, default_mask());
        assert!(created.warnings.is_empty());
    }

    #[test]
    fn uses_the_given_name() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let chosen = ProjectName::parse("chosen").unwrap();

        let created = create_project(
            &home,
            &repository(),
            Some(chosen.clone()),
            &identity(),
            &FakeGit::new(false),
            &ConfigFiles,
        )
        .unwrap();

        assert_eq!(created.name, chosen);
        assert!(home.workspace_dir(&chosen).exists());
    }

    #[test]
    fn fails_when_project_exists_without_touching_it() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let git = FakeGit::new(false);
        create_project(&home, &repository(), None, &identity(), &git, &ConfigFiles).unwrap();

        let error = create_project(&home, &repository(), None, &identity(), &git, &ConfigFiles)
            .unwrap_err();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(git.clones.borrow().len(), 1);
        let name = ProjectName::parse("my-project").unwrap();
        assert!(home.project_config_file(&name).exists());
    }

    #[test]
    fn failed_clone_leaves_nothing_behind() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);

        let error = create_project(
            &home,
            &repository(),
            None,
            &identity(),
            &FakeGit::new(true),
            &ConfigFiles,
        )
        .unwrap_err();

        assert!(error.to_string().contains("git clone failed"));
        let name = ProjectName::parse("my-project").unwrap();
        assert!(!home.project_dir(&name).exists());
    }

    #[test]
    fn directory_is_used_in_place_and_initialized() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let user_dir = directory.path().join("My App");
        let git = FakeGit::with_remotes(&["origin"], Some("origin/main"));

        let created = create_project(
            &home,
            &ProjectSource::Directory(user_dir.clone()),
            None,
            &identity(),
            &git,
            &ConfigFiles,
        )
        .unwrap();

        let canonical = without_verbatim_prefix(fs::canonicalize(&user_dir).unwrap());
        assert_eq!(created.name.as_str(), "my-app");
        assert_eq!(created.workspace, canonical);
        assert!(created.initialized_git);
        assert_eq!(git.inits.borrow()[0], canonical);
        assert!(git.clones.borrow().is_empty());
        assert!(created.warnings.is_empty());
        let config =
            config_file::load_project_config(&home.project_config_file(&created.name)).unwrap();
        assert_eq!(config.workspace, Some(canonical));
        assert_eq!(config.repository, None);
        assert_eq!(config.mask, directory_mask());
        assert_eq!(
            config.git.host,
            Some(GitHost::parse("origin.example.com").unwrap())
        );
        assert!(!home.workspace_dir(&created.name).exists());
    }

    #[test]
    fn the_push_host_comes_from_origin_else_the_first_remote() {
        let host_for = |remotes: &[&str]| {
            let directory = tempfile::tempdir().unwrap();
            let home = home(&directory);
            let git = FakeGit::with_remotes(remotes, None);
            let created = create_project(
                &home,
                &ProjectSource::Directory(directory.path().join("app")),
                None,
                &identity(),
                &git,
                &ConfigFiles,
            )
            .unwrap();
            config_file::load_project_config(&home.project_config_file(&created.name))
                .unwrap()
                .git
                .host
                .map(|host| host.to_string())
        };

        assert_eq!(
            host_for(&["upstream", "origin"]).as_deref(),
            Some("origin.example.com")
        );
        assert_eq!(
            host_for(&["upstream", "fork"]).as_deref(),
            Some("upstream.example.com")
        );
        assert_eq!(host_for(&[]), None);
    }

    #[test]
    fn existing_repository_is_not_initialized_again() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let user_dir = directory.path().join("app");
        fs::create_dir_all(user_dir.join(".git")).unwrap();
        let git = FakeGit::with_remotes(&["origin"], Some("origin/main"));

        let created = create_project(
            &home,
            &ProjectSource::Directory(user_dir),
            None,
            &identity(),
            &git,
            &ConfigFiles,
        )
        .unwrap();

        assert!(!created.initialized_git);
        assert!(git.inits.borrow().is_empty());
    }

    #[test]
    fn failure_never_removes_the_users_directory() {
        let directory = tempfile::tempdir().unwrap();
        let home = home(&directory);
        let user_dir = directory.path().join("app");
        fs::create_dir_all(&user_dir).unwrap();
        fs::write(user_dir.join("notes.txt"), "mine").unwrap();

        let error = create_project(
            &home,
            &ProjectSource::Directory(user_dir.clone()),
            None,
            &identity(),
            &FakeGit::new(true),
            &ConfigFiles,
        )
        .unwrap_err();

        assert!(error.to_string().contains("git init failed"));
        assert!(user_dir.join("notes.txt").is_file());
        assert!(
            !home
                .project_dir(&ProjectName::parse("app").unwrap())
                .exists()
        );
    }

    fn warnings_for(git: &FakeGit, identity: &GitIdentity) -> Vec<String> {
        git_setup_warnings(Path::new("/code/app"), identity, git).unwrap()
    }

    #[test]
    fn warns_about_a_missing_remote() {
        let warnings = warnings_for(&FakeGit::with_remotes(&[], None), &identity());

        assert_eq!(
            warnings,
            [
                "no remote: the agent's commits stay on this machine. To add one: git -C /code/app remote add origin <url>"
            ]
        );
    }

    #[test]
    fn warns_about_a_branch_without_upstream() {
        let warnings = warnings_for(&FakeGit::with_remotes(&["origin"], None), &identity());

        assert_eq!(
            warnings,
            [
                "branch main has no upstream. After the first commit: git -C /code/app push -u origin main"
            ]
        );
    }

    #[test]
    fn says_which_of_several_remotes_is_used() {
        let tracked = warnings_for(
            &FakeGit::with_remotes(&["fork", "upstream"], Some("upstream/main")),
            &identity(),
        );
        let untracked = warnings_for(
            &FakeGit::with_remotes(&["fork", "origin"], None),
            &identity(),
        );

        assert_eq!(
            tracked,
            ["several remotes (fork, upstream); the current branch tracks upstream"]
        );
        assert_eq!(
            untracked[0],
            "several remotes (fork, origin) and the current branch tracks none; origin would be used"
        );
        assert!(untracked[1].ends_with("push -u origin main"));
    }

    #[test]
    fn warns_about_a_missing_identity() {
        let git = FakeGit::with_remotes(&["origin"], Some("origin/main"));

        let warnings = warnings_for(&git, &GitIdentity::default());

        assert_eq!(
            warnings,
            [
                "commits made by the agent will have no author. Set [git] name and email with `casulo config`"
            ]
        );
    }
}
