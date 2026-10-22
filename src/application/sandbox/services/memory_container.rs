//! The project memory in `casulo run`: the memory image, and the memory
//! container every agent of a project shares.
//!
//! The memory container is the only one that mounts `memory/`. Agents reach
//! it over MCP on an internal network, so memory gives them no new access.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::application::agent::domain::McpServer;
use crate::application::agent::domain::hooks::SessionEvent;
use crate::application::agent::domain::mcp_server::McpTransport;
use crate::application::sandbox::domain::container::{
    CONTAINER_HOME, CONTAINER_UID, CONTAINER_USER, CONTAINER_WORKSPACE,
};
use crate::application::sandbox::domain::{
    CommandLine, DetachedStart, EnvironmentVariable, HostUser, Mount, RunSpec,
};
use crate::application::sandbox::ports::{Containers, Images};
use crate::application::sandbox::services::inject::{AgentSetup, SessionHook};
use crate::application::shared::domain::{CasuloHome, HarnessName, ProjectName, ResourceName};

/// Port the memory server listens on in its container.
pub const MEMORY_PORT: u16 = 7373;

/// Name the agent reaches the memory container by on the memory network.
pub const MEMORY_HOST: &str = "memory";

/// Where `memory/` is mounted in the memory container.
const CONTAINER_MEMORY_DIR: &str = "/memory";

/// Name of the MCP server agents see.
const MCP_SERVER_NAME: &str = "memory";

/// The variable that names the run's session in the agent container. The
/// session id is not a secret: it only names a row of memory's sessions.
const SESSION_VARIABLE: &str = "CASULO_SESSION";

/// The header that names the session in requests to the memory server.
const SESSION_HEADER: &str = "X-Casulo-Session";

/// The hook script, run with Node, which every agent image has.
const HOOK_SCRIPT: &str = include_str!("memory-hook.mjs");

/// The file name of the hook script in the agent container.
const HOOK_SCRIPT_NAME: &str = "memory-hook.mjs";

/// Release binaries are downloaded from this repository.
const RELEASES_URL: &str = "https://github.com/gustavoSaboia97/casulo/releases/download";

/// Where the memory image gets the Linux `casulo` binary from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// A released casulo: download the static Linux binary of the same
    /// version from the GitHub release, checked against these SHA-256 sums,
    /// which were embedded when the release was built.
    Release {
        version: String,
        sha256_x86_64: String,
        sha256_aarch64: String,
    },
    /// A development build: compile casulo from its source in `source_dir`.
    Source { source_dir: PathBuf },
}

/// Name of the memory network and of the memory container of `project`.
/// Networks and containers have separate names, so one string serves both.
pub fn memory_name(project: &ProjectName) -> String {
    format!("casulo-memory-{project}")
}

/// Name of the memory image.
pub fn image_tag(source: &ImageSource) -> String {
    match source {
        ImageSource::Release { version, .. } => format!("casulo/memory:{version}"),
        ImageSource::Source { .. } => "casulo/memory:dev".to_string(),
    }
}

/// The Dockerfile of the memory image: a slim Linux with git and the
/// `casulo` binary, run as the same unprivileged user as the agents.
pub fn dockerfile(source: &ImageSource) -> String {
    let base = format!(
        "FROM debian:bookworm-slim AS base\n\
         RUN apt-get update \\\n    \
             && apt-get install --yes --no-install-recommends ca-certificates git \\\n    \
             && rm -rf /var/lib/apt/lists/*\n\
         RUN useradd --create-home --home-dir {CONTAINER_HOME} --uid {CONTAINER_UID} --shell /bin/bash {CONTAINER_USER}\n"
    );
    let binary = match source {
        ImageSource::Release {
            version,
            sha256_x86_64,
            sha256_aarch64,
        } => {
            let add = |stage: &str, target: &str, sha256: &str| {
                format!(
                    "\nFROM base AS casulo-{stage}\n\
                     ADD --checksum=sha256:{sha256} --chmod=755 {RELEASES_URL}/v{version}/casulo-{target} /usr/local/bin/casulo\n"
                )
            };
            // BuildKit sets TARGETARCH to the daemon's architecture, so only
            // the stage of that architecture is built.
            format!(
                "{}{}\nFROM casulo-${{TARGETARCH}}\n",
                add("amd64", "x86_64-unknown-linux-musl", sha256_x86_64),
                add("arm64", "aarch64-unknown-linux-musl", sha256_aarch64),
            )
        }
        ImageSource::Source { .. } => "\nFROM rust:1-slim-bookworm AS builder\n\
             WORKDIR /src\n\
             COPY . .\n\
             RUN cargo build --release --locked --bin casulo\n\
             \n\
             FROM base\n\
             COPY --from=builder /src/target/release/casulo /usr/local/bin/casulo\n"
            .to_string(),
    };
    format!(
        "# syntax=docker/dockerfile:1\n\
         # Generated by casulo for the memory server. `casulo build` rewrites this file.\n\
         {base}{binary}\n\
         USER {CONTAINER_USER}\n"
    )
}

