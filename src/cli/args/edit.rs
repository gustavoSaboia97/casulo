//! Arguments of `casulo edit`.

use clap::Subcommand;

use super::MemoryArgs;
use crate::application::agent::domain::SkillFile;
use crate::application::shared::domain::ResourceName;

/// Arguments of `casulo edit`.
#[derive(Debug, clap::Args)]
pub struct EditArgs {
    /// What to change
    #[command(subcommand)]
    pub target: EditTarget,
}

/// What `casulo edit` changes.
#[derive(Debug, Subcommand)]
pub enum EditTarget {
    /// A subagent
    Agent(NameArgs),
    /// An Agent Skill, or one of its files with --file
    Skill(SkillArgs),
    /// An MCP server
    Mcp(NameArgs),
    /// A workflow of subagents; saving it writes its skill
    Workflow(NameArgs),
    /// Notes in the project's memory
    Memory(EditMemoryArgs),
}

/// The name of a subagent, skill, MCP server or workflow.
#[derive(Debug, clap::Args)]
pub struct NameArgs {
    /// Its name, such as code-reviewer
    #[arg(value_parser = ResourceName::parse)]
    pub name: ResourceName,
}

/// The name of a skill, and optionally one of its files.
#[derive(Debug, clap::Args)]
pub struct SkillArgs {
    /// Its name, such as release-notes
    #[arg(value_parser = ResourceName::parse)]
    pub name: ResourceName,
    /// A file inside the skill's folder instead of SKILL.md, such as assets/template.md
    #[arg(long, value_parser = SkillFile::parse)]
    pub file: Option<SkillFile>,
}

/// Arguments of `casulo edit memory`.
#[derive(Debug, clap::Args)]
pub struct EditMemoryArgs {
    #[command(flatten)]
    pub memory: MemoryArgs,
    /// What to edit
    #[command(subcommand)]
    pub target: EditMemoryTarget,
}

/// What `casulo edit memory` changes.
#[derive(Debug, Subcommand)]
pub enum EditMemoryTarget {
    /// A file's notes, in your editor; saving settles conflicts
    Note {
        /// File path relative to the repository root, such as src/main.rs
        path: String,
    },
    /// A project note: general, or of the current branch with --branch
    Topic(TopicArgs),
}

/// A project note's topic.
#[derive(Debug, clap::Args)]
pub struct TopicArgs {
    /// Its name, such as error-handling
    pub topic: String,
    /// The note of the current branch instead of the general one
    #[arg(long)]
    pub branch: bool,
}
