//! `casulo run` and `casulo shell`: everything decided before a container
//! starts. `plan` reads files under `~/.casulo` but starts nothing, so every
//! rule here, such as the order of the mounts, is tested without Docker.

use anyhow::Result;

use crate::application::agent::domain::Harness;
use crate::application::agent::services::workflows;
use crate::application::project::domain::config::{choose_model_settings, memory_enabled};
use crate::application::project::domain::push::{
    PushPolicy, TOKEN_VARIABLE, choose_identity, choose_push,
};
use crate::application::project::domain::{GlobalConfig, ProjectConfig};
use crate::application::sandbox::domain::{EnvironmentVariable, HostUser, RunSpec};
use crate::application::sandbox::services::inject::{self, AgentSetup};
use crate::application::sandbox::services::{mask, memory_container, push, run};
use crate::application::shared::domain::{CasuloHome, ProjectName, ResourceName};

/// What `plan` needs, already loaded and checked.
pub struct RunRequest {
    /// The casulo home.
    pub home: CasuloHome,
    /// The project to run.
    pub project: ProjectName,
    /// Its `casulo.toml`.
    pub project_config: ProjectConfig,
    /// The global `casulo.toml`.
    pub global: GlobalConfig,
    /// The agent to run.
    pub harness: Harness,
    /// The user's subagents and MCP servers.
    pub setup: AgentSetup,
    /// Who owns the workspace, to run as on Linux.
    pub workspace_owner: Option<HostUser>,
    /// A command to run instead of the agent, such as `bash` for `casulo shell`.
    pub command: Option<Vec<String>>,
    /// The network of the project's compose services, when they run.
    pub compose_network: Option<String>,
    /// Whether a terminal is attached to casulo.
    pub terminal: bool,
    /// Makes the agent container's name unique, such as casulo's process id.
    pub session_id: u32,
    /// The memory image, used when memory is on.
    pub memory_image: String,
}

/// The containers to start and what they need.
#[derive(Debug, Clone, PartialEq)]
pub struct RunPlan {
    /// The agent container.
    pub spec: RunSpec,
    /// The memory container, when memory is on for this run.
    pub memory_spec: Option<RunSpec>,
    /// The subagents and MCP servers given to the agent, memory's included.
    pub setup: AgentSetup,
    /// Push settings, when the agent may push.
    pub push_policy: Option<PushPolicy>,
    /// Names of every secret variable passed to the agent, by name only.
    pub secret_names: Vec<String>,
    /// Things the user should know; the run goes on.
    pub warnings: Vec<String>,
}

