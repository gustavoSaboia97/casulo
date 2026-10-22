//! The kinds of resource in the user's agent setup, and how a skill is checked.

use std::fmt;

use anyhow::{Result, bail};

use super::resource_name::ResourceName;

/// A kind of resource under `~/.casulo/agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// A subagent, `agents/<name>.md`.
    Agent,
    /// An Agent Skill, `skills/<name>/SKILL.md`.
    Skill,
    /// An MCP server, `mcp/<name>.toml`.
    Mcp,
    /// A workflow of subagents, `workflows/<name>.toml`.
    Workflow,
}

impl ResourceKind {
    /// Every kind, in the order `casulo list` prints them.
    pub const ALL: [ResourceKind; 4] = [Self::Agent, Self::Skill, Self::Mcp, Self::Workflow];

    /// Extension of the file the user edits, so the editor highlights it.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Agent | Self::Skill => "md",
            Self::Mcp | Self::Workflow => "toml",
        }
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Self::Agent => "agent",
            Self::Skill => "skill",
            Self::Mcp => "mcp",
            Self::Workflow => "workflow",
        })
    }
}

/// Checks what the Agent Skills spec requires of a `SKILL.md`: YAML
/// frontmatter between `---` lines, a `name` equal to the folder name and a
/// non-empty `description`. Returns the description. It reads the
/// frontmatter line by line, so no YAML parser is needed.
pub fn check_skill(name: &ResourceName, text: &str) -> Result<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        bail!(
            "SKILL.md must start with a --- line, then name and description, then another --- line"
        );
    }
    let mut frontmatter = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        frontmatter.push(line);
    }
    if !closed {
        bail!("the frontmatter has no closing --- line");
    }
    let found_name = top_level_value(&frontmatter, "name").unwrap_or_default();
    if found_name != name.as_str() {
        bail!("name must be \"{name}\", the name of the skill's folder");
    }
    let description = top_level_value(&frontmatter, "description").unwrap_or_default();
    if description.is_empty() {
        bail!("description is empty; say what the skill does and when to use it");
    }
    Ok(description)
}

/// The value of a top-level `key: value` line, without quotes. A block
/// value (`key: >` or `key: |`) is read from the indented lines below it.
fn top_level_value(lines: &[&str], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let index = lines.iter().position(|line| line.starts_with(&prefix))?;
    let value = lines[index][prefix.len()..].trim();
    if value.starts_with('>') || value.starts_with('|') {
        let block: Vec<&str> = lines[index + 1..]
            .iter()
            .take_while(|line| line.starts_with([' ', '\t']) || line.trim().is_empty())
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        return Some(block.join(" "));
    }
    let unquoted = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value);
    Some(unquoted.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name() -> ResourceName {
        ResourceName::parse("release-notes").unwrap()
    }

    #[test]
    fn accepts_a_skill_and_returns_its_description() {
        let text = "---\nname: release-notes\ndescription: \"Write release notes\"\nlicense: MIT\n---\n\nBody.\n";

        assert_eq!(check_skill(&name(), text).unwrap(), "Write release notes");
    }

    #[test]
    fn reads_block_descriptions() {
        let text = "---\nname: 'release-notes'\ndescription: >\n  Write release\n  notes\nallowed-tools: Bash\n---\n";

        assert_eq!(check_skill(&name(), text).unwrap(), "Write release notes");
    }

    #[test]
    fn rejects_invalid_skills() {
        let invalid = [
            "no frontmatter",
            "---\nname: release-notes\ndescription: d\n",
            "---\nname: other\ndescription: d\n---\n",
            "---\nname: release-notes\n---\n",
            "---\nname: release-notes\ndescription:   \n---\n",
            "---\nmetadata:\n  name: release-notes\ndescription: d\n---\n",
        ];
        for text in invalid {
            assert!(
                check_skill(&name(), text).is_err(),
                "should be invalid:\n{text}"
            );
        }
    }
}
