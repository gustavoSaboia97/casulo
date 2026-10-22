//! Traits for the outside world used by the doctor group, implemented in
//! `crate::infrastructure` and replaced by fakes in tests.

use anyhow::Result;

use crate::application::doctor::domain::{Fix, HostOs, ServerStatus, Version};

/// What `casulo doctor` asks a container runtime: is it installed and
/// running, and how to fix it on each OS.
pub trait RuntimeDiagnostics {
    /// Name or path of the runtime binary, as configured.
    fn binary(&self) -> &str;

    /// Version of the command-line tool. Fails when it is not installed.
    fn client_version(&self) -> Result<String>;

    /// Asks the daemon about itself.
    fn server_status(&self) -> ServerStatus;

    /// Oldest daemon version casulo supports.
    fn minimum_version(&self) -> Version;

    /// Ways to install the command-line tool and daemon on this OS.
    fn install_fixes(&self, os: HostOs) -> Vec<Fix>;

    /// Ways to start the daemon on this OS.
    fn start_fixes(&self, os: HostOs) -> Vec<Fix>;

    /// Ways to let the current user talk to the daemon on this OS.
    fn permission_fixes(&self, os: HostOs) -> Vec<Fix>;

    /// Ways to upgrade the runtime on this OS.
    fn upgrade_fixes(&self, os: HostOs) -> Vec<Fix>;
}

/// What `casulo doctor` asks the git command-line tool: is it installed,
/// and how to install it on each OS.
pub trait GitDiagnostics {
    /// Version reported by git. Fails when it is not installed.
    fn version(&self) -> Result<String>;

    /// Ways to install git on this OS.
    fn install_fixes(&self, os: HostOs) -> Vec<Fix>;
}
