//! Harnesses: how to install and run one coding agent in the container.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::application::shared::domain::HarnessName;

/// A validated harness manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harness {
    /// Name used on the command line, such as `claude`.
    pub name: HarnessName,
    /// Shell lines that install the agent, each one a `RUN` in the Dockerfile.
    pub install: Vec<String>,
    /// Program and arguments that start the agent.
    pub command: Vec<String>,
    /// The directory where the agent keeps its login, settings and history,
    /// relative to the user home in the container, such as `.claude`. It is
    /// kept per project between runs; the rest of the home starts empty.
    pub config_dir: Option<String>,
    /// The variable that tells the agent where `config_dir` is, such as
    /// `CLAUDE_CONFIG_DIR`. Lets an agent that also writes files next to that
    /// directory keep them inside it.
    pub config_env: Option<String>,
    /// Names of environment variables with credentials, passed by name only.
    pub auth_env: Vec<String>,
    /// Files written into `config_dir` when they do not exist yet, such as
    /// settings that skip the agent's first-run setup. Sorted by path.
    pub config_files: Vec<ConfigFile>,
    /// File the agent reads its project instructions from, such as `CLAUDE.md`.
    pub instructions_file: Option<String>,
    /// Arguments that choose the session model, with `{model}` in one of
    /// them. Empty when the harness has no way to choose a model.
    pub model_args: Vec<String>,
    /// Arguments that choose the reasoning effort, with `{effort}` in one of
    /// them. Empty when the harness has no way to choose an effort.
    pub effort_args: Vec<String>,
    /// Where the user's agent setup from `~/.casulo/agent` goes.
    pub inject: Inject,
}

/// A file casulo writes into a new agent config directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    /// Path relative to the config directory, such as `.claude.json`.
    pub path: String,
    /// The whole content of the file.
    pub content: String,
}

/// Where each piece of the user's agent setup goes in the container. A
/// piece left out is not given to this harness.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inject {
    /// Path of the global instructions file, relative to the container home.
    pub instructions: Option<String>,
    /// Directory for Agent Skills, relative to the container home.
    pub skills: Option<String>,
    /// Directory and format for subagents.
    pub agents: Option<AgentsTarget>,
    /// How MCP servers are passed to the agent.
    pub mcp: Option<McpMode>,
    /// How session hooks are given to the agent.
    pub hooks: Option<HooksMode>,
}

/// Where subagents go and the format they are written in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsTarget {
    /// Directory relative to the container home, such as `.claude/agents`.
    pub dir: String,
    /// File format the harness reads.
    pub format: AgentsFormat,
}

/// File formats for subagents. A closed set: each one is rendered by code.
// `rename_all` maps `ClaudeMarkdown` to "claude-markdown" in TOML. An unknown
// value is an error that lists the valid ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentsFormat {
    /// Markdown with YAML frontmatter, read by Claude Code.
    ClaudeMarkdown,
    /// One TOML file per subagent, read by Codex.
    CodexToml,
}

/// Ways to pass MCP servers to a harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpMode {
    /// A generated JSON file passed with `--mcp-config`.
    ClaudeMcpConfig,
    /// `-c mcp_servers.<name>.<key>=<value>` arguments.
    CodexConfigOverrides,
}

/// Ways to give a harness session hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HooksMode {
    /// A generated settings file passed with `--settings`.
    ClaudeSettings,
}

/// The manifest exactly as written in TOML, before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    name: String,
    install: Vec<String>,
    command: Vec<String>,
    #[serde(default)]
    config_dir: Option<String>,
    #[serde(default)]
    config_env: Option<String>,
    #[serde(default)]
    auth_env: Vec<String>,
    // A `BTreeMap` keeps the TOML table sorted by path, so the order of the
    // files never depends on how the manifest was written.
    #[serde(default)]
    config_files: BTreeMap<String, String>,
    #[serde(default)]
    instructions_file: Option<String>,
    #[serde(default)]
    model_args: Vec<String>,
    #[serde(default)]
    effort_args: Vec<String>,
    #[serde(default)]
    inject: Inject,
}