/// Writes the memory image's Dockerfile and builds it. The build context
/// is casulo's source for a development build, else an empty directory.
/// Returns the image tag.
pub fn build_image(
    home: &CasuloHome,
    source: &ImageSource,
    runtime: &dyn Images,
) -> Result<String> {
    let build_dir = home.memory_build_dir();
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("cannot create {}", build_dir.display()))?;
    let dockerfile_path = build_dir.join("Dockerfile");
    fs::write(&dockerfile_path, dockerfile(source))
        .with_context(|| format!("cannot write {}", dockerfile_path.display()))?;
    let context_dir = match source {
        ImageSource::Release { .. } => build_dir,
        ImageSource::Source { source_dir } => source_dir.clone(),
    };
    let tag = image_tag(source);
    runtime.build_image(&dockerfile_path, &context_dir, &tag)?;
    Ok(tag)
}

/// The memory container of `project`: detached, on the internal memory
/// network as `memory`, with the same hardening as agent containers,
/// `memory/` read-write and the workspace read-only.
pub fn memory_spec(
    home: &CasuloHome,
    project: &ProjectName,
    workspace: PathBuf,
    user: Option<HostUser>,
    image: String,
) -> RunSpec {
    let name = memory_name(project);
    let value = |name: &str, value: &str| EnvironmentVariable::Value {
        name: name.to_string(),
        value: value.to_string(),
    };
    RunSpec {
        image,
        command: [
            "casulo",
            "memory",
            "serve",
            "--dir",
            CONTAINER_MEMORY_DIR,
            "--workspace",
            CONTAINER_WORKSPACE,
            "--host",
            "0.0.0.0",
            "--port",
            &MEMORY_PORT.to_string(),
        ]
        .map(str::to_string)
        .to_vec(),
        workspace,
        secret_values: Vec::new(),
        workspace_read_only: true,
        mounts: vec![Mount {
            source: home.memory_dir(project),
            target: CONTAINER_MEMORY_DIR.to_string(),
            read_only: false,
        }],
        environment: vec![
            value("HOME", CONTAINER_HOME),
            // The workspace belongs to the host user, and git refuses to read
            // a repository owned by someone else unless told to trust it.
            value("GIT_CONFIG_COUNT", "1"),
            value("GIT_CONFIG_KEY_0", "safe.directory"),
            value("GIT_CONFIG_VALUE_0", CONTAINER_WORKSPACE),
            // The workspace is read-only: git must not try to refresh its index.
            value("GIT_OPTIONAL_LOCKS", "0"),
        ],
        user,
        terminal: false,
        network: Some(name.clone()),
        network_alias: Some(MEMORY_HOST.to_string()),
        extra_networks: Vec::new(),
        name: Some(name),
        detach: true,
    }
}

/// The commands `start` would run, for `--dry-run`.
pub fn start_command_lines(runtime: &dyn Containers, spec: &RunSpec) -> Vec<CommandLine> {
    let network = spec.network.as_deref().unwrap_or_default();
    let mut lines = vec![runtime.network_create_command_line(network, true)];
    lines.extend(runtime.run_command_lines(spec));
    lines
}

/// Whether `start` found the memory container already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStart {
    /// This run started it.
    Started,
    /// Another session started it; this one shares it.
    AlreadyRunning,
}

/// Ensures the internal memory network exists and the memory container
/// described by `spec` is running on it.
pub fn start(runtime: &dyn Containers, spec: &RunSpec) -> Result<MemoryStart> {
    let (Some(network), Some(name)) = (&spec.network, &spec.name) else {
        bail!("the memory container needs a network and a name");
    };
    runtime.ensure_network(network, true)?;
    if runtime.container_running(name)? {
        return Ok(MemoryStart::AlreadyRunning);
    }
    match runtime.start_detached(spec)? {
        DetachedStart::Started => Ok(MemoryStart::Started),
        // Another `casulo run` created it between the check and the start.
        DetachedStart::NameTaken => Ok(MemoryStart::AlreadyRunning),
    }
}

