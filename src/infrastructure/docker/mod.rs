//! The Docker container runtime, called through the `docker` command-line tool.
//!
//! Everything specific to Docker lives here. This file runs the binary;
//! `args` builds its arguments, including the hardening of every
//! container, and `diagnostics` reads its answers and knows how to install
//! it on each system.

mod args;
mod diagnostics;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::application::doctor::domain::{Fix, HostOs, ServerStatus, Version};
use crate::application::doctor::ports::RuntimeDiagnostics;
use crate::application::sandbox::domain::{CommandLine, DetachedStart, RunSpec};
use crate::application::sandbox::ports::{Compose, Containers, Images};
use args::{
    build_args, compose_config_args, compose_project_args, compose_up_args, network_create_args,
    run_args, run_steps, secret_values,
};
use diagnostics::{classify_info, first_line};

/// Oldest Docker Engine casulo supports. 24.0 still ships in the package
/// repositories of current long-term-support distributions.
const MINIMUM_VERSION: Version = Version {
    major: 24,
    minor: 0,
};

/// Docker, called through the configured binary.
pub struct Docker {
    binary: String,
}

impl Docker {
    /// A Docker runtime that runs `binary` (for example "docker" or a full path).
    pub fn new(binary: &str) -> Self {
        Self {
            binary: binary.to_string(),
        }
    }

    fn not_runnable(&self) -> String {
        format!(
            "cannot run {}; is it installed? run `casulo doctor` to check",
            self.binary
        )
    }
}

impl RuntimeDiagnostics for Docker {
    fn binary(&self) -> &str {
        &self.binary
    }

