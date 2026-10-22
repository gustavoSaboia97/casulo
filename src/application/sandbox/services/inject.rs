//! Give the user's agent setup from `~/.casulo/agent` to the harness that runs.

use std::path::PathBuf;

use anyhow::Result;

use crate::application::agent::domain::harness::{HooksMode, McpMode};
use crate::application::agent::domain::hooks::{self, SessionEvent};
use crate::application::agent::domain::mcp_server;
use crate::application::agent::domain::{Harness, McpServer, Subagent};
use crate::application::sandbox::domain::Mount;
use crate::application::sandbox::domain::container::CONTAINER_HOME;
use crate::application::sandbox::services::replace_files::replace_files;
use crate::application::shared::domain::{CasuloHome, ProjectName};

/// The parts of `~/.casulo/agent` that are parsed, not mounted as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSetup {
    /// Subagents from `agent/agents/`, sorted by name.
    pub subagents: Vec<Subagent>,
    /// MCP servers from `agent/mcp/`, sorted by name.
    pub mcp_servers: Vec<McpServer>,
    /// A hook run at session events, given by casulo, never by the user.
    pub session_hook: Option<SessionHook>,
}

/// A script the harness runs at some events of each session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHook {
    /// The script's file name in the container, such as `memory-hook.mjs`.
    pub script_name: String,
    /// The script, a Node module.
    pub script: String,
    /// When it runs.
    pub events: Vec<SessionEvent>,
}

/// Read-only mounts that put the user's agent setup where `harness` reads
/// it, as its manifest's `[inject]` declares. Pieces that do not exist on
/// the host, or that the harness does not declare, are skipped.
///
/// Read-only, because everything under `~/.casulo/agent` is trusted and the
/// agent must not be able to change its own instructions.
pub fn inject_mounts(
    home: &CasuloHome,
    project: &ProjectName,
    harness: &Harness,
    setup: &AgentSetup,
) -> Vec<Mount> {
    let mut mounts = Vec::new();
    let instructions = home.agent_instructions_file();
    if let Some(target) = &harness.inject.instructions
        && instructions.is_file()
    {
        mounts.push(Mount {
            source: instructions,
            target: format!("{CONTAINER_HOME}/{target}"),
            read_only: true,
        });
    }
    let skills = home.skills_dir();
    if let Some(target) = &harness.inject.skills
        && skills.is_dir()
    {
        mounts.push(Mount {
            source: skills,
            target: format!("{CONTAINER_HOME}/{target}"),
            read_only: true,
        });
    }
    if let Some(agents) = &harness.inject.agents
        && !setup.subagents.is_empty()
    {
        mounts.push(Mount {
            source: generated_agents_dir(home, project, harness),
            target: format!("{CONTAINER_HOME}/{}", agents.dir),
            read_only: true,
        });
    }
    if harness.inject.mcp == Some(McpMode::ClaudeMcpConfig) && !setup.mcp_servers.is_empty() {
        mounts.push(Mount {
            source: generated_mcp_config(home, project, harness),
            target: container_mcp_config(),
            read_only: true,
        });
    }
    if let Some(hook) = claude_session_hook(harness, setup) {
        let directory = home.generated_dir(project, &harness.name);
        mounts.push(Mount {
            source: directory.join(SETTINGS_FILE),
            target: container_casulo_file(SETTINGS_FILE),
            read_only: true,
        });
        mounts.push(Mount {
            source: directory.join(&hook.script_name),
            target: container_casulo_file(&hook.script_name),
            read_only: true,
        });
    }
    mounts
}

/// Renders the subagents and the MCP config in the harness's formats into
/// `generated/<harness>/`, replacing what an earlier run left there, so a
/// deleted subagent or server does not linger.
pub fn write_generated_files(
    home: &CasuloHome,
    project: &ProjectName,
    harness: &Harness,
    setup: &AgentSetup,
) -> Result<()> {
    let directory = home.generated_dir(project, &harness.name);
    let mut files = Vec::new();
    if let Some(agents) = &harness.inject.agents
        && !setup.subagents.is_empty()
    {
        let agents_dir = generated_agents_dir(home, project, harness);
        for subagent in &setup.subagents {
            let (file_name, text) = subagent.render(agents.format, &harness.name);
            files.push((agents_dir.join(file_name), text));
        }
    }
    if harness.inject.mcp == Some(McpMode::ClaudeMcpConfig) && !setup.mcp_servers.is_empty() {
        files.push((
            generated_mcp_config(home, project, harness),
            mcp_server::claude_mcp_config(&setup.mcp_servers),
        ));
    }
    if let Some(hook) = claude_session_hook(harness, setup) {
        let command = format!("node {}", container_casulo_file(&hook.script_name));
        files.push((
            directory.join(SETTINGS_FILE),
            hooks::claude_settings(&command, &hook.events),
        ));
        files.push((directory.join(&hook.script_name), hook.script.clone()));
    }
    replace_files(&directory, &files)
}

