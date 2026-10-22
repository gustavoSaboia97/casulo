//! The arguments casulo gives the `docker` binary. Pure functions, so the
//! hardening of every container is tested without Docker.

use std::ffi::OsString;
use std::path::Path;

use crate::application::sandbox::domain::container::{
    CONTAINER_HOME, CONTAINER_UID, CONTAINER_WORKSPACE,
};
use crate::application::sandbox::domain::{EnvironmentVariable, Mount, RunSpec};

/// Arguments of `docker compose` for the project, before the subcommand.
pub(super) fn compose_project_args(project_name: &str, project_directory: &Path) -> Vec<OsString> {
    vec![
        "compose".into(),
        "--project-name".into(),
        project_name.into(),
        "--project-directory".into(),
        project_directory.into(),
    ]
}

/// Arguments of `docker compose up` with the model read from stdin, so what
/// runs is exactly what casulo validated. `--wait` returns once the services
/// are running and healthy.
pub(super) fn compose_up_args(project_name: &str, project_directory: &Path) -> Vec<OsString> {
    let mut args = compose_project_args(project_name, project_directory);
    args.extend(["--file", "-", "up", "--detach", "--wait"].map(OsString::from));
    args
}

/// Arguments of `docker compose config` for one compose file. The project
/// name and directory are always given, so `.env` cannot change them.
pub(super) fn compose_config_args(
    project_name: &str,
    workspace: &Path,
    file: &Path,
) -> Vec<OsString> {
    let mut args = compose_project_args(project_name, workspace);
    args.extend([
        "--file".into(),
        file.into(),
        "config".into(),
        "--format".into(),
        "json".into(),
    ]);
    args
}

/// Most processes the container may run, which stops fork bombs.
const PIDS_LIMIT: u32 = 1024;

/// Size limit of each in-memory directory (`/tmp` and the home).
const TMPFS_SIZE: &str = "1g";

/// Arguments of `docker network create`.
pub(super) fn network_create_args(network: &str, internal: bool) -> Vec<String> {
    let mut args = vec!["network".to_string(), "create".to_string()];
    if internal {
        args.push("--internal".to_string());
    }
    args.push(network.to_string());
    args
}

/// The spec's secret values as `(name, value)` pairs for a process
/// environment, from which docker forwards each one named with `--env=NAME`.
pub(super) fn secret_values(spec: &RunSpec) -> impl Iterator<Item = (&str, &str)> {
    spec.secret_values
        .iter()
        .map(|(name, secret)| (name.as_str(), secret.expose()))
}

/// The commands that start `spec`, each as its arguments. One `docker run`,
/// or, to join more than one network before the program starts, `docker
/// create`, a `docker network connect` per extra network and `docker start`.
pub(super) fn run_steps(spec: &RunSpec) -> Vec<Vec<String>> {
    let Some(name) = spec
        .name
        .as_ref()
        .filter(|_| !spec.extra_networks.is_empty())
    else {
        return vec![run_args(spec)];
    };
    let mut steps = vec![container_args("create", spec)];
    for network in &spec.extra_networks {
        steps.push(vec![
            "network".to_string(),
            "connect".to_string(),
            network.clone(),
            name.clone(),
        ]);
    }
    steps.push(vec![
        "start".to_string(),
        "--attach".to_string(),
        "--interactive".to_string(),
        name.clone(),
    ]);
    steps
}

/// Arguments of `docker run` for `spec`.
pub(super) fn run_args(spec: &RunSpec) -> Vec<String> {
    container_args("run", spec)
}

