//! `casulo list`: subagents, skills, MCP servers and workflows, or the
//! project's memory notes.

use anyhow::Result;

use super::project_memory::open_memory;
use crate::application::agent::services::resources;
use crate::application::memory::services::user_notes::RowState;
use crate::application::shared::domain::ResourceKind;
use crate::cli::args::{ListArgs, ListMemoryArgs, ListMemoryTarget, ListTarget};
use crate::cli::environment;
use crate::cli::output;

use crate::application::memory;

/// `casulo list`.
pub fn list(arguments: ListArgs) -> Result<()> {
    let kind = match arguments.target {
        None => None,
        Some(ListTarget::Agent) => Some(ResourceKind::Agent),
        Some(ListTarget::Skill) => Some(ResourceKind::Skill),
        Some(ListTarget::Mcp) => Some(ResourceKind::Mcp),
        Some(ListTarget::Workflow) => Some(ResourceKind::Workflow),
        Some(ListTarget::Memory(arguments)) => {
            return list_memory(arguments);
        }
    };
    let home = environment::casulo_home_from_environment()?;
    let kinds = match kind {
        Some(kind) => vec![kind],
        None => ResourceKind::ALL.to_vec(),
    };
    let mut summaries = Vec::new();
    for each in kinds {
        summaries.extend(resources::list(&home, each)?);
    }
    // With one kind the kind is known, so only the name and description.
    print!(
        "{}",
        output::resource_list::render(
            &summaries,
            kind.is_none(),
            output::resource_list::style_from_environment()
        )
    );
    Ok(())
}

/// `casulo list memory note|topic`.
fn list_memory(arguments: ListMemoryArgs) -> Result<()> {
    let opened = open_memory(arguments.memory, false)?;
    match arguments.target {
        ListMemoryTarget::Note {
            stale,
            conflict,
            all_versions,
        } => {
            for row in memory::services::user_notes::list_file_notes(
                &*opened.memory.database,
                all_versions,
            )? {
                let keep = match (stale, conflict) {
                    (false, false) => true,
                    _ => {
                        (stale && row.state == RowState::Stale)
                            || (conflict && row.state == RowState::Conflict)
                    }
                };
                if keep {
                    print!("{}", output::memory::format_note_row(&row));
                }
            }
        }
        ListMemoryTarget::Topic { branch } => {
            let current = memory::ports::Git::current_branch(
                &crate::infrastructure::git::GitCli,
                &opened.memory.workspace,
            )?;
            for note in opened.memory.database.project_notes(current.as_deref())? {
                if branch && note.branch.is_none() {
                    continue;
                }
                print!("{}", output::memory::format_topic_row(&note));
            }
        }
    }
    Ok(())
}
