//! `casulo run` and `casulo shell`: describe the agent container to start.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::agent::domain::Harness;
use crate::application::project::domain::{GitIdentity, ModelSettings};
use crate::application::sandbox::domain::container::{CONTAINER_HOME, image_tag};
use crate::application::sandbox::domain::{EnvironmentVariable, HostUser, Mount, RunSpec};
use crate::application::shared::domain::{CasuloHome, ProjectName, Secret};

/// The command that starts the agent: the harness program, then the model
/// and effort arguments, then the harness's own arguments
/// (`claude --model opus --effort high`). Also returns a warning for each
/// setting the harness has no arguments for, which is then ignored.
pub fn agent_command(harness: &Harness, settings: &ModelSettings) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut extra = Vec::new();
    for (value, arguments, placeholder, key) in [
        (&settings.model, &harness.model_args, "{model}", "model"),
        (&settings.effort, &harness.effort_args, "{effort}", "effort"),
    ] {
        let Some(value) = value else { continue };
        if arguments.is_empty() {
            warnings.push(format!(
                "harness \"{}\" has no way to set the {key}, so {key} = \"{value}\" is ignored",
                harness.name
            ));
            continue;
        }
        extra.extend(
            arguments
                .iter()
                .map(|argument| argument.replace(placeholder, value)),
        );
    }
    let mut command = harness.command.clone();
    // The manifest guarantees a program, so `1` is never past the end.
    command.splice(1..1, extra);
    (command, warnings)
}

/// The mount that keeps the agent's login, settings and history between
/// runs: `config/<harness>/` of this project on the host, at the harness's
/// `config_dir` in the container. `None` when the harness declares none.
pub fn config_mount(home: &CasuloHome, project: &ProjectName, harness: &Harness) -> Option<Mount> {
    let config_dir = harness.config_dir.as_ref()?;
    Some(Mount {
        source: home.agent_config_dir(project, &harness.name),
        target: format!("{CONTAINER_HOME}/{config_dir}"),
        read_only: false,
    })
}

