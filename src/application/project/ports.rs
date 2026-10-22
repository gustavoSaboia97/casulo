//! Traits for the outside world used by the project group, implemented in
//! `crate::infrastructure` and replaced by fakes in tests.

use std::path::Path;

use anyhow::Result;

use crate::application::project::domain::{
    GitHost, GlobalConfig, ProjectConfig, ProjectSource, RepositoryUrl,
};
use crate::application::shared::domain::WorkspacePath;

/// The git command-line tool.
pub trait Git {
    /// Clones `url` into `destination`, which must not exist yet, with the
    /// user's own git credentials.
    fn clone_repository(&self, url: &RepositoryUrl, destination: &Path) -> Result<()>;

    /// Creates an empty repository in `directory`.
    fn init(&self, directory: &Path) -> Result<()>;

    /// Names of the remotes of the repository in `directory`.
    fn remotes(&self, directory: &Path) -> Result<Vec<String>>;

    /// The URL of `remote` in the repository in `directory`, if it has one.
    fn remote_url(&self, directory: &Path, remote: &str) -> Result<Option<String>>;

    /// The checked-out branch, or `None` when no branch is checked out.
    fn current_branch(&self, directory: &Path) -> Result<Option<String>>;

    /// The upstream of the current branch, such as `origin/main`, if it has one.
    fn upstream(&self, directory: &Path) -> Result<Option<String>>;
}

/// The `casulo.toml` files, and the other files casulo creates once in its
/// home. casulo never rewrites a file it created, which keeps the user's
/// comments and edits.
pub trait ConfigStore {
    /// Writes the global `casulo.toml` at `path`, every option commented out
    /// with its default, only if no file is there yet. On Linux and macOS
    /// only its owner can read it, since it may hold secrets. Returns
    /// whether it was created.
    fn create_global_config(&self, path: &Path) -> Result<bool>;

    /// Writes a new project's `casulo.toml` at `path`, only if no file is
    /// there yet: its source and `mask`, the push `host` read from a
    /// directory's remote, and every other option commented out. On Linux
    /// and macOS only its owner can read it, since `[git]` may hold a push
    /// token. Returns whether it was created.
    fn create_project_config(
        &self,
        path: &Path,
        source: &ProjectSource,
        mask: &[WorkspacePath],
        host: Option<&GitHost>,
    ) -> Result<bool>;

    /// Writes `text` to `path` only if no file is there yet, such as the
    /// agent instructions. Returns whether it was created.
    fn create_if_missing(&self, path: &Path, text: &str) -> Result<bool>;

    /// Loads the global configuration, or the defaults when the file does
    /// not exist.
    fn load_global_config(&self, path: &Path) -> Result<GlobalConfig>;

    /// Loads the configuration of a project. The file must exist.
    fn load_project_config(&self, path: &Path) -> Result<ProjectConfig>;
}
