//! `casulo stop memory`: stops the project's memory server now.

use anyhow::Result;

use super::{home_and_config, runtime};
use crate::application::sandbox::ports::Containers;
use crate::application::sandbox::services::memory_container as memory_service;
use crate::cli::args::MemoryArgs;
use crate::cli::environment;

/// `casulo stop memory`: stops the project's memory server now.
pub fn stop(arguments: MemoryArgs) -> Result<()> {
    let (home, global) = home_and_config()?;
    let project = environment::resolve_project(&home, arguments.project)?;
    let runtime = runtime(&global);
    let name = memory_service::memory_name(&project);
    if !runtime.container_running(&name)? {
        eprintln!("the memory of \"{project}\" is not running");
        return Ok(());
    }
    runtime.stop_container(&name)?;
    eprintln!("stopped the memory of \"{project}\"");
    Ok(())
}
