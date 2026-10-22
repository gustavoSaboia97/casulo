//! The global and project configuration, as stored in `casulo.toml` files.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};

use serde::{Deserialize, Serialize};

use super::push::GitSettings;
use super::repository_url::RepositoryUrl;
use crate::application::shared::domain::{
    CasuloHome, HarnessName, ProjectName, Secret, WorkspacePath,
};

/// Container runtime used when the global configuration does not choose one.
const DEFAULT_RUNTIME: &str = "docker";

/// Harness used when neither the command line nor the project chooses one.
const DEFAULT_HARNESS: &str = "claude";

/// Global settings, from `~/.casulo/config/casulo.toml`.
// `default` fills every missing field from `Default`, so an empty file is valid.
// `deny_unknown_fields` turns a typo in a key into an error instead of a
// setting that is silently ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
    /// Name or path of the container runtime binary.
    pub runtime: String,
    /// Harness used when none is chosen.
    pub default_harness: HarnessName,
    /// Who the agent's commits are by, and whether it may push, from `[git]`.
    pub git: GitSettings,
    /// Editor for `casulo config`, such as `code --wait`. Not set by default.
    pub editor: Option<String>,
    /// Session model and effort for each harness, from `[harness.<name>]`.
    // A `BTreeMap` is a dictionary kept sorted by key, so output is stable.
    pub harness: BTreeMap<HarnessName, ModelSettings>,
    /// The project memory, from `[memory]`.
    pub memory: MemorySettings,
    /// Values of secret variables, such as `CLAUDE_CODE_OAUTH_TOKEN`, from
    /// `[secrets]`. Used only when the host environment does not set them.
    pub secrets: BTreeMap<String, Secret>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            runtime: DEFAULT_RUNTIME.to_string(),
            default_harness: HarnessName::parse(DEFAULT_HARNESS)
                .expect("the default harness name is valid"),
            git: GitSettings::default(),
            editor: None,
            harness: BTreeMap::new(),
            memory: MemorySettings::default(),
            secrets: BTreeMap::new(),
        }
    }
}

/// Author and committer identity for commits made inside the container,
/// chosen from the project's and the global `[git]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitIdentity {
    /// Name, for example "Ada Lovelace". Not set by default.
    pub name: Option<String>,
    /// Email, for example "ada@example.com". Not set by default.
    pub email: Option<String>,
}

/// Settings of one project, from `~/.casulo/projects/<name>/casulo.toml`.
///
/// It lives outside the repository, so nothing in the cloned code can change it.
// `try_from` reads the file into `UncheckedProjectConfig` first, then checks
// that exactly one of `repository` and `workspace` is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedProjectConfig")]
pub struct ProjectConfig {
    /// Git URL the workspace was cloned from. Set when `workspace` is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryUrl>,
    /// Absolute path of a host directory used in place as the workspace.
    /// Set when `repository` is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    /// Harness for this project, overriding the global default when set.
    #[serde(default)]
    pub harness: Option<HarnessName>,
    /// Workspace paths hidden from the agent, such as `.env`. Each one that
    /// exists is covered by an empty, read-only file or directory.
    #[serde(default)]
    pub mask: Vec<WorkspacePath>,
    /// Session model and effort for each harness, from `[models.<name>]`,
    /// overriding the global `[harness.<name>]`. Named differently because
    /// `harness` already chooses the project's harness.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<HarnessName, ModelSettings>,
    /// Git identity and push for this project, overriding the global `[git]`
    /// one value at a time.
    #[serde(default)]
    pub git: GitSettings,
    /// Services from the workspace's docker compose file, from `[compose]`.
    #[serde(default)]
    pub compose: ComposeSettings,
    /// The project memory, overriding the global `[memory]`.
    #[serde(default)]
    pub memory: MemorySettings,
    /// Extra steps for this project's agent image, from `[image]`.
    #[serde(default)]
    pub image: ImageSettings,
}

/// `[image]` in a project's `casulo.toml`: what the project needs in its
/// agent image, such as a compiler. It lives in `~/.casulo`, so the
/// repository cannot change it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageSettings {
    /// Shell commands run as root by `casulo build`, after the agent is
    /// installed, such as `apt-get install --yes build-essential`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<String>,
}

/// `[memory]` in the global or a project's `casulo.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemorySettings {
    /// Whether agents get the project memory. Off by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Whether the project memory is on: the project's choice, else the
/// global one, else off.
pub fn memory_enabled(project: &MemorySettings, global: &MemorySettings) -> bool {
    project.enabled.or(global.enabled).unwrap_or(false)
}

/// `[compose]` in a project's `casulo.toml`. It lives in `~/.casulo`, so the
/// repository cannot turn it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComposeSettings {
    /// Whether `casulo run` starts the services and joins their network.
    pub enabled: bool,
    /// The compose file, relative to the workspace.
    pub file: WorkspacePath,
}

impl Default for ComposeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            file: WorkspacePath::parse("docker-compose.yml")
                .expect("the default compose file is a valid path"),
        }
    }
}