/// Stops the memory container of `project` when no other agent is left on
/// its network. `finished` is the agent container of the session that just
/// ended: it may still be listed for a moment while Docker removes it.
/// Returns whether it stopped memory.
pub fn stop_if_last(
    runtime: &dyn Containers,
    project: &ProjectName,
    finished: Option<&str>,
) -> Result<bool> {
    let name = memory_name(project);
    let containers = runtime.containers_on_network(&name)?;
    let others_left = containers
        .iter()
        .any(|other| *other != name && Some(other.as_str()) != finished);
    if !containers.contains(&name) || others_left {
        return Ok(false);
    }
    runtime.stop_container(&name)?;
    Ok(true)
}

/// The `memory` MCP server for `harness`. Each harness has its own URL, so
/// the server knows who wrote a note without trusting the agent. With
/// `with_session`, each request names the run's session, from
/// `CASULO_SESSION`.
pub fn mcp_server(harness: &HarnessName, with_session: bool) -> McpServer {
    let env_headers = if with_session {
        vec![(SESSION_HEADER.to_string(), SESSION_VARIABLE.to_string())]
    } else {
        Vec::new()
    };
    McpServer {
        name: ResourceName::parse(MCP_SERVER_NAME).expect("\"memory\" is a valid name"),
        transport: McpTransport::Url {
            url: format!("http://{MEMORY_HOST}:{MEMORY_PORT}/mcp/{harness}"),
            token_env: None,
            env_headers,
        },
    }
}

/// Adds the `memory` MCP server to the servers given to the agent.
/// `with_session` as for `mcp_server`.
pub fn add_mcp_server(
    setup: &mut AgentSetup,
    harness: &HarnessName,
    with_session: bool,
) -> Result<()> {
    if setup
        .mcp_servers
        .iter()
        .any(|server| server.name.as_str() == MCP_SERVER_NAME)
    {
        bail!(
            "an MCP server named \"{MCP_SERVER_NAME}\" in agent/mcp/ conflicts with casulo memory; rename it with `casulo delete mcp {MCP_SERVER_NAME}` and `casulo add mcp <name>`"
        );
    }
    setup.mcp_servers.push(mcp_server(harness, with_session));
    setup
        .mcp_servers
        .sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    Ok(())
}

/// Gives the agent memory's session hook: a script that relays each hook to
/// the memory server of `harness`, which follows the session and reminds
/// the agent once to save what it learned.
pub fn add_session_hook(setup: &mut AgentSetup, harness: &HarnessName) {
    let url = format!("http://{MEMORY_HOST}:{MEMORY_PORT}/hooks/{harness}");
    setup.session_hook = Some(SessionHook {
        script_name: HOOK_SCRIPT_NAME.to_string(),
        // A harness name is lowercase letters, digits and `-`, so the URL
        // never needs quoting in the script's string.
        script: HOOK_SCRIPT.replace("__URL__", &url),
        events: vec![SessionEvent::Start, SessionEvent::Stop, SessionEvent::End],
    });
}