impl Harness {
    /// Parses and validates a manifest written in TOML.
    pub fn parse_manifest(text: &str) -> Result<Self> {
        let raw: RawManifest = toml::from_str(text)?;
        let name = HarnessName::parse(&raw.name)?;

        if raw.install.is_empty() {
            bail!("harness \"{name}\": install needs at least one line");
        }
        for line in &raw.install {
            if line.trim().is_empty() || line.contains(['\n', '\r']) {
                bail!("harness \"{name}\": each install entry must be one non-empty line");
            }
        }
        if raw.command.is_empty() || raw.command.iter().any(|part| part.is_empty()) {
            bail!("harness \"{name}\": command needs a program and no empty arguments");
        }
        if let Some(path) = &raw.config_dir {
            check_relative_path(&name, "config_dir", path)?;
        }
        let without_dir =
            raw.config_dir.is_none() && (raw.config_env.is_some() || !raw.config_files.is_empty());
        if without_dir {
            bail!("harness \"{name}\": config_env and config_files need a config_dir");
        }
        if let Some(variable) = &raw.config_env
            && !is_environment_variable_name(variable)
        {
            bail!("harness \"{name}\": invalid config_env; use a name like CLAUDE_CONFIG_DIR");
        }
        for variable in &raw.auth_env {
            if !is_environment_variable_name(variable) {
                bail!(
                    "harness \"{name}\": invalid auth_env; use names like ANTHROPIC_API_KEY, never the key itself"
                );
            }
        }
        for path in raw.config_files.keys() {
            check_relative_path(&name, "config file", path)?;
        }
        let invalid_file_name = raw.instructions_file.as_deref().is_some_and(|file| {
            file.is_empty() || file.contains(['/', '\\']) || file == "." || file == ".."
        });
        if invalid_file_name {
            bail!("harness \"{name}\": instructions_file must be a file name, such as CLAUDE.md");
        }
        check_placeholder_args(&name, "model_args", "{model}", &raw.model_args)?;
        check_placeholder_args(&name, "effort_args", "{effort}", &raw.effort_args)?;
        let inject = &raw.inject;
        if let Some(path) = &inject.instructions {
            check_relative_path(&name, "inject.instructions", path)?;
        }
        if let Some(path) = &inject.skills {
            check_relative_path(&name, "inject.skills", path)?;
        }
        if let Some(agents) = &inject.agents {
            check_relative_path(&name, "inject.agents.dir", &agents.dir)?;
        }

        Ok(Self {
            name,
            install: raw.install,
            command: raw.command,
            config_dir: raw.config_dir,
            config_env: raw.config_env,
            auth_env: raw.auth_env,
            config_files: raw
                .config_files
                .into_iter()
                .map(|(path, content)| ConfigFile { path, content })
                .collect(),
            instructions_file: raw.instructions_file,
            model_args: raw.model_args,
            effort_args: raw.effort_args,
            inject: raw.inject,
        })
    }
}

impl Harness {
    /// The pieces of the user's agent setup this harness receives, such as
    /// `instructions` or `model`, in a fixed order.
    pub fn supported_pieces(&self) -> Vec<&'static str> {
        [
            ("instructions", self.inject.instructions.is_some()),
            ("skills", self.inject.skills.is_some()),
            ("agents", self.inject.agents.is_some()),
            ("mcp", self.inject.mcp.is_some()),
            ("hooks", self.inject.hooks.is_some()),
            ("model", !self.model_args.is_empty()),
            ("effort", !self.effort_args.is_empty()),
        ]
        .into_iter()
        .filter(|(_, supported)| *supported)
        .map(|(piece, _)| piece)
        .collect()
    }
}

/// Arguments with a placeholder must use it, or the value would be lost.
/// Empty means the harness does not support the setting.
fn check_placeholder_args(
    name: &HarnessName,
    field: &str,
    placeholder: &str,
    arguments: &[String],
) -> Result<()> {
    let valid = arguments.is_empty()
        || (arguments.iter().all(|argument| !argument.is_empty())
            && arguments
                .iter()
                .any(|argument| argument.contains(placeholder)));
    if !valid {
        bail!(
            "harness \"{name}\": {field} needs non-empty arguments with {placeholder} in one of them, such as [\"--flag\", \"{placeholder}\"]"
        );
    }
    Ok(())
}