/// Arguments that give the session hooks to the agent, appended to its command.
pub fn hook_arguments(harness: &Harness, setup: &AgentSetup) -> Vec<String> {
    match claude_session_hook(harness, setup) {
        Some(_) => vec![
            "--settings".to_string(),
            container_casulo_file(SETTINGS_FILE),
        ],
        None => Vec::new(),
    }
}

/// The session hook, when there is one and `harness` takes hooks from a
/// Claude Code settings file, the only way there is so far.
fn claude_session_hook(harness: &Harness, setup: &AgentSetup) -> Option<SessionHook> {
    match harness.inject.hooks {
        Some(HooksMode::ClaudeSettings) => setup.session_hook.clone(),
        None => None,
    }
}

/// Arguments that give the MCP servers to the agent, appended to its command.
pub fn mcp_arguments(harness: &Harness, setup: &AgentSetup) -> Vec<String> {
    if setup.mcp_servers.is_empty() {
        return Vec::new();
    }
    match harness.inject.mcp {
        Some(McpMode::ClaudeMcpConfig) => {
            vec!["--mcp-config".to_string(), container_mcp_config()]
        }
        Some(McpMode::CodexConfigOverrides) => {
            mcp_server::codex_config_overrides(&setup.mcp_servers)
        }
        None => Vec::new(),
    }
}

