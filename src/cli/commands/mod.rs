//! One handler per command: each loads the configuration, builds the
//! adapters, calls the services and prints the result.

mod add;
mod build;
mod compose;
mod config;
mod delete;
mod describe;
mod doctor;
mod edit;
mod harness;
mod home;
mod index;
mod list;
mod memory;
mod new;
mod project_memory;
mod promote;
mod run;
mod search;
mod setup;
mod stop;

use std::process::ExitCode;

use anyhow::Result;

use crate::application::project::domain::GlobalConfig;
use crate::application::shared::domain::CasuloHome;
use crate::cli::args::{
    Cli, Command, DescribeCommand, HarnessCommand, IndexCommand, MemoryCommand, PromoteCommand,
    SearchCommand, StopCommand,
};
use crate::cli::environment;
use crate::infrastructure::config_file;
use crate::infrastructure::docker::Docker;

/// Runs the command chosen on the command line, and returns the exit code
/// for the process.
pub fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Doctor => doctor::doctor()?,
        Command::Setup => setup::setup()?,
        Command::New(arguments) => new::new(arguments)?,
        Command::Build(arguments) => build::build(arguments)?,
        Command::Run(arguments) => return run::start_container(arguments, None),
        Command::Shell(arguments) => {
            return run::start_container(arguments, Some(vec!["bash".to_string()]));
        }
        Command::Harness(HarnessCommand::List) => harness::list()?,
        Command::Compose(command) => compose::compose(command)?,
        Command::Describe(DescribeCommand::Memory(arguments)) => describe::describe(arguments)?,
        Command::Index(IndexCommand::Memory(arguments)) => index::index(arguments)?,
        Command::Memory(MemoryCommand::Serve(arguments)) => memory::serve(arguments)?,
        Command::Stop(StopCommand::Memory(arguments)) => stop::stop(arguments)?,
        Command::Search(SearchCommand::Memory(arguments)) => search::search(arguments)?,
        Command::Config(arguments) => config::config(arguments)?,
        Command::Add(arguments) => add::add(arguments)?,
        Command::Edit(arguments) => edit::edit(arguments)?,
        Command::List(arguments) => list::list(arguments)?,
        Command::Promote(PromoteCommand::Memory(arguments)) => promote::promote(arguments)?,
        Command::Delete(arguments) => delete::delete(arguments)?,
        Command::Home => home::home()?,
    }
    Ok(ExitCode::SUCCESS)
}

/// The casulo home and its global configuration.
fn home_and_config() -> Result<(CasuloHome, GlobalConfig)> {
    let home = environment::casulo_home_from_environment()?;
    let path = home.global_config_file();
    let global = config_file::load_global_config(&path)?;
    // A file created by an older casulo, or copied in, may be readable by
    // every user of the machine; say so while it holds secrets.
    warn_if_others_can_read(
        &path,
        !global.secrets.is_empty() || global.git.token.is_some(),
    );
    Ok((home, global))
}

/// Warns when `path` holds secrets and other users of this machine can read
/// it, as a file created by an older casulo, or copied in, may be.
fn warn_if_others_can_read(path: &std::path::Path, holds_secrets: bool) {
    if holds_secrets && config_file::readable_by_others(path).unwrap_or(false) {
        eprintln!(
            "warning: {} holds secrets and other users of this machine can read it; run `chmod 600 {}`",
            path.display(),
            path.display()
        );
    }
}

/// The container runtime the global configuration chooses.
fn runtime(global: &GlobalConfig) -> Docker {
    Docker::new(&global.runtime)
}

/// The `editor` key of the global config. It is read even from a broken
/// file, and an unreadable file is only a warning, so the editor still opens.
fn configured_editor(home: &CasuloHome) -> Option<String> {
    match config_file::load_configured_editor(&home.global_config_file()) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("warning: {error:#}, so its `editor` key is ignored");
            None
        }
    }
}