/// Decides the containers of a run. `is_set_on_host` says whether the host
/// sets a variable, so a secret exported in the shell wins over `[secrets]`.
pub fn plan(request: RunRequest, is_set_on_host: &dyn Fn(&str) -> bool) -> Result<RunPlan> {
    let RunRequest {
        home,
        project,
        project_config,
        global,
        harness,
        mut setup,
        workspace_owner,
        command,
        compose_network,
        terminal,
        session_id,
        memory_image,
    } = request;
    // A broken workflow must not stop an unrelated run, so it only warns.
    let agent_names: Vec<ResourceName> = setup
        .subagents
        .iter()
        .map(|subagent| subagent.name.clone())
        .collect();
    let mut warnings = workflows::check_skills(&home, &agent_names);
    let workspace = project_config.workspace_dir(&home, &project);

    // Memory reaches the agent as an MCP server, so a harness that cannot
    // receive one runs without it.
    let mut use_memory = memory_enabled(&project_config.memory, &global.memory);
    if use_memory && harness.inject.mcp.is_none() {
        warnings.push(format!(
            "harness \"{}\" cannot receive MCP servers, so memory is off for this run",
            harness.name
        ));
        use_memory = false;
    }
    // Session hooks let memory remind the agent to save what it learned;
    // a harness that cannot take hooks runs with memory as before.
    let session_hooks = use_memory && harness.inject.hooks.is_some();
    if use_memory {
        memory_container::add_mcp_server(&mut setup, &harness.name, session_hooks)?;
    }
    if session_hooks {
        memory_container::add_session_hook(&mut setup, &harness.name);
    }

    let push_policy = choose_push(
        &project_config.git,
        &global.git,
        project_config.repository.as_ref(),
    )?;
    // Order matters: a mount nested in another one must come after it, and
    // the masks come last so nothing can mount over them.
    let mounts = [
        run::config_mount(&home, &project, &harness)
            .into_iter()
            .collect(),
        inject::inject_mounts(&home, &project, &harness, &setup),
        push::push_mounts(&home, &project, push_policy.as_ref()),
        mask::mask_mounts(&home, &project, &workspace, &project_config.mask),
    ]
    .concat();

    let command = match command {
        Some(command) => command,
        None => {
            let settings = choose_model_settings(&harness.name, &project_config, &global);
            let (mut command, setting_warnings) = run::agent_command(&harness, &settings);
            warnings.extend(setting_warnings);
            command.extend(inject::mcp_arguments(&harness, &setup));
            command.extend(inject::hook_arguments(&harness, &setup));
            command
        }
    };
    let mut spec = run::run_spec(
        workspace,
        &project,
        &harness,
        &choose_identity(&project_config.git, &global.git),
        workspace_owner,
        command,
        mounts,
    );
    spec.network = compose_network;
    spec.terminal = terminal;
    if session_hooks {
        spec.environment
            .push(memory_container::session_variable(session_id));
    }

    // MCP servers and the push credential helper read their secrets from the
    // container environment, passed by name only like the harness's own.
    let mut extra_secrets = inject::mcp_environment_names(&harness, &setup);
    if push_policy.is_some() {
        extra_secrets.push(TOKEN_VARIABLE.to_string());
    }
    for name in &extra_secrets {
        spec.environment
            .push(EnvironmentVariable::FromHost { name: name.clone() });
    }
    let secret_names = [harness.auth_env.clone(), extra_secrets].concat();
    let set_on_host: Vec<String> = secret_names
        .iter()
        .filter(|name| is_set_on_host(name))
        .cloned()
        .collect();
    // The push token from `[git]` joins the values of `[secrets]`, so it
    // reaches the container the same way: in the environment, never in an
    // argument, and only when the host does not set the variable itself.
    let mut config_values = global.secrets.clone();
    if let Some(token) = push_policy.as_ref().and_then(|policy| policy.token.clone()) {
        config_values.insert(TOKEN_VARIABLE.to_string(), token);
    }
    spec.secret_values = run::config_secrets(&secret_names, &set_on_host, &config_values);
    if push_policy.is_some()
        && !set_on_host.iter().any(|name| name == TOKEN_VARIABLE)
        && !config_values.contains_key(TOKEN_VARIABLE)
    {
        warnings.push(format!(
            "push is on but there is no token: set token under [git] in casulo.toml, or export {TOKEN_VARIABLE}"
        ));
    }

    let memory_spec = if use_memory {
        // The session id makes the name unique, so sessions of one project
        // can run side by side.
        spec.name = Some(format!("casulo-{project}-{}-{session_id}", harness.name));
        spec.extra_networks = vec![memory_container::memory_name(&project)];
        Some(memory_container::memory_spec(
            &home,
            &project,
            spec.workspace.clone(),
            workspace_owner,
            memory_image,
        ))
    } else {
        None
    };

    Ok(RunPlan {
        spec,
        memory_spec,
        setup,
        push_policy,
        secret_names,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::application::sandbox::test_support::built_in_harness;
    use crate::application::shared::domain::Secret;

    fn request(project_toml: &str, harness: Harness) -> RunRequest {
        let home = CasuloHome::resolve(Some(OsString::from("home")), None).unwrap();
        let project_config: ProjectConfig = toml::from_str(&format!(
            "repository = \"https://example.com/demo.git\"\n{project_toml}"
        ))
        .unwrap();
        RunRequest {
            home,
            project: ProjectName::parse("demo").unwrap(),
            project_config,
            global: GlobalConfig::default(),
            harness,
            setup: AgentSetup::default(),
            workspace_owner: None,
            command: None,
            compose_network: None,
            terminal: true,
            session_id: 42,
            memory_image: "casulo/memory:dev".to_string(),
        }
    }

    fn nobody_sets(_name: &str) -> bool {
        false
    }

    #[test]
    fn masks_are_mounted_last() {
        let directory = tempfile::tempdir().unwrap();
        let mut masked = request("mask = [\".env\"]\n", built_in_harness("claude"));
        masked.home = CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap();
        let workspace = masked.home.workspace_dir(&masked.project);
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join(".env"), "TOKEN=secret\n").unwrap();

        let plan = plan(masked, &nobody_sets).unwrap();

        // A mask under another mount, such as the agent config, would be
        // hidden if that mount came after it.
        assert!(plan.spec.mounts.len() > 1);
        let last = plan.spec.mounts.last().unwrap();
        assert_eq!(last.target, "/workspace/.env");
        assert!(last.read_only);
    }

    #[test]
    fn memory_joins_its_network_and_adds_its_server() {
        let plan = plan(
            request("[memory]\nenabled = true\n", built_in_harness("claude")),
            &nobody_sets,
        )
        .unwrap();

        assert_eq!(plan.spec.name.as_deref(), Some("casulo-demo-claude-42"));
        assert_eq!(plan.spec.extra_networks, ["casulo-memory-demo"]);
        assert_eq!(plan.setup.mcp_servers[0].name.as_str(), "memory");
        let memory = plan.memory_spec.unwrap();
        assert_eq!(memory.image, "casulo/memory:dev");
        // Only the memory container mounts memory/.
        assert!(memory.mounts.iter().any(|mount| mount.target == "/memory"));
        assert!(
            plan.spec
                .mounts
                .iter()
                .all(|mount| mount.target != "/memory")
        );
    }

    fn session_variable_of(spec: &RunSpec) -> Option<String> {
        spec.environment.iter().find_map(|variable| match variable {
            EnvironmentVariable::Value { name, value } if name == "CASULO_SESSION" => {
                Some(value.clone())
            }
            _ => None,
        })
    }

    #[test]
    fn with_memory_claude_gets_the_session_and_its_hooks() {
        // Given memory on, with claude, which declares hooks
        let request = request("[memory]\nenabled = true\n", built_in_harness("claude"));

        // When the run is planned
        let plan = plan(request, &nobody_sets).unwrap();

        // Then the agent knows its session and loads the hook settings
        assert_eq!(session_variable_of(&plan.spec).as_deref(), Some("42"));
        assert!(plan.setup.session_hook.is_some());
        assert!(plan.spec.command.ends_with(&[
            "--settings".to_string(),
            "/home/casulo/.casulo/settings.json".to_string()
        ]));
    }

    #[test]
    fn with_memory_codex_gets_no_session() {
        // Given memory on, with codex, which declares no hooks
        let request = request("[memory]\nenabled = true\n", built_in_harness("codex"));

        // When the run is planned
        let plan = plan(request, &nobody_sets).unwrap();

        // Then it runs with memory as before: no session, hook or settings
        assert_eq!(session_variable_of(&plan.spec), None);
        assert!(plan.setup.session_hook.is_none());
        assert!(!plan.spec.command.contains(&"--settings".to_string()));
    }

    #[test]
    fn without_memory_claude_gets_no_session() {
        // Given memory off, with claude
        let request = request("", built_in_harness("claude"));

        // When the run is planned
        let plan = plan(request, &nobody_sets).unwrap();

        // Then there is no session, hook or settings
        assert_eq!(session_variable_of(&plan.spec), None);
        assert!(plan.setup.session_hook.is_none());
        assert!(!plan.spec.command.contains(&"--settings".to_string()));
    }

    #[test]
    fn memory_is_off_for_a_harness_without_mcp() {
        let mut no_mcp = built_in_harness("claude");
        no_mcp.inject.mcp = None;

        let plan = plan(request("[memory]\nenabled = true\n", no_mcp), &nobody_sets).unwrap();

        assert!(plan.memory_spec.is_none());
        assert!(plan.spec.extra_networks.is_empty());
        assert!(plan.warnings[0].contains("memory is off for this run"));
    }

    #[test]
    fn without_memory_nothing_joins_its_network() {
        let plan = plan(request("", built_in_harness("claude")), &nobody_sets).unwrap();

        assert!(plan.memory_spec.is_none());
        assert_eq!(plan.spec.name, None);
        assert!(plan.setup.mcp_servers.is_empty());
    }

    #[test]
    fn a_secret_set_on_the_host_wins_over_the_config() {
        let mut both = request("", built_in_harness("claude"));
        both.global
            .secrets
            .insert("ANTHROPIC_API_KEY".to_string(), Secret::new("from-config"));
        let mut config_only = request("", built_in_harness("claude"));
        config_only.global.secrets = both.global.secrets.clone();

        let host_wins = plan(both, &|name| name == "ANTHROPIC_API_KEY").unwrap();
        let config_used = plan(config_only, &nobody_sets).unwrap();

        assert!(host_wins.spec.secret_values.is_empty());
        assert_eq!(config_used.spec.secret_values[0].0, "ANTHROPIC_API_KEY");
        assert!(
            config_used
                .secret_names
                .contains(&"ANTHROPIC_API_KEY".to_string())
        );
    }

    #[test]
    fn the_push_token_from_git_reaches_the_container_by_value() {
        let with_token = request(
            "[git]\npush = true\ntoken = \"github_pat_value\"\n",
            built_in_harness("claude"),
        );

        let plan = plan(with_token, &nobody_sets).unwrap();

        let (name, token) = plan
            .spec
            .secret_values
            .iter()
            .find(|(name, _)| name == "CASULO_GIT_TOKEN")
            .expect("the token is passed to the container");
        assert_eq!(
            (name.as_str(), token.expose()),
            ("CASULO_GIT_TOKEN", "github_pat_value")
        );
        // By name only on the command line: the value goes in the environment.
        assert!(
            plan.spec
                .environment
                .contains(&EnvironmentVariable::FromHost {
                    name: "CASULO_GIT_TOKEN".to_string()
                })
        );
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    }

    #[test]
    fn a_push_token_on_the_host_wins_over_the_config() {
        let with_token = request(
            "[git]\npush = true\ntoken = \"github_pat_value\"\n",
            built_in_harness("claude"),
        );

        let plan = plan(with_token, &|name| name == "CASULO_GIT_TOKEN").unwrap();

        assert!(
            plan.spec
                .secret_values
                .iter()
                .all(|(name, _)| name != "CASULO_GIT_TOKEN")
        );
    }

    #[test]
    fn push_without_any_token_warns() {
        let plan = plan(
            request("[git]\npush = true\n", built_in_harness("claude")),
            &nobody_sets,
        )
        .unwrap();

        assert!(
            plan.warnings[0].contains("there is no token"),
            "{:?}",
            plan.warnings
        );
    }

    #[test]
    fn the_project_identity_reaches_the_container() {
        let mut project = request(
            "[git]\nemail = \"ada@work.example\"\n",
            built_in_harness("claude"),
        );
        project.global.git.name = Some("Ada".to_string());

        let plan = plan(project, &nobody_sets).unwrap();

        for (name, expected) in [
            ("GIT_AUTHOR_NAME", "Ada"),
            ("GIT_AUTHOR_EMAIL", "ada@work.example"),
        ] {
            assert!(
                plan.spec.environment.contains(&EnvironmentVariable::Value {
                    name: name.to_string(),
                    value: expected.to_string()
                }),
                "{name}"
            );
        }
    }

    #[test]
    fn a_given_command_replaces_the_agent() {
        let mut shell = request("", built_in_harness("claude"));
        shell.command = Some(vec!["bash".to_string()]);
        shell.compose_network = Some("casulo-demo_default".to_string());

        let plan = plan(shell, &nobody_sets).unwrap();

        assert_eq!(plan.spec.command, ["bash"]);
        assert_eq!(plan.spec.network.as_deref(), Some("casulo-demo_default"));
    }
}
