//! `casulo compose`: checks, approves, starts and stops the services of the
//! project's compose file.

use anyhow::{Result, bail};

use super::{home_and_config, runtime};
use crate::application::sandbox::ports::Compose;
use crate::application::sandbox::services::compose as compose_service;
use crate::application::sandbox::services::compose::{Approval, CheckedCompose, ComposeAction};
use crate::application::shared::domain::{CasuloHome, ProjectName};
use crate::cli::args::{ComposeArgs, ComposeCommand};
use crate::cli::environment;
use crate::infrastructure::config_file;

/// `casulo compose check|up|down|ps`.
pub fn compose(command: ComposeCommand) -> Result<()> {
    let (home, global) = home_and_config()?;
    let runtime = runtime(&global);
    match command {
        ComposeCommand::Check(arguments) => {
            let (project, checked) = check_project_compose(&home, arguments, &runtime)?;
            print!("{}", checked.model.summary());
            eprintln!("casulo allows these services");
            require_compose_approval(&home, &project, &checked, false)
        }
        ComposeCommand::Up(arguments) => {
            let (project, checked) = check_project_compose(&home, arguments, &runtime)?;
            require_compose_approval(&home, &project, &checked, true)?;
            compose_service::start(&home, &project, &checked, &runtime)
        }
        ComposeCommand::Down(arguments) => {
            run_action(&home, arguments, ComposeAction::Down, &runtime)
        }
        ComposeCommand::Ps(arguments) => run_action(&home, arguments, ComposeAction::Ps, &runtime),
    }
}

/// Checks the compose file of the project chosen by `arguments`.
fn check_project_compose(
    home: &CasuloHome,
    arguments: ComposeArgs,
    runtime: &dyn Compose,
) -> Result<(ProjectName, CheckedCompose)> {
    let project = environment::resolve_project(home, arguments.project)?;
    let project_config = config_file::load_project_config(&home.project_config_file(&project))?;
    let workspace = project_config.workspace_dir(home, &project);
    let checked = compose_service::check(&project, &workspace, &project_config.compose, runtime)?;
    Ok((project, checked))
}

/// Runs `action`, which needs no compose file, for the project chosen by
/// `arguments`.
fn run_action(
    home: &CasuloHome,
    arguments: ComposeArgs,
    action: ComposeAction,
    runtime: &dyn Compose,
) -> Result<()> {
    let project = environment::resolve_project(home, arguments.project)?;
    compose_service::run_command(home, &project, action, runtime)
}

/// Asks the user to approve the compose model when it is new or changed
/// since the last approval, and records the answer. Fails when the user
/// says no, or when there is no terminal to ask on.
pub(super) fn require_compose_approval(
    home: &CasuloHome,
    project: &ProjectName,
    checked: &CheckedCompose,
    show_summary: bool,
) -> Result<()> {
    let question = match compose_service::approval(home, project, checked)? {
        Approval::UpToDate => return Ok(()),
        Approval::FirstTime => {
            if show_summary {
                eprint!(
                    "casulo will start these services:\n{}",
                    checked.model.summary()
                );
            }
            "start these services for the agent? [y/N] "
        }
        Approval::Changed(changes) => {
            eprint!("the compose file changed since you approved it:\n{changes}");
            "approve the change? [y/N] "
        }
    };
    if !environment::ask(&mut std::io::stdin().lock(), question)? {
        bail!(
            "the services were not approved, so none will be started; review them with `casulo compose check`"
        );
    }
    compose_service::approve(home, project, checked)?;
    eprintln!("approved");
    Ok(())
}