/// Config and inject paths are mounted inside the container home, so they
/// must stay in it.
fn check_relative_path(name: &HarnessName, kind: &str, path: &str) -> Result<()> {
    let inside_home = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if !inside_home {
        bail!(
            "harness \"{name}\": invalid {kind} \"{path}\"; use a path relative to the home directory, such as .config/agent"
        );
    }
    Ok(())
}

/// Whether `value` is a portable environment variable name: letters, digits
/// and `_`, not starting with a digit. This also keeps `=` out, so a name can
/// never smuggle in a value.
pub(super) fn is_environment_variable_name(value: &str) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
name = "agent"
install = ["npm install -g agent"]
command = ["agent", "--yes"]
config_dir = ".agent"
config_env = "AGENT_HOME"
auth_env = ["AGENT_API_KEY"]
config_files = { "sub/z.json" = "{}", "agent.json" = '{"setup": true}' }
instructions_file = "AGENTS.md"
model_args = ["--model", "{model}"]
effort_args = ["-c", "effort={effort}"]

[inject]
instructions = ".agent/AGENTS.md"
skills = ".agent/skills"
agents = { dir = ".agent/agents", format = "codex-toml" }
mcp = "claude-mcp-config"
hooks = "claude-settings"
"#;

    fn with(line: &str) -> String {
        format!("{VALID}{line}\n")
    }

    fn replace(field: &str, value: &str) -> String {
        VALID
            .lines()
            .map(|line| {
                if line.starts_with(&format!("{field} =")) {
                    format!("{field} = {value}")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_a_complete_manifest() {
        let harness = Harness::parse_manifest(VALID).unwrap();

        assert_eq!(harness.name.as_str(), "agent");
        assert_eq!(harness.install, ["npm install -g agent"]);
        assert_eq!(harness.command, ["agent", "--yes"]);
        assert_eq!(harness.config_dir.as_deref(), Some(".agent"));
        assert_eq!(harness.config_env.as_deref(), Some("AGENT_HOME"));
        assert_eq!(harness.auth_env, ["AGENT_API_KEY"]);
        assert_eq!(
            harness.config_files,
            [
                ConfigFile {
                    path: "agent.json".to_string(),
                    content: "{\"setup\": true}".to_string(),
                },
                ConfigFile {
                    path: "sub/z.json".to_string(),
                    content: "{}".to_string(),
                },
            ]
        );
        assert_eq!(harness.instructions_file.as_deref(), Some("AGENTS.md"));
        assert_eq!(harness.model_args, ["--model", "{model}"]);
        assert_eq!(harness.effort_args, ["-c", "effort={effort}"]);
        assert_eq!(
            harness.inject,
            Inject {
                instructions: Some(".agent/AGENTS.md".to_string()),
                skills: Some(".agent/skills".to_string()),
                agents: Some(AgentsTarget {
                    dir: ".agent/agents".to_string(),
                    format: AgentsFormat::CodexToml,
                }),
                mcp: Some(McpMode::ClaudeMcpConfig),
                hooks: Some(HooksMode::ClaudeSettings),
            }
        );
    }

    #[test]
    fn optional_fields_default_to_empty() {
        let text = "name = \"agent\"\ninstall = [\"true\"]\ncommand = [\"agent\"]\n";
        let harness = Harness::parse_manifest(text).unwrap();

        assert_eq!(harness.config_dir, None);
        assert_eq!(harness.config_env, None);
        assert!(harness.auth_env.is_empty());
        assert!(harness.config_files.is_empty());
        assert_eq!(harness.instructions_file, None);
        assert!(harness.model_args.is_empty());
        assert!(harness.effort_args.is_empty());
        assert_eq!(harness.inject, Inject::default());
    }

    #[test]
    fn rejects_unknown_fields_and_bad_toml() {
        assert!(Harness::parse_manifest(&with("privileged = true")).is_err());
        assert!(Harness::parse_manifest("name = ").is_err());
        assert!(Harness::parse_manifest("name = \"agent\"\n").is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        let invalid = [
            replace("name", "\"Agent X\""),
            replace("install", "[]"),
            replace("install", "[\"echo a\\nrm -rf /\"]"),
            replace("command", "[]"),
            replace("command", "[\"\"]"),
            replace("config_dir", "\"/root/.ssh\""),
            replace("config_dir", "\"../outside\""),
            replace("config_dir", "\".agent/../../x\""),
            replace("config_dir", "\"\""),
            replace("config_env", "\"HOME=/root\""),
            replace("auth_env", "[\"TOKEN=value\"]"),
            replace("auth_env", "[\"1TOKEN\"]"),
            replace("auth_env", "[\"\"]"),
            replace("config_files", "{ \"../.bashrc\" = \"x\" }"),
            replace("config_files", "{ \"/etc/passwd\" = \"x\" }"),
            replace("instructions_file", "\"docs/AGENTS.md\""),
            replace("instructions_file", "\"..\""),
            replace("model_args", "[\"--model\"]"),
            replace("model_args", "[\"--model\", \"\", \"{model}\"]"),
            replace("effort_args", "[\"--effort\", \"{model}\"]"),
            replace("instructions", "\"/etc/AGENTS.md\""),
            replace("skills", "\"../skills\""),
            replace(
                "agents",
                "{ dir = \"C:\\\\agents\", format = \"codex-toml\" }",
            ),
            replace("agents", "{ dir = \".agents\" }"),
            replace("mcp", "\"json\""),
            replace("hooks", "\"json\""),
            with("[inject.extra]"),
            format!("state_paths = [\".agent\"]\n{VALID}"),
        ];
        for text in invalid {
            assert!(
                Harness::parse_manifest(&text).is_err(),
                "should be invalid:\n{text}"
            );
        }
    }

    #[test]
    fn config_env_and_files_need_a_config_dir() {
        let without_dir: String = VALID
            .lines()
            .filter(|line| !line.starts_with("config_dir"))
            .collect::<Vec<_>>()
            .join("\n");

        let error = Harness::parse_manifest(&without_dir).unwrap_err();

        assert!(error.to_string().contains("need a config_dir"), "{error}");
    }

    #[test]
    fn unknown_formats_list_the_valid_ones() {
        let agents = Harness::parse_manifest(&replace(
            "agents",
            "{ dir = \".agents\", format = \"yaml\" }",
        ))
        .unwrap_err();
        let mcp = Harness::parse_manifest(&replace("mcp", "\"json\"")).unwrap_err();

        assert!(
            agents
                .to_string()
                .contains("expected `claude-markdown` or `codex-toml`")
        );
        assert!(
            mcp.to_string()
                .contains("expected `claude-mcp-config` or `codex-config-overrides`")
        );
    }

    #[test]
    fn an_unknown_hooks_mode_lists_the_valid_one() {
        // Given a manifest with hooks the code cannot render
        let text = replace("hooks", "\"git-hooks\"");

        // When it is parsed
        let error = Harness::parse_manifest(&text).unwrap_err();

        // Then the error names the valid mode
        assert!(
            error.to_string().contains("expected `claude-settings`"),
            "{error}"
        );
    }

    #[test]
    fn lists_the_supported_pieces() {
        let complete = Harness::parse_manifest(VALID).unwrap();
        let bare =
            Harness::parse_manifest("name = \"a\"\ninstall = [\"true\"]\ncommand = [\"a\"]\n")
                .unwrap();

        assert_eq!(
            complete.supported_pieces(),
            [
                "instructions",
                "skills",
                "agents",
                "mcp",
                "hooks",
                "model",
                "effort"
            ]
        );
        assert!(bare.supported_pieces().is_empty());
    }

    #[test]
    fn accepts_environment_variable_names() {
        for name in ["A", "_PRIVATE", "OPENAI_API_KEY", "key2"] {
            assert!(is_environment_variable_name(name), "{name}");
        }
    }
}
