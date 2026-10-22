//! Checks the host requirements and says how to fix what is missing.

use crate::application::doctor::domain::{
    Finding, Fix, HostOs, Requirement, ServerStatus, Status, Version,
};
use crate::application::doctor::ports::{GitDiagnostics, RuntimeDiagnostics};

/// Checks every requirement in order. A requirement whose dependency failed
/// is reported as not checked instead of repeating the same problem.
pub fn check_host(
    git: &dyn GitDiagnostics,
    runtime: &dyn RuntimeDiagnostics,
    os: HostOs,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    let git_status = match git.version() {
        Ok(version) => Status::Ok(version),
        Err(error) => Status::Problem(format!("{error:#}")),
    };
    findings.push(finding(Requirement::Git, git_status, git.install_fixes(os)));

    let cli_status = match runtime.client_version() {
        Ok(version) => Status::Ok(version),
        Err(error) => Status::Problem(format!("{error:#}")),
    };
    let cli_installed = matches!(cli_status, Status::Ok(_));
    findings.push(finding(
        Requirement::RuntimeCli,
        cli_status,
        runtime.install_fixes(os),
    ));

    if !cli_installed {
        for requirement in [
            Requirement::RuntimeDaemon,
            Requirement::RuntimePermission,
            Requirement::RuntimeVersion,
        ] {
            findings.push(finding(requirement, Status::NotChecked, Vec::new()));
        }
        return findings;
    }

    // One question to the daemon answers whether it runs, whether we may use
    // it and which version it is.
    let (daemon, permission, version) = match runtime.server_status() {
        ServerStatus::Reachable { version } => (
            Status::Ok("reachable".to_string()),
            Status::Ok("allowed".to_string()),
            check_version(&version, runtime.minimum_version()),
        ),
        ServerStatus::PermissionDenied { message } => (
            Status::Ok("running".to_string()),
            Status::Problem(message),
            Status::NotChecked,
        ),
        ServerStatus::Unreachable { message } => (
            Status::Problem(message),
            Status::NotChecked,
            Status::NotChecked,
        ),
    };
    findings.push(finding(
        Requirement::RuntimeDaemon,
        daemon,
        runtime.start_fixes(os),
    ));
    findings.push(finding(
        Requirement::RuntimePermission,
        permission,
        runtime.permission_fixes(os),
    ));
    findings.push(finding(
        Requirement::RuntimeVersion,
        version,
        runtime.upgrade_fixes(os),
    ));
    findings
}

/// The first problem in the findings, which is the one to fix first.
pub fn first_problem(findings: &[Finding]) -> Option<&Finding> {
    findings.iter().find(|finding| finding.is_problem())
}

fn check_version(reported: &str, minimum: Version) -> Status {
    match Version::parse(reported) {
        Some(version) if version >= minimum => Status::Ok(reported.to_string()),
        Some(_) => Status::Problem(format!(
            "{reported} is too old, casulo needs {minimum} or newer"
        )),
        None => Status::Problem(format!(
            "cannot read the version \"{reported}\", casulo needs {minimum} or newer"
        )),
    }
}

