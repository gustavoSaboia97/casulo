//! `casulo describe memory`: what the project's memory holds, or the
//! details of one file.

use anyhow::{Result, bail};

use super::project_memory::NO_MEMORY_YET;
use crate::cli::args::{DescribeMemoryArgs, MemoryArgs, MemoryResource};
use crate::cli::environment;
use crate::cli::output;

use crate::application::memory;

/// `casulo describe memory [note <path>]`.
pub fn describe(arguments: DescribeMemoryArgs) -> Result<()> {
    match arguments.resource {
        None => describe_memory(arguments.memory),
        Some(MemoryResource::Note { path }) => describe_memory_note(arguments.memory, &path),
    }
}

/// `casulo describe memory`: counts of what the project's memory holds.
fn describe_memory(arguments: MemoryArgs) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let project = environment::resolve_project(&home, arguments.project)?;
    let database_file = home.memory_database_file(&project);
    println!("project        {project}");
    if !database_file.exists() {
        println!("memory         not created yet");
        return Ok(());
    }
    let database = crate::infrastructure::sqlite::Database::open(&database_file)?;
    let state = memory::services::state::memory_state(&database)?;
    println!("files          {}", state.files);
    println!("links          {}", state.edges);
    println!(
        "file notes     {} ({} ok, {} stale, {} in conflict)",
        state.file_notes, state.ok, state.stale, state.conflicts
    );
    println!("project notes  {}", state.project_notes);
    println!("events         {}", state.events);
    println!(
        "sessions       {} ({} with changes, {} with notes, {} blocked)",
        state.sessions,
        state.sessions_changing_files,
        state.sessions_saving_notes,
        state.sessions_blocked
    );
    Ok(())
}

/// `casulo describe memory note <path>`: a file's neighbors and notes.
fn describe_memory_note(arguments: MemoryArgs, path: &str) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let project = environment::resolve_project(&home, arguments.project)?;
    let path = memory::domain::RepoPath::parse(path)?;
    let database_file = home.memory_database_file(&project);
    if !database_file.exists() {
        bail!("project \"{project}\" {NO_MEMORY_YET}");
    }
    let database = crate::infrastructure::sqlite::Database::open(&database_file)?;
    let details = memory::services::file_details::file_details(&database, &path)?;
    print!("{}", output::memory::format_file_details(&details));
    Ok(())
}