impl ProjectConfig {
    /// The workspace on the host: the directory given to `casulo new`, or
    /// the clone in `projects/<name>/workspace/`.
    pub fn workspace_dir(&self, home: &CasuloHome, project: &ProjectName) -> PathBuf {
        match &self.workspace {
            Some(directory) => directory.clone(),
            None => home.workspace_dir(project),
        }
    }
}

/// `casulo.toml` of a project as written, before the checks that need more
/// than one field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedProjectConfig {
    repository: Option<RepositoryUrl>,
    workspace: Option<PathBuf>,
    #[serde(default)]
    harness: Option<HarnessName>,
    #[serde(default)]
    mask: Vec<WorkspacePath>,
    #[serde(default)]
    models: BTreeMap<HarnessName, ModelSettings>,
    #[serde(default)]
    git: GitSettings,
    #[serde(default)]
    compose: ComposeSettings,
    #[serde(default)]
    memory: MemorySettings,
    #[serde(default)]
    image: ImageSettings,
}

impl TryFrom<UncheckedProjectConfig> for ProjectConfig {
    type Error = anyhow::Error;

    fn try_from(unchecked: UncheckedProjectConfig) -> Result<Self> {
        match (&unchecked.repository, &unchecked.workspace) {
            (Some(_), None) => {}
            (None, Some(directory)) if directory.is_absolute() => {}
            (None, Some(_)) => bail!("workspace must be an absolute path"),
            _ => {
                bail!("set exactly one of repository (a cloned project) or workspace (a directory)")
            }
        }
        Ok(Self {
            repository: unchecked.repository,
            workspace: unchecked.workspace,
            harness: unchecked.harness,
            mask: unchecked.mask,
            models: unchecked.models,
            git: unchecked.git,
            compose: unchecked.compose,
            memory: unchecked.memory,
            image: unchecked.image,
        })
    }
}

/// The model and reasoning effort of an agent session. Values are passed
/// through as written, since only the harness knows its models.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelSettings {
    /// Model name, such as `opus`. Not set by default.
    pub model: Option<String>,
    /// Reasoning effort, such as `high`. Not set by default.
    pub effort: Option<String>,
}

/// The model settings for `harness`: each value from the project when it
/// sets one, else from the global config, else unset.
pub fn choose_model_settings(
    harness: &HarnessName,
    project: &ProjectConfig,
    global: &GlobalConfig,
) -> ModelSettings {
    let project = project.models.get(harness).cloned().unwrap_or_default();
    let global = global.harness.get(harness).cloned().unwrap_or_default();
    ModelSettings {
        model: project.model.or(global.model),
        effort: project.effort.or(global.effort),
    }
}

/// Paths a new project masks: files that commonly hold secrets, and the
/// harness config a repository can ship. That config could grant the agent
/// permissions, run hooks or add MCP servers and subagents, and nothing from
/// the repository may grant permissions.
pub fn default_mask() -> Vec<WorkspacePath> {
    [
        ".env",
        ".mcp.json",
        ".claude/settings.json",
        ".claude/agents",
        ".codex/agents",
    ]
    .iter()
    .map(|path| WorkspacePath::parse(path).expect("the default mask is a valid path"))
    .collect()
}