    fn client_version(&self) -> Result<String> {
        let output = Command::new(&self.binary)
            .arg("--version")
            .output()
            .with_context(|| format!("cannot run {}", self.binary))?;
        if !output.status.success() {
            bail!("{} --version failed", self.binary);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn server_status(&self) -> ServerStatus {
        let output = Command::new(&self.binary)
            .args(["info", "--format", "{{.ServerVersion}}"])
            .output();
        match output {
            Ok(output) => classify_info(
                output.status.success(),
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
            ),
            Err(error) => ServerStatus::Unreachable {
                message: format!("cannot run {}: {error}", self.binary),
            },
        }
    }

    fn minimum_version(&self) -> Version {
        MINIMUM_VERSION
    }

    fn install_fixes(&self, os: HostOs) -> Vec<Fix> {
        diagnostics::install_fixes(os)
    }

    fn start_fixes(&self, os: HostOs) -> Vec<Fix> {
        diagnostics::start_fixes(os)
    }

    fn permission_fixes(&self, os: HostOs) -> Vec<Fix> {
        diagnostics::permission_fixes(os)
    }

    fn upgrade_fixes(&self, os: HostOs) -> Vec<Fix> {
        diagnostics::upgrade_fixes(os)
    }
}

impl Images for Docker {
    fn build_image(&self, dockerfile: &Path, context_dir: &Path, tag: &str) -> Result<()> {
        let status = Command::new(&self.binary)
            .args(build_args(dockerfile, context_dir, tag))
            .status()
            .with_context(|| self.not_runnable())?;
        if !status.success() {
            bail!("the image build failed; see the output above");
        }
        Ok(())
    }

    fn image_exists(&self, tag: &str) -> Result<bool> {
        let output = Command::new(&self.binary)
            .args(["image", "inspect", "--format", "{{.Id}}", tag])
            .output()
            .with_context(|| self.not_runnable())?;
        if output.status.success() {
            return Ok(true);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_lowercase().contains("no such image") {
            return Ok(false);
        }
        bail!(
            "{} is not answering: {}; run `casulo doctor` to see what is wrong",
            self.binary,
            first_line(&stderr)
        )
    }
}

impl Containers for Docker {
    fn run_command_lines(&self, spec: &RunSpec) -> Vec<CommandLine> {
        run_steps(spec)
            .into_iter()
            .map(|args| CommandLine {
                program: self.binary.clone(),
                args,
            })
            .collect()
    }

    fn run(&self, spec: &RunSpec) -> Result<i32> {
        // Secret values reach docker through the environment it inherits from
        // casulo, or from `secret_values`, and docker forwards the ones named
        // with `--env=NAME`.
        let mut steps = run_steps(spec);
        let last = steps.pop().expect("there is always a step that starts it");
        for (index, step) in steps.iter().enumerate() {
            // `create` prints the container id, which the user does not need.
            let output = Command::new(&self.binary)
                .args(step)
                .envs(secret_values(spec))
                .stderr(Stdio::inherit())
                .output()
                .with_context(|| self.not_runnable())?;
            if !output.status.success() {
                if index > 0
                    && let Some(name) = &spec.name
                {
                    // Created but not started: remove it, or the name stays taken.
                    let _ = Command::new(&self.binary)
                        .args(["rm", "--force", name])
                        .output();
                }
                bail!("{} {} failed; see its output above", self.binary, step[0]);
            }
        }
        let status = Command::new(&self.binary)
            .args(last)
            .envs(secret_values(spec))
            .status()
            .with_context(|| self.not_runnable())?;
        // No code means the container was stopped by a signal.
        Ok(status.code().unwrap_or(1))
    }

    fn start_detached(&self, spec: &RunSpec) -> Result<DetachedStart> {
        let output = Command::new(&self.binary)
            .args(run_args(spec))
            .output()
            .with_context(|| self.not_runnable())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("is already in use") {
                return Ok(DetachedStart::NameTaken);
            }
            bail!(
                "cannot start container {}: {}",
                spec.name.as_deref().unwrap_or(&spec.image),
                first_line(&stderr)
            );
        }
        Ok(DetachedStart::Started)
    }

    fn container_running(&self, name: &str) -> Result<bool> {
        let output = Command::new(&self.binary)
            .args([
                "container",
                "inspect",
                "--format",
                "{{.State.Running}}",
                name,
            ])
            .output()
            .with_context(|| self.not_runnable())?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim() == "true");
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_lowercase().contains("no such") {
            return Ok(false);
        }
        bail!(
            "{} is not answering: {}; run `casulo doctor` to see what is wrong",
            self.binary,
            first_line(&stderr)
        )
    }

    fn containers_on_network(&self, network: &str) -> Result<Vec<String>> {
        let output = Command::new(&self.binary)
            .args([
                "ps",
                "--filter",
                &format!("network={network}"),
                "--format",
                "{{.Names}}",
            ])
            .output()
            .with_context(|| self.not_runnable())?;
        if !output.status.success() {
            bail!(
                "cannot list the containers on {network}: {}",
                first_line(&String::from_utf8_lossy(&output.stderr))
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn stop_container(&self, name: &str) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["stop", name])
            .output()
            .with_context(|| self.not_runnable())?;
        if !output.status.success() {
            bail!(
                "cannot stop {name}: {}",
                first_line(&String::from_utf8_lossy(&output.stderr))
            );
        }
        Ok(())
    }

    fn network_create_command_line(&self, network: &str, internal: bool) -> CommandLine {
        CommandLine {
            program: self.binary.clone(),
            args: network_create_args(network, internal),
        }
    }

    fn ensure_network(&self, network: &str, internal: bool) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(network_create_args(network, internal))
            .output()
            .with_context(|| self.not_runnable())?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Two runs may create it at the same moment; either one winning is fine.
        if output.status.success() || stderr.contains("already exists") {
            return Ok(());
        }
        bail!("cannot create network {network}: {}", first_line(&stderr))
    }
}

impl Compose for Docker {
    fn compose_config(&self, project_name: &str, workspace: &Path, file: &Path) -> Result<String> {
        let output = Command::new(&self.binary)
            .args(compose_config_args(project_name, workspace, file))
            .output()
            .with_context(|| self.not_runnable())?;
        if !output.status.success() {
            bail!(
                "{} compose cannot read {}: {}; check the file, and that Docker Compose v2 is installed",
                self.binary,
                file.display(),
                first_line(&String::from_utf8_lossy(&output.stderr))
            );
        }
        String::from_utf8(output.stdout).context("docker compose config printed invalid UTF-8")
    }

    fn compose_up(&self, project_name: &str, project_directory: &Path, json: &str) -> Result<()> {
        // stdout and stderr stay attached, so the user sees the progress.
        let mut child = Command::new(&self.binary)
            .args(compose_up_args(project_name, project_directory))
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| self.not_runnable())?;
        child
            .stdin
            .take()
            .context("cannot write to docker compose")?
            .write_all(json.as_bytes())
            .context("cannot write to docker compose")?;
        // The stdin handle was dropped above, so compose sees the end of the file.
        let status = child.wait().with_context(|| self.not_runnable())?;
        if !status.success() {
            bail!("docker compose up failed; see its output above");
        }
        Ok(())
    }

    fn compose_command(
        &self,
        project_name: &str,
        project_directory: &Path,
        command: &str,
    ) -> Result<()> {
        let mut args = compose_project_args(project_name, project_directory);
        args.push(command.into());
        let status = Command::new(&self.binary)
            .args(args)
            .status()
            .with_context(|| self.not_runnable())?;
        if !status.success() {
            bail!("docker compose {command} failed; see its output above");
        }
        Ok(())
    }
}
