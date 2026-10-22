//! Arguments of `casulo list`.

use clap::Subcommand;

use super::MemoryArgs;

/// Arguments of `casulo list`.
#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// What to list [default: every subagent, skill, MCP server and workflow]
    #[command(subcommand)]
    pub target: Option<ListTarget>,
}

/// What `casulo list` shows.
#[derive(Debug, Subcommand)]
pub enum ListTarget {
    /// Subagents, with their descriptions
    Agent,
    /// Agent Skills, with their descriptions
    Skill,
    /// MCP servers, with their commands or URLs
    Mcp,
    /// Workflows of subagents, with their descriptions
    Workflow,
    /// Notes in the project's memory
    Memory(ListMemoryArgs),
}

/// Arguments of `casulo list memory`.
#[derive(Debug, clap::Args)]
pub struct ListMemoryArgs {
    #[command(flatten)]
    pub memory: MemoryArgs,
    /// What to list
    #[command(subcommand)]
    pub target: ListMemoryTarget,
}

/// What `casulo list memory` shows.
#[derive(Debug, Subcommand)]
pub enum ListMemoryTarget {
    /// One line per file and kind: path, kind, status, author, summary
    Note {
        /// Only notes written for another version of the file
        #[arg(long)]
        stale: bool,
        /// Only notes with two texts to settle
        #[arg(long)]
        conflict: bool,
        /// Every stored version, including notes kept for other branches
        #[arg(long)]
        all_versions: bool,
    },
    /// Project notes: general, and of the current branch
    Topic {
        /// Only the notes of the current branch
        #[arg(long)]
        branch: bool,
    },
}