/// Arguments of `docker run` or `docker create` for `spec`, hardened: no
/// capabilities, no privilege escalation, a read-only root filesystem with
/// in-memory `/tmp` and home, and only the workspace (plus `spec.mounts`)
/// from the host.
pub(super) fn container_args(verb: &str, spec: &RunSpec) -> Vec<String> {
    let mut args = vec![verb.to_string(), "--rm".to_string()];
    if spec.detach {
        // A server in the background runs as PID 1, which ignores SIGTERM;
        // `--init` forwards it, so `docker stop` does not wait 10 seconds.
        args.extend(["--detach".to_string(), "--init".to_string()]);
    } else {
        args.push("--interactive".to_string());
        if spec.terminal {
            args.push("--tty".to_string());
        }
    }
    if let Some(name) = &spec.name {
        args.push(format!("--name={name}"));
    }
    args.extend([
        "--cap-drop=ALL".to_string(),
        "--security-opt=no-new-privileges".to_string(),
        format!("--pids-limit={PIDS_LIMIT}"),
        "--read-only".to_string(),
        // Never download: a missing local image would otherwise be pulled
        // from a registry where anyone could have published that name.
        "--pull=never".to_string(),
        format!("--tmpfs=/tmp:rw,exec,nosuid,nodev,size={TMPFS_SIZE},mode=1777"),
    ]);
    // The home is owned by whoever the agent runs as, or it could not write there.
    let (uid, gid) = match spec.user {
        Some(user) => (user.uid, user.gid),
        None => (CONTAINER_UID, CONTAINER_UID),
    };
    args.push(format!(
        "--tmpfs={CONTAINER_HOME}:rw,exec,nosuid,nodev,size={TMPFS_SIZE},uid={uid},gid={gid},mode=0700"
    ));
    let workspace = Mount {
        source: spec.workspace.clone(),
        target: CONTAINER_WORKSPACE.to_string(),
        read_only: spec.workspace_read_only,
    };
    for mount in std::iter::once(&workspace).chain(&spec.mounts) {
        args.push(mount_arg(mount));
    }
    args.push(format!("--workdir={CONTAINER_WORKSPACE}"));
    if let Some(network) = &spec.network {
        args.push(format!("--network={network}"));
        if let Some(alias) = &spec.network_alias {
            args.push(format!("--network-alias={alias}"));
        }
    }
    if let Some(user) = spec.user {
        args.push(format!("--user={}:{}", user.uid, user.gid));
    }
    for variable in &spec.environment {
        args.push(match variable {
            EnvironmentVariable::Value { name, value } => format!("--env={name}={value}"),
            // Only the name: docker reads the value from its own environment.
            EnvironmentVariable::FromHost { name } => format!("--env={name}"),
        });
    }
    args.push(spec.image.clone());
    args.extend(spec.command.iter().cloned());
    args
}

/// A `--mount` argument. Docker reads it as CSV, so a field with a comma or
/// a quote is wrapped in quotes.
pub(super) fn mount_arg(mount: &Mount) -> String {
    let mut fields = vec![
        "type=bind".to_string(),
        csv_field(&format!("source={}", mount.source.display())),
        csv_field(&format!("target={}", mount.target)),
    ];
    if mount.read_only {
        fields.push("readonly".to_string());
    }
    format!("--mount={}", fields.join(","))
}

