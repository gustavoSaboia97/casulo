//! Host requirements and the result of checking them.

use super::host::Fix;

/// Something the host needs before casulo can run agents, in the order
/// they are checked. Later requirements depend on earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// git is installed, for cloning projects on the host.
    Git,
    /// The container runtime command-line tool is installed.
    RuntimeCli,
    /// The container runtime daemon is running and reachable.
    RuntimeDaemon,
    /// The current user is allowed to talk to the daemon.
    RuntimePermission,
    /// The daemon is recent enough.
    RuntimeVersion,
}

impl Requirement {
    /// Every requirement, in the order they are checked.
    pub const ALL: [Requirement; 5] = [
        Requirement::Git,
        Requirement::RuntimeCli,
        Requirement::RuntimeDaemon,
        Requirement::RuntimePermission,
        Requirement::RuntimeVersion,
    ];

    /// Short description shown in reports, with the runtime binary name.
    pub fn label(self, runtime: &str) -> String {
        match self {
            Requirement::Git => "git".to_string(),
            Requirement::RuntimeCli => format!("{runtime} command-line tool"),
            Requirement::RuntimeDaemon => format!("{runtime} daemon running"),
            Requirement::RuntimePermission => format!("permission to use {runtime}"),
            Requirement::RuntimeVersion => format!("{runtime} version"),
        }
    }
}

/// What the runtime daemon answered when asked about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// The daemon answered, with its version as reported.
    Reachable { version: String },
    /// The daemon exists but the current user may not talk to it.
    PermissionDenied { message: String },
    /// The daemon did not answer: not started, or not installed.
    Unreachable { message: String },
}

/// The outcome of checking one requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The requirement is met, with a detail such as the version found.
    Ok(String),
    /// The requirement is not met, with what is wrong.
    Problem(String),
    /// Not checked because an earlier requirement is not met.
    NotChecked,
}

/// One checked requirement, with the ways to fix it when it is not met.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Which requirement was checked.
    pub requirement: Requirement,
    /// What the check found.
    pub status: Status,
    /// Alternative ways to fix a problem. Empty when the requirement is met.
    pub fixes: Vec<Fix>,
}

impl Finding {
    /// Whether this finding is a problem the user has to fix.
    pub fn is_problem(&self) -> bool {
        matches!(self.status, Status::Problem(_))
    }
}