/// Names of the host variables the MCP servers need, without repeats. They
/// are passed by name only, like `auth_env`.
pub fn mcp_environment_names(harness: &Harness, setup: &AgentSetup) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if harness.inject.mcp.is_none() {
        return names;
    }
    for name in setup
        .mcp_servers
        .iter()
        .flat_map(McpServer::environment_names)
    {
        if !names.contains(&name) && !harness.auth_env.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Where the generated MCP config for Claude Code is mounted in the container.
fn container_mcp_config() -> String {
    container_casulo_file("mcp.json")
}

/// The settings file with the session hooks, for Claude Code.
const SETTINGS_FILE: &str = "settings.json";

/// Where casulo's generated file `name` is mounted in the container.
fn container_casulo_file(name: &str) -> String {
    format!("{CONTAINER_HOME}/.casulo/{name}")
}

fn generated_agents_dir(home: &CasuloHome, project: &ProjectName, harness: &Harness) -> PathBuf {
    home.generated_dir(project, &harness.name).join("agents")
}

fn generated_mcp_config(home: &CasuloHome, project: &ProjectName, harness: &Harness) -> PathBuf {
    home.generated_dir(project, &harness.name).join("mcp.json")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use super::*;
    use crate::application::project::domain::GitIdentity;
    use crate::application::sandbox::ports::Containers;
    use crate::application::sandbox::services::run;
    use crate::application::sandbox::test_support::built_in_harness;
    use crate::application::shared::domain::ProjectName;
    use crate::infrastructure::docker::Docker;

    fn demo() -> ProjectName {
        ProjectName::parse("demo").unwrap()
    }

    fn home_in(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap()
    }

    fn write_instructions(home: &CasuloHome) {
        fs::create_dir_all(home.agent_dir()).unwrap();
        fs::write(home.agent_instructions_file(), "Answer in English.\n").unwrap();
    }

    /// The docker arguments for a run with these mounts, with the temporary
    /// home shown as `~/.casulo` and `/` separators, so the snapshot is the
    /// same on every OS.
    fn run_args(home: &CasuloHome, harness: &Harness, mounts: Vec<Mount>) -> String {
        let project = ProjectName::parse("demo").unwrap();
        let spec = run::run_spec(
            home.workspace_dir(&project),
            &project,
            harness,
            &GitIdentity::default(),
            None,
            harness.command.clone(),
            mounts,
        );
        let line = Docker::new("docker").run_command_lines(&spec).remove(0);
        line.args
            .join("\n")
            .replace(&home.root().display().to_string(), "~/.casulo")
            .replace('\\', "/")
    }

    #[test]
    fn run_args_without_instructions() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let claude = built_in_harness("claude");

        let mounts = inject_mounts(&home, &demo(), &claude, &AgentSetup::default());

        assert!(mounts.is_empty());
        insta::assert_snapshot!(run_args(&home, &claude, mounts));
    }

    #[test]
    fn run_args_with_instructions() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        write_instructions(&home);

        for name in ["claude", "codex"] {
            let harness = built_in_harness(name);
            let mounts = inject_mounts(&home, &demo(), &harness, &AgentSetup::default());
            insta::assert_snapshot!(
                format!("run_args_with_instructions_for_{name}"),
                run_args(&home, &harness, mounts)
            );
        }
    }

    #[test]
    fn run_args_with_instructions_and_config_dir() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        write_instructions(&home);
        let claude = built_in_harness("claude");
        let project = ProjectName::parse("demo").unwrap();

        // The instructions are mounted inside the `.claude` config mount, so
        // they must come after it.
        let mounts = [
            run::config_mount(&home, &project, &claude)
                .into_iter()
                .collect(),
            inject_mounts(&home, &demo(), &claude, &AgentSetup::default()),
        ]
        .concat();

        insta::assert_snapshot!(run_args(&home, &claude, mounts));
    }

    #[test]
    fn run_args_with_skills() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        fs::create_dir_all(home.skills_dir().join("release-notes")).unwrap();

        for name in ["claude", "codex"] {
            let harness = built_in_harness(name);
            let mounts = inject_mounts(&home, &demo(), &harness, &AgentSetup::default());
            insta::assert_snapshot!(
                format!("run_args_with_skills_for_{name}"),
                run_args(&home, &harness, mounts)
            );
        }
    }

    fn reviewer() -> AgentSetup {
        let text = "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems only.\n";
        AgentSetup {
            subagents: vec![Subagent::parse("reviewer", text).unwrap()],
            mcp_servers: Vec::new(),
            session_hook: None,
        }
    }

    #[test]
    fn run_args_with_subagents() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);

        for name in ["claude", "codex"] {
            let harness = built_in_harness(name);
            let mounts = inject_mounts(&home, &demo(), &harness, &reviewer());
            insta::assert_snapshot!(
                format!("run_args_with_subagents_for_{name}"),
                run_args(&home, &harness, mounts)
            );
        }
    }

    #[test]
    fn generated_subagents_replace_the_previous_run() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let claude = built_in_harness("claude");
        let agents_dir = home.generated_dir(&demo(), &claude.name).join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(agents_dir.join("deleted.md"), "old").unwrap();

        write_generated_files(&home, &demo(), &claude, &reviewer()).unwrap();

        assert!(!agents_dir.join("deleted.md").exists());
        assert!(
            fs::read_to_string(agents_dir.join("reviewer.md"))
                .unwrap()
                .starts_with("---\nname: \"reviewer\"")
        );
        let codex = built_in_harness("codex");
        write_generated_files(&home, &demo(), &codex, &reviewer()).unwrap();
        assert!(
            home.generated_dir(&demo(), &codex.name)
                .join("agents")
                .join("reviewer.toml")
                .is_file()
        );

        write_generated_files(&home, &demo(), &claude, &AgentSetup::default()).unwrap();
        assert!(!agents_dir.exists());
    }

    fn with_mcp_servers() -> AgentSetup {
        AgentSetup {
            subagents: Vec::new(),
            mcp_servers: vec![
                McpServer::parse(
                    "context7",
                    "command = [\"npx\", \"-y\", \"@upstash/context7-mcp\"]\nenv = [\"CONTEXT7_TOKEN\", \"ANTHROPIC_API_KEY\"]\n",
                )
                .unwrap(),
                McpServer::parse(
                    "github",
                    "url = \"https://api.githubcopilot.com/mcp/\"\ntoken_env = \"CONTEXT7_TOKEN\"\n",
                )
                .unwrap(),
            ],
            session_hook: None,
        }
    }

    fn with_session_hook() -> AgentSetup {
        AgentSetup {
            session_hook: Some(SessionHook {
                script_name: "memory-hook.mjs".to_string(),
                script: "// the hook\n".to_string(),
                events: vec![SessionEvent::Start, SessionEvent::Stop],
            }),
            ..AgentSetup::default()
        }
    }

    #[test]
    fn a_session_hook_is_written_mounted_and_passed_to_claude() {
        // Given a setup with a session hook
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let claude = built_in_harness("claude");
        let setup = with_session_hook();

        // When claude's files, mounts and arguments are made
        write_generated_files(&home, &demo(), &claude, &setup).unwrap();
        let mounts = inject_mounts(&home, &demo(), &claude, &setup);
        let arguments = hook_arguments(&claude, &setup);

        // Then the settings run the script, and both are mounted read-only
        let generated = home.generated_dir(&demo(), &claude.name);
        let settings = fs::read_to_string(generated.join("settings.json")).unwrap();
        assert!(settings.contains("\"command\": \"node /home/casulo/.casulo/memory-hook.mjs\""));
        assert_eq!(
            fs::read_to_string(generated.join("memory-hook.mjs")).unwrap(),
            "// the hook\n"
        );
        let targets: Vec<(&str, bool)> = mounts
            .iter()
            .map(|mount| (mount.target.as_str(), mount.read_only))
            .collect();
        assert_eq!(
            targets,
            [
                ("/home/casulo/.casulo/settings.json", true),
                ("/home/casulo/.casulo/memory-hook.mjs", true)
            ]
        );
        assert_eq!(
            arguments,
            ["--settings", "/home/casulo/.casulo/settings.json"]
        );
    }

    #[test]
    fn a_harness_without_hooks_gets_none_of_it() {
        // Given a setup with a session hook, and codex, which declares no hooks
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let codex = built_in_harness("codex");
        let setup = with_session_hook();

        // When codex's files, mounts and arguments are made
        write_generated_files(&home, &demo(), &codex, &setup).unwrap();
        let mounts = inject_mounts(&home, &demo(), &codex, &setup);
        let arguments = hook_arguments(&codex, &setup);

        // Then nothing is written, mounted or passed
        assert!(!home.generated_dir(&demo(), &codex.name).exists());
        assert!(mounts.is_empty());
        assert!(arguments.is_empty());
    }

    /// The run args with the MCP servers: mounts, variables and arguments.
    fn run_args_with_mcp(home: &CasuloHome, harness: &Harness, setup: &AgentSetup) -> String {
        let mut harness = harness.clone();
        harness.command.extend(mcp_arguments(&harness, setup));
        harness
            .auth_env
            .extend(mcp_environment_names(&harness, setup));
        run_args(
            home,
            &harness,
            inject_mounts(home, &demo(), &harness, setup),
        )
    }

    #[test]
    fn run_args_with_mcp_servers() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);

        for name in ["claude", "codex"] {
            let harness = built_in_harness(name);
            insta::assert_snapshot!(
                format!("run_args_with_mcp_servers_for_{name}"),
                run_args_with_mcp(&home, &harness, &with_mcp_servers())
            );
        }
    }

    #[test]
    fn mcp_variables_are_listed_once() {
        let claude = built_in_harness("claude");

        assert_eq!(
            mcp_environment_names(&claude, &with_mcp_servers()),
            ["CONTEXT7_TOKEN"]
        );
    }

    #[test]
    fn mcp_config_is_written_only_for_claude() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let claude = built_in_harness("claude");
        let codex = built_in_harness("codex");

        write_generated_files(&home, &demo(), &claude, &with_mcp_servers()).unwrap();
        write_generated_files(&home, &demo(), &codex, &with_mcp_servers()).unwrap();

        let claude_config = home.generated_dir(&demo(), &claude.name).join("mcp.json");
        assert!(
            fs::read_to_string(&claude_config)
                .unwrap()
                .contains("${CONTEXT7_TOKEN}")
        );
        assert!(!home.generated_dir(&demo(), &codex.name).exists());

        write_generated_files(&home, &demo(), &claude, &AgentSetup::default()).unwrap();
        assert!(!claude_config.exists());
    }

    #[test]
    fn harness_without_inject_gets_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        write_instructions(&home);
        fs::create_dir_all(home.skills_dir()).unwrap();
        let mut claude = built_in_harness("claude");
        claude.inject = Default::default();

        assert!(inject_mounts(&home, &demo(), &claude, &reviewer()).is_empty());
        assert!(mcp_arguments(&claude, &with_mcp_servers()).is_empty());
        assert!(mcp_environment_names(&claude, &with_mcp_servers()).is_empty());
    }
}
