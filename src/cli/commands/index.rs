//! `casulo index memory`: reads what changed in the checkout into the
//! project's memory.

use anyhow::{Context, Result};

use super::project_memory::refuse_while_served;
use crate::cli::args::MemoryArgs;
use crate::cli::environment;
use crate::infrastructure::config_file;

use crate::application::memory;

/// `casulo index memory`: reads what changed in the checkout into the
/// project's memory, creating it the first time.
pub fn index(arguments: MemoryArgs) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let project = environment::resolve_project(&home, arguments.project)?;
    refuse_while_served(&home, &project)?;
    let project_config = config_file::load_project_config(&home.project_config_file(&project))?;
    let workspace = project_config.workspace_dir(&home, &project);
    let memory_dir = home.memory_dir(&project);
    std::fs::create_dir_all(&memory_dir)
        .with_context(|| format!("cannot create {}", memory_dir.display()))?;
    let mut database =
        crate::infrastructure::sqlite::Database::open(&home.memory_database_file(&project))?;
    let report = memory::services::indexer::index(
        &workspace,
        &crate::infrastructure::git::GitCli,
        &crate::infrastructure::tree_sitter_rust::TreeSitterRust,
        &mut database,
        &memory::ports::Clock::now(&crate::infrastructure::clock::SystemClock),
    )?;
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    eprintln!(
        "indexed {} files ({} parsed), removed {}{}",
        report.indexed,
        report.parsed,
        report.removed,
        if report.branches_changed {
            "; branches updated"
        } else {
            ""
        }
    );
    Ok(())
}
