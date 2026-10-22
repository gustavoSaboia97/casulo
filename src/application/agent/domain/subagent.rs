//! Subagents: specialized agents the main agent can hand work to, written
//! once in casulo's format and rendered for each harness.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::harness::AgentsFormat;
use crate::application::shared::domain::{HarnessName, ResourceName};

/// The line that opens and closes the frontmatter.
const FRONTMATTER_FENCE: &str = "+++";

/// A validated subagent from `~/.casulo/agent/agents/<name>.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subagent {
    /// Name, equal to the file name without `.md`.
    pub name: ResourceName,
    /// When the main agent should use it. Never empty.
    pub description: String,
    /// The Markdown after the frontmatter. Never empty.
    pub instructions: String,
    /// The model for each harness that sets one.
    pub models: Vec<HarnessModel>,
}

/// The model a subagent uses in one harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessModel {
    /// The harness it applies to.
    pub harness: HarnessName,
    /// Model name, passed through as written, since only the harness knows its models.
    pub model: String,
    /// Reasoning effort, passed through as written.
    pub effort: Option<String>,
}

/// The frontmatter exactly as written in TOML, before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    model: BTreeMap<String, RawModel>,
}

/// A model is either a name or a table with a name and an effort.
// `untagged` tries each variant in order, so `claude = "haiku"` and
// `codex = { model = "...", effort = "low" }` both work.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawModel {
    Name(String),
    WithEffort(RawModelWithEffort),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModelWithEffort {
    model: String,
    effort: Option<String>,
}

impl Subagent {
    /// Parses a subagent file. `file_name` is its name without `.md`, which
    /// must match the `name` in the frontmatter.
    pub fn parse(file_name: &str, text: &str) -> Result<Self> {
        let (frontmatter, body) = split_frontmatter(text)?;
        let raw: RawFrontmatter = toml::from_str(frontmatter)?;
        let name = ResourceName::parse(&raw.name)?;
        if name.as_str() != file_name {
            bail!("it is named \"{name}\", so the file must be {name}.md");
        }
        let description = raw.description.trim().to_string();
        if description.is_empty() {
            bail!("description is empty; say when the main agent should use this subagent");
        }
        let instructions = body.trim().to_string();
        if instructions.is_empty() {
            bail!("the instructions after the closing {FRONTMATTER_FENCE} are empty");
        }
        let mut models = Vec::new();
        for (harness, model) in raw.model {
            let harness = HarnessName::parse(&harness).context("in [model]")?;
            let (model, effort) = match model {
                RawModel::Name(model) => (model, None),
                RawModel::WithEffort(table) => (table.model, table.effort),
            };
            if model.trim().is_empty()
                || effort
                    .as_ref()
                    .is_some_and(|effort| effort.trim().is_empty())
            {
                bail!("[model] {harness}: the model and effort cannot be empty");
            }
            models.push(HarnessModel {
                harness,
                model,
                effort,
            });
        }
        Ok(Self {
            name,
            description,
            instructions,
            models,
        })
    }

    /// The model for `harness`, if the subagent sets one.
    pub fn model_for(&self, harness: &HarnessName) -> Option<&HarnessModel> {
        self.models.iter().find(|model| &model.harness == harness)
    }

    /// The file name and text of this subagent in `format`, with the model
    /// it sets for `harness`. Without one, the model fields are left out and
    /// the subagent uses the session model.
    pub fn render(&self, format: AgentsFormat, harness: &HarnessName) -> (String, String) {
        let model = self.model_for(harness);
        match format {
            AgentsFormat::ClaudeMarkdown => {
                (format!("{}.md", self.name), self.to_claude_markdown(model))
            }
            AgentsFormat::CodexToml => (format!("{}.toml", self.name), self.to_codex_toml(model)),
        }
    }

