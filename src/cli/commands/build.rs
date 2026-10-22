//! `casulo build`: generates the project's Dockerfile and builds the image,
//! and the memory image when memory is on.

use anyhow::Result;

use super::project_memory::build_memory_image;
use super::{home_and_config, runtime};
use crate::application::agent::domain::Harness;
use crate::application::project::domain::GlobalConfig;
use crate::application::project::domain::config::{choose_harness, memory_enabled};
use crate::application::sandbox::services::build;
use crate::application::shared::domain::{CasuloHome, ProjectName};
use crate::cli::args::ProjectArgs;
use crate::cli::environment;
use crate::infrastructure::config_file;
use crate::infrastructure::harness_files;

/// `casulo build`.
pub fn build(arguments: ProjectArgs) -> Result<()> {
    let (home, global) = home_and_config()?;
    let (project, harness) = project_and_harness(&home, &global, arguments)?;
    let runtime = runtime(&global);
    let project_config = config_file::load_project_config(&home.project_config_file(&project))?;
    let tag = build::build_image(&home, &project, &harness, &project_config.image, &runtime)?;
    eprintln!("built {tag}");
    println!("{tag}");
    if memory_enabled(&project_config.memory, &global.memory) {
        let tag = build_memory_image(&home, &runtime)?;
        eprintln!("built {tag}");
        println!("{tag}");
    }
    Ok(())
}

fn project_and_harness(
    home: &CasuloHome,
    global: &GlobalConfig,
    arguments: ProjectArgs,
) -> Result<(ProjectName, Harness)> {
    let project = environment::resolve_project(home, arguments.project)?;
    let project_config = config_file::load_project_config(&home.project_config_file(&project))?;
    let harness_name = choose_harness(arguments.harness, &project_config, global);
    let harness = harness_files::find_harness(&home.harnesses_dir(), &harness_name)?;
    Ok((project, harness))
}
