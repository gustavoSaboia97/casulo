//! What the memory commands share: opening the project's memory, keeping
//! it to one writer, and building the memory image.

use anyhow::{Context, Result, bail};

use super::runtime;
use crate::application::memory::services::note_editing;
use crate::application::sandbox::ports::{Containers, Images};
use crate::application::sandbox::services::memory_container as memory_service;
use crate::application::shared::domain::{CasuloHome, ProjectName};
use crate::cli::args::MemoryArgs;
use crate::cli::environment;
use crate::infrastructure::config_file;

use crate::application::memory;

/// What follows `project "<name>"` when its memory was never created.
pub(super) const NO_MEMORY_YET: &str = "has no memory yet; create it with `casulo index memory`";

/// An open project memory, for the commands that read or change it.
pub(super) struct OpenedMemory {
    pub(super) home: CasuloHome,
    pub(super) project: ProjectName,
    pub(super) memory: note_editing::OpenMemory,
}

/// Opens the memory of the project chosen by `arguments`. Fails when it
/// was never created, saying how to create it, and, when `write` is set,
/// while a session's memory server is using it.
pub(super) fn open_memory(arguments: MemoryArgs, write: bool) -> Result<OpenedMemory> {
    let home = environment::casulo_home_from_environment()?;
    let project = environment::resolve_project(&home, arguments.project)?;
    if write {
        refuse_while_served(&home, &project)?;
    }
    let project_config = config_file::load_project_config(&home.project_config_file(&project))?;
    let workspace = project_config.workspace_dir(&home, &project);
    let database_file = home.memory_database_file(&project);
    if !database_file.exists() {
        bail!("project \"{project}\" {NO_MEMORY_YET}");
    }
    let database = crate::infrastructure::sqlite::Database::open(&database_file)?;
    let notes_dir = home.memory_notes_dir(&project);
    Ok(OpenedMemory {
        home,
        project,
        memory: note_editing::OpenMemory {
            database: Box::new(database),
            workspace,
            notes_dir,
        },
    })
}

/// Fails while the memory container of `project` runs: it is then the only
/// process that may write the memory, so SQLite is never written by two.
pub(super) fn refuse_while_served(home: &CasuloHome, project: &ProjectName) -> Result<()> {
    let global = config_file::load_global_config(&home.global_config_file())?;
    let runtime = runtime(&global);
    // Without a runtime answering, no memory container can be running.
    if runtime
        .container_running(&memory_service::memory_name(project))
        .unwrap_or(false)
    {
        bail!(
            "the memory of \"{project}\" is in use by a running session; try again when it ends, or stop it with `casulo stop memory`"
        );
    }
    Ok(())
}

/// Where the memory image gets casulo from. A release build has the
/// checksums of its Linux binaries embedded by the release workflow; any
/// other build compiles casulo from the source it was built from.
pub(super) fn memory_image_source() -> memory_service::ImageSource {
    match (
        option_env!("CASULO_MEMORY_SHA256_X86_64"),
        option_env!("CASULO_MEMORY_SHA256_AARCH64"),
    ) {
        (Some(sha256_x86_64), Some(sha256_aarch64)) => memory_service::ImageSource::Release {
            version: env!("CARGO_PKG_VERSION").to_string(),
            sha256_x86_64: sha256_x86_64.to_string(),
            sha256_aarch64: sha256_aarch64.to_string(),
        },
        _ => memory_service::ImageSource::Source {
            source_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        },
    }
}

/// Builds the memory image, checking first that a development build still
/// has its source.
pub(super) fn build_memory_image(home: &CasuloHome, runtime: &dyn Images) -> Result<String> {
    let source = memory_image_source();
    if let memory_service::ImageSource::Source { source_dir } = &source
        && !source_dir.join("Cargo.toml").is_file()
    {
        bail!(
            "this casulo was built from source at {}, which is gone, so the memory image cannot be built; install a released casulo or build casulo again",
            source_dir.display()
        );
    }
    memory_service::build_image(home, &source, runtime)
}

/// The current branch of the workspace; fails on a detached checkout.
pub(super) fn current_branch(workspace: &std::path::Path) -> Result<String> {
    memory::ports::Git::current_branch(&crate::infrastructure::git::GitCli, workspace)?
        .context("the checkout is not on a branch; check out a branch to use branch notes")
}