    /// Markdown with YAML frontmatter, as Claude Code reads it. Strings are
    /// double-quoted and escaped, so no YAML parser or crate is needed.
    fn to_claude_markdown(&self, model: Option<&HarnessModel>) -> String {
        let mut frontmatter = format!(
            "name: {}\ndescription: {}\n",
            yaml_string(self.name.as_str()),
            yaml_string(&self.description)
        );
        if let Some(model) = model {
            frontmatter.push_str(&format!("model: {}\n", yaml_string(&model.model)));
            if let Some(effort) = &model.effort {
                frontmatter.push_str(&format!("effort: {}\n", yaml_string(effort)));
            }
        }
        format!("---\n{frontmatter}---\n\n{}\n", self.instructions)
    }

    /// A TOML file, as Codex reads it.
    fn to_codex_toml(&self, model: Option<&HarnessModel>) -> String {
        #[derive(Serialize)]
        struct CodexAgent {
            name: String,
            description: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            model: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            model_reasoning_effort: Option<String>,
            developer_instructions: String,
        }
        let agent = CodexAgent {
            name: self.name.to_string(),
            description: self.description.clone(),
            model: model.map(|model| model.model.clone()),
            model_reasoning_effort: model.and_then(|model| model.effort.clone()),
            developer_instructions: self.instructions.clone(),
        };
        toml::to_string(&agent).expect("plain strings always serialize to TOML")
    }
}

/// Splits `text` into the frontmatter between the first two `+++` lines and
/// the rest.
fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let missing = || {
        anyhow::anyhow!(
            "the file must start with a {FRONTMATTER_FENCE} line, then the TOML settings, then another {FRONTMATTER_FENCE} line"
        )
    };
    let after_open = text
        .strip_prefix(FRONTMATTER_FENCE)
        .and_then(|rest| {
            rest.strip_prefix("\r\n")
                .or_else(|| rest.strip_prefix('\n'))
        })
        .ok_or_else(missing)?;
    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        if line.trim_end() == FRONTMATTER_FENCE {
            let frontmatter = &after_open[..offset];
            let body = &after_open[offset + line.len()..];
            return Ok((frontmatter, body));
        }
        offset += line.len();
    }
    Err(missing())
}