/// Keeps fixes only for problems, so a met requirement carries none.
fn finding(requirement: Requirement, status: Status, fixes: Vec<Fix>) -> Finding {
    let fixes = if matches!(status, Status::Problem(_)) {
        fixes
    } else {
        Vec::new()
    };
    Finding {
        requirement,
        status,
        fixes,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use super::*;
    use crate::application::doctor::domain::LinuxDistro;

    struct FakeGit {
        installed: bool,
    }

    impl GitDiagnostics for FakeGit {
        fn version(&self) -> Result<String> {
            if self.installed {
                Ok("git version 2.50.0".to_string())
            } else {
                bail!("cannot run git: not found")
            }
        }

        fn install_fixes(&self, _os: HostOs) -> Vec<Fix> {
            vec![Fix::manual("install git", "https://git.example")]
        }
    }

    struct FakeRuntime {
        installed: bool,
        server: ServerStatus,
    }

    impl RuntimeDiagnostics for FakeRuntime {
        fn binary(&self) -> &str {
            "fake-runtime"
        }
        fn client_version(&self) -> Result<String> {
            if self.installed {
                Ok("Fake version 28.0.0".to_string())
            } else {
                bail!("cannot run fake-runtime: not found")
            }
        }
        fn server_status(&self) -> ServerStatus {
            self.server.clone()
        }
        fn minimum_version(&self) -> Version {
            Version {
                major: 24,
                minor: 0,
            }
        }
        fn install_fixes(&self, os: HostOs) -> Vec<Fix> {
            vec![Fix::manual(
                &format!("install on {os:?}"),
                "https://install.example",
            )]
        }
        fn start_fixes(&self, _os: HostOs) -> Vec<Fix> {
            vec![Fix::manual("start", "https://start.example")]
        }
        fn permission_fixes(&self, _os: HostOs) -> Vec<Fix> {
            vec![Fix::manual("allow", "https://allow.example")]
        }
        fn upgrade_fixes(&self, _os: HostOs) -> Vec<Fix> {
            vec![Fix::manual("upgrade", "https://upgrade.example")]
        }
    }

    const OS: HostOs = HostOs::Linux(LinuxDistro::Ubuntu);

    fn runtime(server: ServerStatus) -> FakeRuntime {
        FakeRuntime {
            installed: true,
            server,
        }
    }

    fn reachable(version: &str) -> ServerStatus {
        ServerStatus::Reachable {
            version: version.to_string(),
        }
    }

    fn statuses(findings: &[Finding]) -> Vec<&Status> {
        findings.iter().map(|finding| &finding.status).collect()
    }

    #[test]
    fn healthy_host_has_no_problems() {
        let findings = check_host(
            &FakeGit { installed: true },
            &runtime(reachable("28.3.2")),
            OS,
        );

        assert_eq!(findings.len(), Requirement::ALL.len());
        assert!(first_problem(&findings).is_none());
        assert!(findings.iter().all(|finding| finding.fixes.is_empty()));
    }

    #[test]
    fn findings_follow_the_declared_order() {
        let findings = check_host(
            &FakeGit { installed: true },
            &runtime(reachable("28.3.2")),
            OS,
        );
        let order: Vec<Requirement> = findings.iter().map(|finding| finding.requirement).collect();

        assert_eq!(order, Requirement::ALL);
    }

    #[test]
    fn missing_git_is_a_problem_with_its_fix() {
        let findings = check_host(
            &FakeGit { installed: false },
            &runtime(reachable("28.3.2")),
            OS,
        );
        let problem = first_problem(&findings).expect("git is missing");

        assert_eq!(problem.requirement, Requirement::Git);
        assert_eq!(problem.fixes[0].summary, "install git");
    }

    #[test]
    fn missing_runtime_skips_the_daemon_checks() {
        let missing = FakeRuntime {
            installed: false,
            server: reachable("28.3.2"),
        };
        let findings = check_host(&FakeGit { installed: true }, &missing, HostOs::MacOs);

        assert!(matches!(findings[1].status, Status::Problem(_)));
        assert_eq!(findings[1].fixes[0].summary, "install on MacOs");
        assert_eq!(
            statuses(&findings)[2..],
            [
                &Status::NotChecked,
                &Status::NotChecked,
                &Status::NotChecked
            ]
        );
    }

    #[test]
    fn stopped_daemon_offers_to_start_it() {
        let stopped = ServerStatus::Unreachable {
            message: "Cannot connect".to_string(),
        };
        let findings = check_host(&FakeGit { installed: true }, &runtime(stopped), OS);
        let problem = first_problem(&findings).expect("daemon is stopped");

        assert_eq!(problem.requirement, Requirement::RuntimeDaemon);
        assert_eq!(
            problem.status,
            Status::Problem("Cannot connect".to_string())
        );
        assert_eq!(problem.fixes[0].summary, "start");
        assert_eq!(
            statuses(&findings)[3..],
            [&Status::NotChecked, &Status::NotChecked]
        );
    }

    #[test]
    fn permission_denied_offers_the_permission_fix() {
        let denied = ServerStatus::PermissionDenied {
            message: "permission denied".to_string(),
        };
        let findings = check_host(&FakeGit { installed: true }, &runtime(denied), OS);
        let problem = first_problem(&findings).expect("permission is denied");

        assert_eq!(problem.requirement, Requirement::RuntimePermission);
        assert_eq!(problem.fixes[0].summary, "allow");
        assert_eq!(findings[4].status, Status::NotChecked);
    }

    #[test]
    fn old_version_offers_an_upgrade() {
        let findings = check_host(
            &FakeGit { installed: true },
            &runtime(reachable("23.0.6")),
            OS,
        );
        let problem = first_problem(&findings).expect("version is too old");

        assert_eq!(problem.requirement, Requirement::RuntimeVersion);
        assert_eq!(
            problem.status,
            Status::Problem("23.0.6 is too old, casulo needs 24.0 or newer".to_string())
        );
        assert_eq!(problem.fixes[0].summary, "upgrade");
    }

    #[test]
    fn minimum_version_is_accepted() {
        assert_eq!(
            check_version(
                "24.0.0",
                Version {
                    major: 24,
                    minor: 0
                }
            ),
            Status::Ok("24.0.0".to_string())
        );
    }

    #[test]
    fn unreadable_version_is_a_problem() {
        assert!(matches!(
            check_version(
                "dev",
                Version {
                    major: 24,
                    minor: 0
                }
            ),
            Status::Problem(_)
        ));
    }
}
