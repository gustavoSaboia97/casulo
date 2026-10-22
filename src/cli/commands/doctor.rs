//! `casulo doctor`: checks that git and the container runtime are ready.

use anyhow::{Result, bail};

use super::{home_and_config, runtime};
use crate::application::doctor::domain::HostOs;
use crate::application::doctor::ports::{GitDiagnostics, RuntimeDiagnostics};
use crate::application::doctor::services::doctor::{check_host, first_problem};
use crate::cli::environment;
use crate::cli::output::doctor::format_report;
use crate::infrastructure::git::GitCli;

/// `casulo doctor`.
pub fn doctor() -> Result<()> {
    let (_, global) = home_and_config()?;
    let runtime = runtime(&global);
    report(&GitCli, &runtime, environment::host_os_from_environment())?;
    Ok(())
}

/// Prints the state of every requirement and fails if any is not met.
fn report(git: &dyn GitDiagnostics, runtime: &dyn RuntimeDiagnostics, os: HostOs) -> Result<()> {
    let findings = check_host(git, runtime, os);
    print!("{}", format_report(&findings, runtime.binary()));
    if first_problem(&findings).is_some() {
        bail!("some requirements are not met; run `casulo setup` to fix them");
    }
    Ok(())
}