/// The variable that names the session for the memory server, and its
/// value for this run.
pub fn session_variable(session_id: u32) -> EnvironmentVariable {
    EnvironmentVariable::Value {
        name: SESSION_VARIABLE.to_string(),
        value: session_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::path::Path;

    use super::*;
    use crate::infrastructure::docker::Docker;

    fn demo() -> ProjectName {
        ProjectName::parse("demo").unwrap()
    }

    fn home() -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from("/home/me/.casulo")), None).unwrap()
    }

    fn spec() -> RunSpec {
        memory_spec(
            &home(),
            &demo(),
            PathBuf::from("/home/me/code/demo"),
            Some(HostUser {
                uid: 1001,
                gid: 1002,
            }),
            "casulo/memory:dev".to_string(),
        )
    }

    /// A runtime that keeps running containers and their networks in memory.
    #[derive(Default)]
    struct FakeRuntime {
        /// Running containers and the network each is on.
        running: RefCell<Vec<(String, String)>>,
        /// What the runtime was asked to do, in order.
        calls: RefCell<Vec<String>>,
        /// Another session starts the container between the check and the start.
        race: bool,
    }

    impl FakeRuntime {
        fn with_running(containers: &[(&str, &str)]) -> Self {
            Self {
                running: RefCell::new(
                    containers
                        .iter()
                        .map(|(name, network)| (name.to_string(), network.to_string()))
                        .collect(),
                ),
                ..Self::default()
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Images for FakeRuntime {
        fn build_image(&self, dockerfile: &Path, context_dir: &Path, tag: &str) -> Result<()> {
            self.calls.borrow_mut().push(format!(
                "build {tag} {} {}",
                dockerfile.display(),
                context_dir.display()
            ));
            Ok(())
        }
        fn image_exists(&self, _tag: &str) -> Result<bool> {
            bail!("unused")
        }
    }

    impl Containers for FakeRuntime {
        fn run_command_lines(&self, _spec: &RunSpec) -> Vec<CommandLine> {
            Vec::new()
        }
        fn run(&self, _spec: &RunSpec) -> Result<i32> {
            bail!("unused")
        }
        fn start_detached(&self, spec: &RunSpec) -> Result<DetachedStart> {
            let name = spec.name.clone().unwrap_or_default();
            self.calls.borrow_mut().push(format!("start {name}"));
            if self.race {
                return Ok(DetachedStart::NameTaken);
            }
            let network = spec.network.clone().unwrap_or_default();
            self.running.borrow_mut().push((name, network));
            Ok(DetachedStart::Started)
        }
        fn container_running(&self, name: &str) -> Result<bool> {
            Ok(self.running.borrow().iter().any(|(other, _)| other == name))
        }
        fn containers_on_network(&self, network: &str) -> Result<Vec<String>> {
            Ok(self
                .running
                .borrow()
                .iter()
                .filter(|(_, other)| other == network)
                .map(|(name, _)| name.clone())
                .collect())
        }
        fn stop_container(&self, name: &str) -> Result<()> {
            self.calls.borrow_mut().push(format!("stop {name}"));
            self.running.borrow_mut().retain(|(other, _)| other != name);
            Ok(())
        }
        fn network_create_command_line(&self, _network: &str, _internal: bool) -> CommandLine {
            CommandLine {
                program: "fake".to_string(),
                args: Vec::new(),
            }
        }
        fn ensure_network(&self, network: &str, internal: bool) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("network {network} internal={internal}"));
            Ok(())
        }
    }

    #[test]
    fn first_session_creates_the_internal_network_and_starts_memory() {
        let runtime = FakeRuntime::default();

        let outcome = start(&runtime, &spec()).unwrap();

        assert_eq!(outcome, MemoryStart::Started);
        assert_eq!(
            runtime.calls(),
            [
                "network casulo-memory-demo internal=true",
                "start casulo-memory-demo"
            ]
        );
    }

    #[test]
    fn second_session_reuses_the_running_container() {
        let runtime = FakeRuntime::with_running(&[("casulo-memory-demo", "casulo-memory-demo")]);

        let outcome = start(&runtime, &spec()).unwrap();

        assert_eq!(outcome, MemoryStart::AlreadyRunning);
        assert!(!runtime.calls().iter().any(|call| call.starts_with("start")));
    }

    #[test]
    fn two_sessions_starting_at_once_share_one_container() {
        let runtime = FakeRuntime {
            race: true,
            ..FakeRuntime::default()
        };

        let outcome = start(&runtime, &spec()).unwrap();

        assert_eq!(outcome, MemoryStart::AlreadyRunning);
    }

    #[test]
    fn last_one_out_stops_memory() {
        let runtime = FakeRuntime::with_running(&[("casulo-memory-demo", "casulo-memory-demo")]);

        assert!(stop_if_last(&runtime, &demo(), None).unwrap());
        assert_eq!(runtime.calls(), ["stop casulo-memory-demo"]);
    }

    #[test]
    fn the_session_that_just_ended_does_not_count() {
        // Docker may still list the agent container while it removes it.
        let runtime = FakeRuntime::with_running(&[
            ("casulo-memory-demo", "casulo-memory-demo"),
            ("casulo-demo-claude-7", "casulo-memory-demo"),
        ]);

        assert!(stop_if_last(&runtime, &demo(), Some("casulo-demo-claude-7")).unwrap());
    }

    #[test]
    fn memory_keeps_running_while_another_agent_uses_it() {
        let runtime = FakeRuntime::with_running(&[
            ("casulo-memory-demo", "casulo-memory-demo"),
            ("casulo-demo-codex-7", "casulo-memory-demo"),
        ]);

        assert!(!stop_if_last(&runtime, &demo(), Some("casulo-demo-claude-7")).unwrap());
        assert!(runtime.calls().is_empty());
    }

    #[test]
    fn nothing_to_stop_when_memory_is_not_running() {
        let runtime = FakeRuntime::default();

        assert!(!stop_if_last(&runtime, &demo(), None).unwrap());
    }

    #[test]
    fn memory_container_command() {
        let line = Docker::new("docker").run_command_lines(&spec()).remove(0);

        // Raw arguments with `/` separators, so the snapshot is the same on every OS.
        insta::assert_snapshot!(line.args.join(" ").replace('\\', "/"));
    }

    #[test]
    fn dry_run_shows_the_network_first() {
        let lines = start_command_lines(&Docker::new("docker"), &spec());

        assert_eq!(
            lines[0].display(&[]),
            "docker network create --internal casulo-memory-demo"
        );
        assert_eq!(lines[1].args[0], "run");
    }

    #[test]
    fn agents_never_mount_memory() {
        let spec = spec();

        // Only the memory container mounts memory/, and it runs detached.
        assert!(spec.detach);
        assert!(spec.workspace_read_only);
        assert_eq!(spec.mounts[0].source, home().memory_dir(&demo()));
    }

    #[test]
    fn each_harness_gets_its_own_url() {
        let claude = HarnessName::parse("claude").unwrap();

        let server = mcp_server(&claude, false);

        assert_eq!(
            server.transport,
            McpTransport::Url {
                url: "http://memory:7373/mcp/claude".to_string(),
                token_env: None,
                env_headers: Vec::new(),
            }
        );
    }

    #[test]
    fn mcp_config_with_memory() {
        let claude = HarnessName::parse("claude").unwrap();
        let codex = HarnessName::parse("codex").unwrap();

        insta::assert_snapshot!(
            "claude_mcp_config_with_memory",
            crate::application::agent::domain::mcp_server::claude_mcp_config(&[mcp_server(
                &claude, false
            )])
        );
        // Codex gets no session header, so its arguments stay as they were.
        assert_eq!(
            crate::application::agent::domain::mcp_server::codex_config_overrides(&[mcp_server(
                &codex, true
            )]),
            [
                "-c",
                "mcp_servers.memory.url=\"http://memory:7373/mcp/codex\""
            ]
        );
    }

    #[test]
    fn with_a_session_claude_sends_it_in_a_header() {
        // Given the memory server for a run with session hooks
        let claude = HarnessName::parse("claude").unwrap();

        // When Claude Code's MCP config is rendered
        let json = crate::application::agent::domain::mcp_server::claude_mcp_config(&[mcp_server(
            &claude, true,
        )]);

        // Then the session comes from the container's variable
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["mcpServers"]["memory"]["headers"]["X-Casulo-Session"],
            "${CASULO_SESSION}"
        );
    }

    #[test]
    fn the_session_hook_calls_the_harness_url() {
        // Given an agent setup
        let mut setup = AgentSetup::default();

        // When memory's session hook is added for claude
        add_session_hook(&mut setup, &HarnessName::parse("claude").unwrap());

        // Then the script posts to claude's hook URL, at the three events
        let hook = setup.session_hook.unwrap();
        assert!(hook.script.contains("\"http://memory:7373/hooks/claude\""));
        assert!(!hook.script.contains("__URL__"));
        assert_eq!(
            hook.events,
            [SessionEvent::Start, SessionEvent::Stop, SessionEvent::End]
        );
    }

    #[test]
    fn a_user_server_named_memory_is_a_conflict() {
        let claude = HarnessName::parse("claude").unwrap();
        let mut setup = AgentSetup {
            subagents: Vec::new(),
            mcp_servers: vec![mcp_server(&claude, false)],
            session_hook: None,
        };

        let error = add_mcp_server(&mut setup, &claude, false).unwrap_err();

        assert!(error.to_string().contains("conflicts with casulo memory"));
    }

    #[test]
    fn release_image_downloads_a_checked_binary() {
        let source = ImageSource::Release {
            version: "0.3.0".to_string(),
            sha256_x86_64: "a".repeat(64),
            sha256_aarch64: "b".repeat(64),
        };

        assert_eq!(image_tag(&source), "casulo/memory:0.3.0");
        insta::assert_snapshot!(dockerfile(&source));
    }

    #[test]
    fn development_image_compiles_casulo_from_source() {
        let source = ImageSource::Source {
            source_dir: PathBuf::from("/src/casulo"),
        };

        assert_eq!(image_tag(&source), "casulo/memory:dev");
        insta::assert_snapshot!(dockerfile(&source));
    }

    #[test]
    fn development_build_uses_the_source_as_context() {
        let directory = tempfile::tempdir().unwrap();
        let home = CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap();
        let source_dir = directory.path().join("source");
        let runtime = FakeRuntime::default();

        build_image(
            &home,
            &ImageSource::Source {
                source_dir: source_dir.clone(),
            },
            &runtime,
        )
        .unwrap();

        let dockerfile = home.memory_build_dir().join("Dockerfile");
        assert!(dockerfile.is_file());
        assert_eq!(
            runtime.calls(),
            [format!(
                "build casulo/memory:dev {} {}",
                dockerfile.display(),
                source_dir.display()
            )]
        );
    }
}
