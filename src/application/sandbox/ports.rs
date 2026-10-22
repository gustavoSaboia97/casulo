//! Traits for the outside world used by the sandbox group, implemented in
//! `crate::infrastructure` and replaced by fakes in tests.
//!
//! A trait is like an abstract base class in Python: it lists methods, and any
//! type that implements them can be passed where `&dyn Trait` is expected.

use std::path::Path;

use anyhow::Result;

use crate::application::sandbox::domain::{CommandLine, DetachedStart, RunSpec};

// A container runtime such as Docker is called through its command-line
// tool. Its abilities are split into four small traits, so each service
// asks only for what it uses and a test fake implements only that; the
// Docker adapter implements all four. `RuntimeDiagnostics` is in
// `application::doctor`.

/// The images of a container runtime.
pub trait Images {
    /// Builds `dockerfile` with `context_dir` as the build context into an
    /// image named `tag`, showing the build output to the user.
    fn build_image(&self, dockerfile: &Path, context_dir: &Path, tag: &str) -> Result<()>;

    /// Whether an image named `tag` exists locally.
    fn image_exists(&self, tag: &str) -> Result<bool>;
}

/// The containers and networks of a container runtime.
pub trait Containers {
    /// The exact commands that would start the container described by
    /// `spec`, in order.
    fn run_command_lines(&self, spec: &RunSpec) -> Vec<CommandLine>;

    /// Starts the container attached to the terminal and waits for it.
    /// Returns its exit code.
    fn run(&self, spec: &RunSpec) -> Result<i32>;

    /// Starts the container described by `spec` in the background.
    fn start_detached(&self, spec: &RunSpec) -> Result<DetachedStart>;

    /// Whether a container named `name` is running.
    fn container_running(&self, name: &str) -> Result<bool>;

    /// Names of the running containers on `network`.
    fn containers_on_network(&self, network: &str) -> Result<Vec<String>>;

    /// Stops the container named `name`.
    fn stop_container(&self, name: &str) -> Result<()>;

    /// The command that creates `network`. An internal network has no route
    /// to the internet.
    fn network_create_command_line(&self, network: &str, internal: bool) -> CommandLine;

    /// Creates `network` unless it exists.
    fn ensure_network(&self, network: &str, internal: bool) -> Result<()>;
}

/// The compose tool of a container runtime.
pub trait Compose {
    /// The normalized model of the compose `file`, as JSON, read with
    /// `workspace` as the project directory and `project_name` as the
    /// compose project. Starts nothing.
    fn compose_config(&self, project_name: &str, workspace: &Path, file: &Path) -> Result<String>;

    /// Starts the services of the validated compose model `json` in the
    /// background and waits until they are healthy.
    fn compose_up(&self, project_name: &str, project_directory: &Path, json: &str) -> Result<()>;

    /// Runs a compose subcommand that needs no file, such as `down` or `ps`,
    /// showing its output.
    fn compose_command(
        &self,
        project_name: &str,
        project_directory: &Path,
        command: &str,
    ) -> Result<()>;
}
