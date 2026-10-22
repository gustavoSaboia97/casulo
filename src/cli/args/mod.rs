//! The command-line arguments, as clap reads them: one file per command.
//! Only definitions live here; reading the environment is in
//! `cli::environment`.

mod add;
mod compose;
mod config;
mod delete;
mod describe;
mod edit;
mod harness;
mod index;
mod list;
mod memory;
mod new;
mod promote;
mod run;
mod search;
mod stop;

use clap::builder::{EnumValueParser, TypedValueParser};
use clap::{Parser, Subcommand};

use crate::application::shared::domain::{HarnessName, ProjectName, ResourceKind, ResourceName};

pub use add::AddArgs;
pub use compose::{ComposeArgs, ComposeCommand};
pub use config::ConfigArgs;
pub use delete::DeleteArgs;
pub use describe::{DescribeCommand, DescribeMemoryArgs, MemoryResource};
pub use edit::{
    EditArgs, EditMemoryArgs, EditMemoryTarget, EditTarget, NameArgs, SkillArgs, TopicArgs,
};
pub use harness::HarnessCommand;
pub use index::IndexCommand;
pub use list::{ListArgs, ListMemoryArgs, ListMemoryTarget, ListTarget};
pub use memory::{MemoryCommand, ServeArgs};
pub use new::NewArgs;
pub use promote::{PromoteCommand, PromoteMemoryArgs, PromoteMemoryTarget};
pub use run::RunArgs;
pub use search::{SearchCommand, SearchMemoryArgs};
pub use stop::StopCommand;

/// Command-line arguments. clap reads the doc comments below as help text.
// `derive(Parser)` generates the argument parsing code from this struct, the
// same idea as declaring arguments with a decorator in Python's click.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands of casulo.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a project: clone a repository and prepare its environment
    New(NewArgs),
    /// Check that git and the container runtime are ready, and how to fix them
    Doctor,
    /// Create ~/.casulo with documented defaults, then install or fix what
    /// casulo needs, asking before each command
    Setup,
    /// Generate the project's Dockerfile for a harness and build the image
    Build(ProjectArgs),
    /// Run the agent in a hardened container with the workspace mounted
    Run(RunArgs),
    /// Open a bash shell in the same container the agent runs in
    Shell(RunArgs),
    /// Work with harnesses, the descriptions of how to install and run each agent
    #[command(subcommand)]
    Harness(HarnessCommand),
    /// Work with the services of the project's docker compose file
    #[command(subcommand)]
    Compose(ComposeCommand),
    /// Show details of a resource, such as the project's memory
    #[command(subcommand)]
    Describe(DescribeCommand),
    /// Update an index, such as the project's memory of its code
    #[command(subcommand)]
    Index(IndexCommand),
    /// Search the project's memory, the same way agents do with recall
    #[command(subcommand)]
    Search(SearchCommand),
    /// Open the global casulo.toml, or a project's, in your editor
    Config(ConfigArgs),
    /// Create a subagent, skill, MCP server or workflow from a template, in your editor
    Add(AddArgs),
    /// Change a subagent, skill, MCP server or workflow in your editor
    Edit(EditArgs),
    /// List your subagents, skills, MCP servers and workflows, or the project's memory notes
    List(ListArgs),
    /// Promote a branch note of the project's memory to a general one
    #[command(subcommand)]
    Promote(PromoteCommand),
    /// Stop a running service, such as the project's memory server
    #[command(subcommand)]
    Stop(StopCommand),
    /// Delete a subagent, skill, MCP server or workflow
    Delete(DeleteArgs),
    /// Print the casulo home directory
    Home,
    /// Internal commands of the memory container
    // Hidden: users never call these; `casulo run` starts them in the memory container.
    #[command(subcommand, hide = true)]
    Memory(MemoryCommand),
}

/// Arguments of `casulo add` and `casulo delete`.
#[derive(Debug, clap::Args)]
pub struct ResourceArgs {
    /// What to work on
    // clap reads a `KindArgument`, and `map` turns it into the domain's
    // `ResourceKind`, so the logic layer does not depend on clap.
    #[arg(value_parser = EnumValueParser::<KindArgument>::new().map(ResourceKind::from))]
    pub kind: ResourceKind,
    /// Its name, such as code-reviewer
    #[arg(value_parser = ResourceName::parse)]
    pub name: ResourceName,
}

/// A kind of resource, as written on the command line.
// `ValueEnum` lets clap accept `agent`, `skill`, `mcp` or `workflow` on the command line
// and list them in `--help`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum KindArgument {
    /// A subagent, `agents/<name>.md`.
    Agent,
    /// An Agent Skill, `skills/<name>/SKILL.md`.
    Skill,
    /// An MCP server, `mcp/<name>.toml`.
    Mcp,
    /// A workflow of subagents, `workflows/<name>.toml`.
    Workflow,
}

impl From<KindArgument> for ResourceKind {
    fn from(kind: KindArgument) -> Self {
        match kind {
            KindArgument::Agent => Self::Agent,
            KindArgument::Skill => Self::Skill,
            KindArgument::Mcp => Self::Mcp,
            KindArgument::Workflow => Self::Workflow,
        }
    }
}

/// Arguments shared by the memory commands.
#[derive(Debug, clap::Args)]
pub struct MemoryArgs {
    /// Project to use [default: the project of the current directory]
    // `global` lets it come after a subcommand too: `describe memory note x --project p`.
    #[arg(long, global = true, value_parser = ProjectName::parse)]
    pub project: Option<ProjectName>,
}

/// Arguments that choose a project and a harness.
#[derive(Debug, clap::Args)]
pub struct ProjectArgs {
    /// Project to use [default: the project of the current directory]
    #[arg(long, value_parser = ProjectName::parse)]
    pub project: Option<ProjectName>,
    /// Harness to use [default: the project's, else the global default]
    #[arg(long, value_parser = HarnessName::parse)]
    pub harness: Option<HarnessName>,
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::*;

    #[test]
    fn every_resource_kind_can_be_written_on_the_command_line() {
        let kinds: Vec<ResourceKind> = KindArgument::value_variants()
            .iter()
            .map(|&kind| ResourceKind::from(kind))
            .collect();

        assert_eq!(kinds, ResourceKind::ALL);
    }
}