pub(super) fn csv_field(field: &str) -> String {
    if field.contains([',', '"']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Arguments of `docker build`. The context is never the workspace: it is
/// the directory holding only the generated Dockerfile, or casulo's own
/// source for a development build of the memory image.
pub(super) fn build_args(
    dockerfile: &Path,
    context_dir: &Path,
    tag: &str,
) -> Vec<std::ffi::OsString> {
    vec![
        "build".into(),
        "--tag".into(),
        tag.into(),
        "--file".into(),
        dockerfile.as_os_str().to_os_string(),
        context_dir.as_os_str().to_os_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sandbox::domain::HostUser;

    fn spec(user: Option<HostUser>) -> RunSpec {
        // A fixed path keeps the snapshots the same on every OS.
        RunSpec {
            image: "casulo/demo:claude".to_string(),
            command: vec!["claude".to_string()],
            workspace: std::path::PathBuf::from("/home/me/.casulo/projects/demo/workspace"),
            mounts: Vec::new(),
            environment: vec![
                EnvironmentVariable::Value {
                    name: "HOME".to_string(),
                    value: "/home/casulo".to_string(),
                },
                EnvironmentVariable::Value {
                    name: "GIT_AUTHOR_NAME".to_string(),
                    value: "Ada Lovelace".to_string(),
                },
                EnvironmentVariable::FromHost {
                    name: "ANTHROPIC_API_KEY".to_string(),
                },
            ],
            secret_values: Vec::new(),
            user,
            terminal: true,
            network: None,
            workspace_read_only: false,
            network_alias: None,
            extra_networks: Vec::new(),
            name: None,
            detach: false,
        }
    }

    #[test]
    fn run_args_on_linux() {
        let linux = spec(Some(HostUser {
            uid: 1001,
            gid: 1002,
        }));

        insta::assert_snapshot!(run_args(&linux).join("\n"));
    }

    #[test]
    fn run_args_on_macos_and_windows() {
        insta::assert_snapshot!(run_args(&spec(None)).join("\n"));
    }

    #[test]
    fn secrets_are_passed_by_name_only() {
        let args = run_args(&spec(None));

        assert!(args.contains(&"--env=ANTHROPIC_API_KEY".to_string()));
        assert!(
            !args
                .iter()
                .any(|arg| arg.starts_with("--env=ANTHROPIC_API_KEY="))
        );
    }

    #[test]
    fn mount_fields_with_commas_are_quoted() {
        let mount = Mount {
            source: std::path::PathBuf::from("/data/a,b"),
            target: "/home/casulo/.claude".to_string(),
            read_only: true,
        };

        assert_eq!(
            mount_arg(&mount),
            "--mount=type=bind,\"source=/data/a,b\",target=/home/casulo/.claude,readonly"
        );
    }

    #[test]
    fn compose_config_names_the_project_directory_and_file() {
        let workspace = std::path::PathBuf::from("app");
        let file = workspace.join("docker-compose.yml");

        let args = compose_config_args("casulo-app", &workspace, &file);

        assert_eq!(
            args,
            [
                "compose".into(),
                "--project-name".into(),
                "casulo-app".into(),
                "--project-directory".into(),
                workspace.clone().into_os_string(),
                "--file".into(),
                file.into_os_string(),
                "config".into(),
                "--format".into(),
                "json".into(),
            ]
        );
    }

    #[test]
    fn compose_up_reads_the_validated_model_from_stdin() {
        let directory = std::path::PathBuf::from("compose");

        let args = compose_up_args("casulo-app", &directory);

        assert_eq!(
            args,
            [
                "compose".into(),
                "--project-name".into(),
                "casulo-app".into(),
                "--project-directory".into(),
                directory.into_os_string(),
                "--file".into(),
                "-".into(),
                "up".into(),
                "--detach".into(),
                "--wait".into(),
            ]
        );
    }

    #[test]
    fn network_is_joined_only_when_set() {
        let mut with_network = spec(None);
        with_network.network = Some("casulo-demo_default".to_string());

        assert!(
            !run_args(&spec(None))
                .iter()
                .any(|arg| arg.starts_with("--network"))
        );
        assert!(run_args(&with_network).contains(&"--network=casulo-demo_default".to_string()));
    }

    #[test]
    fn memory_container_runs_detached_with_a_read_only_workspace() {
        let mut memory = spec(None);
        memory.name = Some("casulo-memory-demo".to_string());
        memory.network = Some("casulo-memory-demo".to_string());
        memory.network_alias = Some("memory".to_string());
        memory.workspace_read_only = true;
        memory.detach = true;

        insta::assert_snapshot!(run_steps(&memory)[0].join("\n"));
    }

    #[test]
    fn extra_networks_are_joined_before_the_agent_starts() {
        let mut agent = spec(None);
        agent.name = Some("casulo-demo-claude-42".to_string());
        agent.extra_networks = vec!["casulo-memory-demo".to_string()];

        let steps = run_steps(&agent);

        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0][..2], ["create", "--rm"]);
        assert!(steps[0].contains(&"--name=casulo-demo-claude-42".to_string()));
        assert_eq!(
            steps[1],
            [
                "network",
                "connect",
                "casulo-memory-demo",
                "casulo-demo-claude-42"
            ]
        );
        assert_eq!(
            steps[2],
            [
                "start",
                "--attach",
                "--interactive",
                "casulo-demo-claude-42"
            ]
        );
    }

    #[test]
    fn without_extra_networks_it_is_one_run() {
        assert_eq!(run_steps(&spec(None)), [run_args(&spec(None))]);
    }

    #[test]
    fn network_create_can_be_internal() {
        assert_eq!(
            network_create_args("casulo-memory-demo", true),
            ["network", "create", "--internal", "casulo-memory-demo"]
        );
    }

    #[test]
    fn build_uses_the_generated_dockerfile_and_context() {
        let context = std::path::PathBuf::from("build").join("claude");

        let args = build_args(&context.join("Dockerfile"), &context, "casulo/demo:claude");

        assert_eq!(
            args,
            [
                "build".into(),
                "--tag".into(),
                "casulo/demo:claude".into(),
                "--file".into(),
                context.join("Dockerfile").into_os_string(),
                context.clone().into_os_string(),
            ]
        );
    }
}