/// `value` as a YAML double-quoted string.
fn yaml_string(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            character if character.is_control() => {
                quoted.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVIEWER: &str = r#"+++
name = "reviewer"
description = "Reviews the diff for bugs and security problems"

[model]
claude = "haiku"
codex = { model = "gpt-5-codex-mini", effort = "low" }
+++

You are a code reviewer. Read the diff and report only real problems.
"#;

    #[test]
    fn parses_frontmatter_instructions_and_models() {
        let agent = Subagent::parse("reviewer", REVIEWER).unwrap();

        assert_eq!(agent.name.as_str(), "reviewer");
        assert_eq!(
            agent.description,
            "Reviews the diff for bugs and security problems"
        );
        assert_eq!(
            agent.instructions,
            "You are a code reviewer. Read the diff and report only real problems."
        );
        let claude = agent
            .model_for(&HarnessName::parse("claude").unwrap())
            .unwrap();
        assert_eq!(
            (claude.model.as_str(), claude.effort.as_deref()),
            ("haiku", None)
        );
        let codex = agent
            .model_for(&HarnessName::parse("codex").unwrap())
            .unwrap();
        assert_eq!(codex.model, "gpt-5-codex-mini");
        assert_eq!(codex.effort.as_deref(), Some("low"));
    }

    #[test]
    fn accepts_windows_line_endings_and_no_models() {
        let text = "+++\r\nname = \"a\"\r\ndescription = \"d\"\r\n+++\r\nDo it.\r\n";

        let agent = Subagent::parse("a", text).unwrap();

        assert_eq!(agent.instructions, "Do it.");
        assert!(agent.models.is_empty());
    }

    #[test]
    fn rejects_invalid_files() {
        let valid_frontmatter = "name = \"a\"\ndescription = \"d\"\n";
        let invalid = [
            ("a", "no frontmatter at all".to_string()),
            ("a", format!("+++\n{valid_frontmatter}no closing fence\n")),
            ("a", format!("---\n{valid_frontmatter}---\nbody\n")),
            ("b", format!("+++\n{valid_frontmatter}+++\nbody\n")),
            ("a", format!("+++\n{valid_frontmatter}+++\n\n  \n")),
            (
                "a",
                "+++\nname = \"a\"\ndescription = \" \"\n+++\nbody\n".to_string(),
            ),
            (
                "a",
                format!("+++\n{valid_frontmatter}tools = [\"Bash\"]\n+++\nbody\n"),
            ),
            (
                "a",
                format!("+++\n{valid_frontmatter}[model]\nclaude = \"\"\n+++\nbody\n"),
            ),
            (
                "a",
                format!(
                    "+++\n{valid_frontmatter}[model]\nclaude = {{ name = \"x\" }}\n+++\nbody\n"
                ),
            ),
            (
                "a",
                format!("+++\n{valid_frontmatter}[model]\n\"../x\" = \"m\"\n+++\nbody\n"),
            ),
            (
                "a--b",
                "+++\nname = \"a--b\"\ndescription = \"d\"\n+++\nbody\n".to_string(),
            ),
        ];
        for (file_name, text) in invalid {
            assert!(
                Subagent::parse(file_name, &text).is_err(),
                "should be invalid:\n{text}"
            );
        }
    }

    #[test]
    fn file_name_must_match() {
        let error = Subagent::parse("other", REVIEWER).unwrap_err();

        assert_eq!(
            error.to_string(),
            "it is named \"reviewer\", so the file must be reviewer.md"
        );
    }

    fn claude() -> HarnessName {
        HarnessName::parse("claude").unwrap()
    }

    fn codex() -> HarnessName {
        HarnessName::parse("codex").unwrap()
    }

    #[test]
    fn renders_the_model_of_each_harness() {
        let agent = Subagent::parse("reviewer", REVIEWER).unwrap();

        for (format, harness) in [
            (AgentsFormat::ClaudeMarkdown, claude()),
            (AgentsFormat::CodexToml, codex()),
        ] {
            let (_, text) = agent.render(format, &harness);
            insta::assert_snapshot!(format!("reviewer_for_{harness}"), text);
        }
    }

    #[test]
    fn leaves_the_model_out_for_a_harness_without_one() {
        let agent = Subagent::parse("reviewer", REVIEWER).unwrap();
        let other = HarnessName::parse("other").unwrap();

        let (_, markdown) = agent.render(AgentsFormat::ClaudeMarkdown, &other);
        let (_, toml_text) = agent.render(AgentsFormat::CodexToml, &other);

        assert!(!markdown.contains("model"));
        assert!(!toml_text.contains("model"));
    }

    fn tricky() -> Subagent {
        let text = "+++\nname = \"tricky\"\ndescription = \"\"\"Says \"hi\" \\\\ and\nspans: lines\"\"\"\n+++\nLine one.\n\nLine \"two\".\n";
        Subagent::parse("tricky", text).unwrap()
    }

    #[test]
    fn renders_claude_markdown_with_escaped_strings() {
        let (file_name, text) = tricky().render(AgentsFormat::ClaudeMarkdown, &claude());

        assert_eq!(file_name, "tricky.md");
        insta::assert_snapshot!(text);
    }

    #[test]
    fn renders_codex_toml_that_reads_back() {
        let agent = tricky();
        let (file_name, text) = agent.render(AgentsFormat::CodexToml, &codex());

        assert_eq!(file_name, "tricky.toml");
        insta::assert_snapshot!(text);
        let table: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(
            table["description"].as_str(),
            Some(agent.description.as_str())
        );
        assert_eq!(
            table["developer_instructions"].as_str(),
            Some(agent.instructions.as_str())
        );
    }

    #[test]
    fn yaml_strings_escape_quotes_backslashes_and_control_characters() {
        assert_eq!(
            yaml_string("a \"b\" \\ c\n\td\u{1}"),
            "\"a \\\"b\\\" \\\\ c\\n\\td\\u0001\""
        );
    }
}