/// Gets the config directory ready before a run: creates it, writes each of
/// the harness's `config_files` that does not exist yet (so a new project
/// skips the agent's first-run setup, but a config in use is never changed),
/// and creates the mount points of `mounts` that land inside it.
///
/// The runtime would otherwise create those mount points itself, owned by
/// root, and the agent could not write to its own config directory.
pub fn prepare_agent_config(
    home: &CasuloHome,
    project: &ProjectName,
    harness: &Harness,
    mounts: &[Mount],
) -> Result<()> {
    let Some(config_dir) = &harness.config_dir else {
        return Ok(());
    };
    let host_config = home.agent_config_dir(project, &harness.name);
    fs::create_dir_all(&host_config)
        .with_context(|| format!("cannot create {}", host_config.display()))?;
    for file in &harness.config_files {
        let path = host_path(&host_config, &file.path);
        if path.exists() {
            continue;
        }
        create_parent(&path)?;
        fs::write(&path, &file.content)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    let container_config = format!("{CONTAINER_HOME}/{config_dir}/");
    for mount in mounts {
        let Some(inside) = mount.target.strip_prefix(&container_config) else {
            continue;
        };
        let mount_point = host_path(&host_config, inside);
        if mount_point.exists() {
            continue;
        }
        if mount.source.is_dir() {
            fs::create_dir_all(&mount_point)
                .with_context(|| format!("cannot create {}", mount_point.display()))?;
        } else {
            create_parent(&mount_point)?;
            fs::write(&mount_point, "")
                .with_context(|| format!("cannot create {}", mount_point.display()))?;
        }
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    Ok(())
}

/// The values from casulo's `[secrets]` for the secret variables in `names`
/// that the host does not set (`set_on_host`), so a variable exported in
/// the shell always wins over the config.
pub fn config_secrets(
    names: &[String],
    set_on_host: &[String],
    secrets: &BTreeMap<String, Secret>,
) -> Vec<(String, Secret)> {
    let mut values: Vec<(String, Secret)> = Vec::new();
    for name in names {
        let already_given =
            set_on_host.contains(name) || values.iter().any(|(given, _)| given == name);
        if already_given {
            continue;
        }
        if let Some(secret) = secrets.get(name) {
            values.push((name.clone(), secret.clone()));
        }
    }
    values
}

/// `relative`, written with `/`, under `base` with the host's separators.
fn host_path(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// Describes the container for `project` with `harness`, running `command`
/// with `workspace` from the host mounted at `/workspace`.
///
/// The git identity is passed so the agent's commits carry it, the
/// harness's `config_env` says where its config directory is, and each of
/// the harness's `auth_env` variables is passed by name only.
pub fn run_spec(
    workspace: PathBuf,
    project: &ProjectName,
    harness: &Harness,
    git_identity: &GitIdentity,
    user: Option<HostUser>,
    command: Vec<String>,
    mounts: Vec<Mount>,
) -> RunSpec {
    let mut environment = vec![value("HOME", CONTAINER_HOME)];
    if let Some(name) = &git_identity.name {
        environment.push(value("GIT_AUTHOR_NAME", name));
        environment.push(value("GIT_COMMITTER_NAME", name));
    }
    if let Some(email) = &git_identity.email {
        environment.push(value("GIT_AUTHOR_EMAIL", email));
        environment.push(value("GIT_COMMITTER_EMAIL", email));
    }
    // The same identity as git config, so `git config user.name` answers in
    // the container too: an agent that checks it before committing would
    // otherwise think no identity is set.
    let identity_config: Vec<(&str, &String)> = [
        ("user.name", &git_identity.name),
        ("user.email", &git_identity.email),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.as_ref().map(|value| (key, value)))
    .collect();
    if !identity_config.is_empty() {
        environment.push(value(
            "GIT_CONFIG_COUNT",
            &identity_config.len().to_string(),
        ));
        for (index, (key, setting)) in identity_config.iter().enumerate() {
            environment.push(value(&format!("GIT_CONFIG_KEY_{index}"), key));
            environment.push(value(&format!("GIT_CONFIG_VALUE_{index}"), setting));
        }
    }
    if let (Some(name), Some(config_dir)) = (&harness.config_env, &harness.config_dir) {
        environment.push(value(name, &format!("{CONTAINER_HOME}/{config_dir}")));
    }
    for name in &harness.auth_env {
        environment.push(EnvironmentVariable::FromHost { name: name.clone() });
    }

    RunSpec {
        image: image_tag(project, &harness.name),
        command,
        workspace,
        workspace_read_only: false,
        mounts,
        environment,
        secret_values: Vec::new(),
        user,
        terminal: true,
        network: None,
        network_alias: None,
        extra_networks: Vec::new(),
        name: None,
        detach: false,
    }
}

fn value(name: &str, value: &str) -> EnvironmentVariable {
    EnvironmentVariable::Value {
        name: name.to_string(),
        value: value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::application::sandbox::test_support::built_in_harness;

    fn home() -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from("home")), None).unwrap()
    }

    #[test]
    fn spec_uses_project_image_workspace_and_command() {
        let project = ProjectName::parse("demo").unwrap();

        let spec = run_spec(
            home().workspace_dir(&project),
            &project,
            &built_in_harness("claude"),
            &GitIdentity::default(),
            None,
            vec!["claude".to_string()],
            Vec::new(),
        );

        assert_eq!(spec.image, "casulo/demo:claude");
        assert_eq!(spec.workspace, home().workspace_dir(&project));
        assert_eq!(spec.command, ["claude"]);
        assert!(spec.terminal);
        assert_eq!(
            spec.environment,
            [
                value("HOME", "/home/casulo"),
                value("CLAUDE_CONFIG_DIR", "/home/casulo/.claude"),
                EnvironmentVariable::FromHost {
                    name: "ANTHROPIC_API_KEY".to_string()
                },
                EnvironmentVariable::FromHost {
                    name: "CLAUDE_CODE_OAUTH_TOKEN".to_string()
                }
            ]
        );
    }

    fn settings(model: Option<&str>, effort: Option<&str>) -> ModelSettings {
        ModelSettings {
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
        }
    }

    #[test]
    fn agent_command_puts_model_and_effort_after_the_program() {
        let mut harness = built_in_harness("claude");
        harness.command = vec!["claude".to_string(), "--verbose".to_string()];

        let (command, warnings) = agent_command(&harness, &settings(Some("opus"), Some("high")));

        assert_eq!(
            command,
            ["claude", "--model", "opus", "--effort", "high", "--verbose"]
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn agent_command_for_codex_sets_the_effort_with_a_config_override() {
        let codex = built_in_harness("codex");

        let (command, _) = agent_command(&codex, &settings(Some("gpt-5-codex"), Some("medium")));

        assert_eq!(
            command,
            [
                "codex",
                "--model",
                "gpt-5-codex",
                "-c",
                "model_reasoning_effort=medium"
            ]
        );
    }

    #[test]
    fn agent_command_without_settings_is_the_harness_command() {
        let (command, warnings) =
            agent_command(&built_in_harness("claude"), &ModelSettings::default());

        assert_eq!(command, ["claude"]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn unsupported_effort_is_ignored_with_a_warning() {
        let mut harness = built_in_harness("claude");
        harness.effort_args = Vec::new();

        let (command, warnings) = agent_command(&harness, &settings(Some("opus"), Some("high")));

        assert_eq!(command, ["claude", "--model", "opus"]);
        assert_eq!(
            warnings,
            ["harness \"claude\" has no way to set the effort, so effort = \"high\" is ignored"]
        );
    }

    #[test]
    fn git_identity_sets_author_and_committer() {
        let identity = GitIdentity {
            name: Some("Ada".to_string()),
            email: Some("ada@example.com".to_string()),
        };
        let project = ProjectName::parse("demo").unwrap();

        let spec = run_spec(
            home().workspace_dir(&project),
            &project,
            &built_in_harness("claude"),
            &identity,
            None,
            Vec::new(),
            Vec::new(),
        );

        for (name, expected) in [
            ("GIT_AUTHOR_NAME", "Ada"),
            ("GIT_COMMITTER_NAME", "Ada"),
            ("GIT_AUTHOR_EMAIL", "ada@example.com"),
            ("GIT_COMMITTER_EMAIL", "ada@example.com"),
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "user.name"),
            ("GIT_CONFIG_VALUE_0", "Ada"),
            ("GIT_CONFIG_KEY_1", "user.email"),
            ("GIT_CONFIG_VALUE_1", "ada@example.com"),
        ] {
            assert!(spec.environment.contains(&value(name, expected)), "{name}");
        }
    }

    #[test]
    fn without_an_identity_no_git_config_is_set() {
        let project = ProjectName::parse("demo").unwrap();

        let spec = run_spec(
            home().workspace_dir(&project),
            &project,
            &built_in_harness("claude"),
            &GitIdentity::default(),
            None,
            Vec::new(),
            Vec::new(),
        );

        assert!(spec.environment.iter().all(|variable| !matches!(
            variable,
            EnvironmentVariable::Value { name, .. } if name.starts_with("GIT_")
        )));
    }

    fn temporary_home(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap()
    }

    #[test]
    fn config_mount_keeps_the_config_dir_in_the_project() {
        let project = ProjectName::parse("demo").unwrap();
        let harness = built_in_harness("claude");

        assert_eq!(
            config_mount(&home(), &project, &harness),
            Some(Mount {
                source: home().agent_config_dir(&project, &harness.name),
                target: "/home/casulo/.claude".to_string(),
                read_only: false,
            })
        );
    }

    #[test]
    fn harness_without_config_dir_keeps_nothing() {
        let mut harness = built_in_harness("claude");
        harness.config_dir = None;
        harness.config_env = None;
        let project = ProjectName::parse("demo").unwrap();

        let spec = run_spec(
            home().workspace_dir(&project),
            &project,
            &harness,
            &GitIdentity::default(),
            None,
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(config_mount(&home(), &project, &harness), None);
        assert!(
            !spec
                .environment
                .iter()
                .any(|variable| matches!(variable, EnvironmentVariable::Value { name, .. } if name == "CLAUDE_CONFIG_DIR"))
        );
    }

    #[test]
    fn spec_tells_the_agent_where_its_config_dir_is() {
        let project = ProjectName::parse("demo").unwrap();

        let spec = run_spec(
            home().workspace_dir(&project),
            &project,
            &built_in_harness("claude"),
            &GitIdentity::default(),
            None,
            Vec::new(),
            Vec::new(),
        );

        assert!(
            spec.environment
                .contains(&value("CLAUDE_CONFIG_DIR", "/home/casulo/.claude"))
        );
    }

    #[test]
    fn config_files_are_written_only_into_a_new_config_dir() {
        let directory = tempfile::tempdir().unwrap();
        let home = temporary_home(&directory);
        let project = ProjectName::parse("demo").unwrap();
        let harness = built_in_harness("claude");
        let config_file = home
            .agent_config_dir(&project, &harness.name)
            .join(".claude.json");

        prepare_agent_config(&home, &project, &harness, &[]).unwrap();
        let written = fs::read_to_string(&config_file).unwrap();
        assert!(written.contains("\"hasCompletedOnboarding\": true"));
        assert!(written.contains("\"/workspace\": {\"hasTrustDialogAccepted\": true}"));

        fs::write(&config_file, "{\"logged_in\": true}").unwrap();
        prepare_agent_config(&home, &project, &harness, &[]).unwrap();
        assert_eq!(
            fs::read_to_string(&config_file).unwrap(),
            "{\"logged_in\": true}"
        );
    }

    #[test]
    fn mount_points_inside_the_config_dir_are_created_first() {
        let directory = tempfile::tempdir().unwrap();
        let home = temporary_home(&directory);
        let project = ProjectName::parse("demo").unwrap();
        let harness = built_in_harness("claude");
        let skills = directory.path().join("skills");
        fs::create_dir_all(&skills).unwrap();
        let instructions = directory.path().join("AGENTS.md");
        fs::write(&instructions, "Be brief.").unwrap();
        let mounts = [
            Mount {
                source: skills,
                target: "/home/casulo/.claude/skills".to_string(),
                read_only: true,
            },
            Mount {
                source: instructions,
                target: "/home/casulo/.claude/CLAUDE.md".to_string(),
                read_only: true,
            },
            Mount {
                source: directory.path().join("mcp.json"),
                target: "/home/casulo/.casulo/mcp.json".to_string(),
                read_only: true,
            },
        ];

        prepare_agent_config(&home, &project, &harness, &mounts).unwrap();

        let config = home.agent_config_dir(&project, &harness.name);
        assert!(config.join("skills").is_dir());
        assert!(config.join("CLAUDE.md").is_file());
        assert!(!config.join(".casulo").exists());
    }

    #[test]
    fn config_secrets_fill_in_only_what_the_host_lacks() {
        let names = [
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CONTEXT7_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ]
        .map(str::to_string);
        let set_on_host = ["ANTHROPIC_API_KEY".to_string()];
        let secrets = BTreeMap::from([
            ("ANTHROPIC_API_KEY".to_string(), Secret::new("from-config")),
            ("CLAUDE_CODE_OAUTH_TOKEN".to_string(), Secret::new("token")),
            ("UNUSED".to_string(), Secret::new("never passed")),
        ]);

        let values = config_secrets(&names, &set_on_host, &secrets);

        assert_eq!(
            values,
            [("CLAUDE_CODE_OAUTH_TOKEN".to_string(), Secret::new("token"))]
        );
    }
}