/// The harness to use: the one chosen on the command line, else the
/// project's, else the global default.
pub fn choose_harness(
    command_line: Option<HarnessName>,
    project: &ProjectConfig,
    global: &GlobalConfig,
) -> HarnessName {
    command_line
        .or_else(|| project.harness.clone())
        .unwrap_or_else(|| global.default_harness.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_off_unless_the_project_or_global_config_turns_it_on() {
        let on = MemorySettings {
            enabled: Some(true),
        };
        let off = MemorySettings {
            enabled: Some(false),
        };
        let unset = MemorySettings::default();

        assert!(!memory_enabled(&unset, &unset));
        assert!(memory_enabled(&unset, &on));
        assert!(!memory_enabled(&off, &on));
        assert!(memory_enabled(&on, &off));
    }

    #[test]
    fn harness_choice_prefers_command_line_then_project_then_global() {
        let codex = HarnessName::parse("codex").unwrap();
        let other = HarnessName::parse("other").unwrap();
        let global = GlobalConfig::default();
        let mut project = ProjectConfig {
            repository: Some(RepositoryUrl::parse("https://example.com/repo.git").unwrap()),
            workspace: None,
            harness: None,
            mask: Vec::new(),
            models: BTreeMap::new(),
            git: GitSettings::default(),
            compose: ComposeSettings::default(),
            memory: MemorySettings::default(),
            image: ImageSettings::default(),
        };

        assert_eq!(choose_harness(None, &project, &global).as_str(), "claude");
        project.harness = Some(codex.clone());
        assert_eq!(choose_harness(None, &project, &global), codex);
        assert_eq!(
            choose_harness(Some(other.clone()), &project, &global),
            other
        );
    }

    #[test]
    fn model_settings_prefer_project_then_global_for_each_value() {
        let claude = HarnessName::parse("claude").unwrap();
        let codex = HarnessName::parse("codex").unwrap();
        let global: GlobalConfig =
            toml::from_str("[harness.claude]\nmodel = \"sonnet\"\neffort = \"medium\"\n").unwrap();
        let project: ProjectConfig = toml::from_str(
            "repository = \"https://example.com/repo.git\"\nharness = \"claude\"\n[models.claude]\nmodel = \"opus\"\n",
        )
        .unwrap();

        let chosen = choose_model_settings(&claude, &project, &global);

        assert_eq!(chosen.model.as_deref(), Some("opus"));
        assert_eq!(chosen.effort.as_deref(), Some("medium"));
        assert_eq!(
            choose_model_settings(&codex, &project, &global),
            ModelSettings::default()
        );
    }

    #[test]
    fn rejects_unknown_model_settings() {
        assert!(toml::from_str::<GlobalConfig>("[harness.claude]\nmodle = \"x\"\n").is_err());
        assert!(toml::from_str::<GlobalConfig>("[harness.\"Bad Name\"]\nmodel = \"x\"\n").is_err());
    }

    #[test]
    fn global_defaults_use_default_runtime_and_claude_without_git_identity() {
        let config = GlobalConfig::default();

        assert_eq!(config.runtime, DEFAULT_RUNTIME);
        assert_eq!(config.default_harness.as_str(), "claude");
        assert_eq!(config.git, GitSettings::default());
        assert_eq!(config.git.name, None);
    }

    #[test]
    fn empty_global_file_gives_defaults() {
        let config: GlobalConfig = toml::from_str("").expect("empty TOML is valid");

        assert_eq!(config, GlobalConfig::default());
    }

    #[test]
    fn partial_global_file_keeps_other_defaults() {
        let config: GlobalConfig = toml::from_str("[git]\nname = \"Ada\"\n").expect("valid TOML");

        assert_eq!(config.git.name.as_deref(), Some("Ada"));
        assert_eq!(config.git.email, None);
        assert_eq!(config.runtime, DEFAULT_RUNTIME);
    }

    #[test]
    fn secrets_are_read_but_never_shown_in_debug_output() {
        let config: GlobalConfig =
            toml::from_str("[secrets]\nCLAUDE_CODE_OAUTH_TOKEN = \"sk-ant-oat01-value\"\n")
                .expect("valid TOML");

        assert_eq!(
            config.secrets["CLAUDE_CODE_OAUTH_TOKEN"].expose(),
            "sk-ant-oat01-value"
        );
        assert!(!format!("{config:?}").contains("sk-ant-oat01-value"));
    }

    #[test]
    fn project_harness_defaults_to_none() {
        let config: ProjectConfig =
            toml::from_str("repository = \"https://example.com/repo.git\"\n").expect("valid TOML");

        assert_eq!(config.harness, None);
        assert!(config.mask.is_empty());
    }

    #[test]
    fn project_mask_is_parsed_and_validated() {
        let config: ProjectConfig = toml::from_str(
            "repository = \"https://example.com/repo.git\"\nmask = [\".env\", \"keys/\"]\n",
        )
        .expect("valid TOML");

        assert_eq!(
            config.mask,
            [
                WorkspacePath::parse(".env").unwrap(),
                WorkspacePath::parse("keys").unwrap()
            ]
        );
        assert!(
            toml::from_str::<ProjectConfig>(
                "repository = \"https://example.com/repo.git\"\nmask = [\"../outside\"]\n"
            )
            .is_err()
        );
    }

    #[test]
    fn project_requires_exactly_one_of_repository_and_workspace() {
        let both = "repository = \"https://example.com/repo.git\"\nworkspace = \"/srv/app\"\n";

        assert!(toml::from_str::<ProjectConfig>("").is_err());
        assert!(toml::from_str::<ProjectConfig>(both).is_err());
        assert!(toml::from_str::<ProjectConfig>("workspace = \"relative/app\"\n").is_err());
        assert!(toml::from_str::<ProjectConfig>("workspace = \"/srv/app\"\nmaks = []\n").is_err());
    }

    #[test]
    fn workspace_directory_is_used_in_place() {
        // An absolute path on every OS, so the test runs everywhere.
        let directory = std::env::temp_dir().join("app");
        let text = format!(
            "workspace = {}\n",
            toml::Value::from(directory.display().to_string())
        );
        let config: ProjectConfig = toml::from_str(&text).unwrap();
        let home = CasuloHome::resolve(Some("home".into()), None).unwrap();
        let project = ProjectName::parse("app").unwrap();

        assert_eq!(config.workspace_dir(&home, &project), directory);
        let cloned = ProjectConfig {
            repository: Some(RepositoryUrl::parse("https://example.com/app.git").unwrap()),
            workspace: None,
            ..config
        };
        assert_eq!(
            cloned.workspace_dir(&home, &project),
            home.workspace_dir(&project)
        );
    }

    #[test]
    fn rejects_invalid_harness_name() {
        let error = toml::from_str::<GlobalConfig>("default_harness = \"../x\"\n")
            .expect_err("invalid name");

        assert!(error.to_string().contains("invalid harness name"));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<GlobalConfig>("defualt_harness = \"codex\"\n").is_err());
        assert!(toml::from_str::<GlobalConfig>("[git]\nmail = \"a@b.c\"\n").is_err());
    }
}
