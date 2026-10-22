//! `casulo new`: creates a project from a repository or a host directory.

use anyhow::Result;

use super::home_and_config;
use crate::application::project::domain::GitSettings;
use crate::application::project::domain::push;
use crate::application::project::services::new_project;
use crate::application::sandbox::services::push as push_service;
use crate::cli::args::NewArgs;
use crate::cli::environment;
use crate::infrastructure::config_file::{self, ConfigFiles};
use crate::infrastructure::git::GitCli;

/// `casulo new`.
pub fn new(arguments: NewArgs) -> Result<()> {
    let (home, global) = home_and_config()?;
    let source = environment::project_source(&arguments.source)?;
    let created = new_project::create_project(
        &home,
        &source,
        arguments.name,
        &push::choose_identity(&GitSettings::default(), &global.git),
        &GitCli,
        &ConfigFiles,
    )?;
    if created.initialized_git {
        eprintln!(
            "{} had no git repository, so casulo ran git init there",
            created.workspace.display()
        );
    }
    for warning in &created.warnings {
        eprintln!("warning: {warning}");
    }
    let project_config =
        config_file::load_project_config(&home.project_config_file(&created.name))?;
    // The project exists by now, so a missing host is only a warning
    // here; `casulo run` refuses to push without one.
    match push::choose_push(
        &project_config.git,
        &global.git,
        project_config.repository.as_ref(),
    ) {
        Ok(Some(policy)) => {
            if push_service::take_reminder(&home, &created.name)? {
                eprintln!("warning: {}", push::push_reminder(&policy));
            }
        }
        Ok(None) => {}
        Err(error) => eprintln!("warning: {error}"),
    }
    eprintln!("created project \"{}\"", created.name);
    println!("{}", created.workspace.display());
    Ok(())
}
