//! `casulo setup`: creates the casulo home, then fixes what the host is
//! missing, asking before each command.

use std::io::{self, BufRead};

use anyhow::{Result, bail};

use super::runtime;
use crate::application::doctor::domain::{Finding, Fix, HostOs, Status};
use crate::application::doctor::ports::{GitDiagnostics, RuntimeDiagnostics};
use crate::application::doctor::services::doctor::{check_host, first_problem};
use crate::application::project::services::setup_home;
use crate::cli::environment::{self, ask, read_answer};
use crate::cli::output::doctor::indent;
use crate::infrastructure::config_file::{self, ConfigFiles};
use crate::infrastructure::git::GitCli;
use crate::infrastructure::shell;

/// `casulo setup`.
pub fn setup() -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    for entry in setup_home::initialize_home(&home, &ConfigFiles)? {
        if entry.created {
            eprintln!("created {}", entry.path.display());
        } else if entry.is_file {
            eprintln!("kept {} (it already exists)", entry.path.display());
        }
    }
    let global = config_file::load_global_config(&home.global_config_file())?;
    let runtime = runtime(&global);
    fix_requirements(&GitCli, &runtime, environment::host_os_from_environment())?;
    Ok(())
}

/// Fixes the requirements one at a time, asking before running anything.
fn fix_requirements(
    git: &dyn GitDiagnostics,
    runtime: &dyn RuntimeDiagnostics,
    os: HostOs,
) -> Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut last_fixed = None;
    loop {
        let findings = check_host(git, runtime, os);
        let Some(problem) = first_problem(&findings) else {
            eprintln!("everything casulo needs is ready");
            return Ok(());
        };
        let label = problem.requirement.label(runtime.binary());
        if last_fixed == Some(problem.requirement) {
            bail!(
                "{label} is still not ready; it may need a new login, a restart or a moment to start, then run `casulo doctor` again"
            );
        }
        eprintln!("problem: {label}: {}", problem_message(problem));

        let Some(fix) = choose_fix(&problem.fixes, &mut input)? else {
            bail!("setup stopped; run `casulo doctor` to see what is left");
        };
        if fix.commands.is_empty() {
            bail!(
                "casulo has no command for this step; {}: {}, then run `casulo setup` again",
                fix.summary,
                fix.docs_url
            );
        }
        eprintln!("\nthis will run:");
        for command in &fix.commands {
            eprintln!("{}", indent(command, "  $ "));
        }
        if fix.uses_sudo() {
            eprintln!("these commands use sudo, so you may be asked for your password");
        }
        if !ask(&mut input, "run these commands? [y/N] ")? {
            bail!("setup stopped; run `casulo doctor` to see what is left");
        }
        for command in &fix.commands {
            eprintln!("{}", indent(command, "$ "));
            if !shell::run_interactive(command)? {
                bail!(
                    "the command failed; see its output above, or follow {}",
                    fix.docs_url
                );
            }
        }
        last_fixed = Some(problem.requirement);
    }
}

/// Lets the user pick one of the alternative fixes, or none.
fn choose_fix(fixes: &[Fix], input: &mut dyn BufRead) -> Result<Option<Fix>> {
    if fixes.len() == 1 {
        eprintln!("fix: {}", fixes[0].summary);
        return Ok(fixes.first().cloned());
    }
    eprintln!("ways to fix it:");
    for (index, fix) in fixes.iter().enumerate() {
        eprintln!("  {}) {}", index + 1, fix.summary);
    }
    let answer = read_answer(
        input,
        &format!("choose 1-{}, or press enter to stop: ", fixes.len()),
    )?;
    let choice = answer.parse::<usize>().ok();
    Ok(choice.and_then(|number| fixes.get(number.checked_sub(1)?).cloned()))
}

fn problem_message(finding: &Finding) -> &str {
    match &finding.status {
        Status::Problem(message) => message,
        Status::Ok(detail) => detail,
        Status::NotChecked => "not checked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(summary: &str) -> Fix {
        Fix::commands(summary, &[], "https://docs.example")
    }

    #[test]
    fn single_fix_is_chosen_without_asking() {
        let fixes = [fix("only")];
        let mut input = "".as_bytes();

        let chosen = choose_fix(&fixes, &mut input).unwrap();

        assert_eq!(chosen.map(|fix| fix.summary), Some("only".to_string()));
    }

    #[test]
    fn user_picks_an_alternative_by_number() {
        let fixes = [fix("first"), fix("second")];
        let mut input = "2\n".as_bytes();

        let chosen = choose_fix(&fixes, &mut input).unwrap();

        assert_eq!(chosen.map(|fix| fix.summary), Some("second".to_string()));
    }

    #[test]
    fn empty_or_invalid_choice_picks_nothing() {
        let fixes = [fix("first"), fix("second")];
        for answer in ["", "\n", "0\n", "3\n", "abc\n"] {
            let mut input = answer.as_bytes();

            assert!(
                choose_fix(&fixes, &mut input).unwrap().is_none(),
                "{answer:?}"
            );
        }
    }
}
