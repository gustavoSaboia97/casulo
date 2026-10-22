//! `casulo search memory`: searches the project's memory as agents do.

use anyhow::Result;

use super::project_memory::open_memory;
use crate::cli::args::SearchMemoryArgs;

use crate::application::memory;

/// `casulo search memory`.
pub fn search(arguments: SearchMemoryArgs) -> Result<()> {
    let opened = open_memory(arguments.memory, false)?;
    let branch = memory::ports::Git::current_branch(
        &crate::infrastructure::git::GitCli,
        &opened.memory.workspace,
    )?;
    let result = memory::services::search::search(
        &*opened.memory.database,
        &arguments.text,
        arguments.limit,
        branch.as_deref(),
    )?;
    print!("{}", memory::services::search::render(&result));
    Ok(())
}
