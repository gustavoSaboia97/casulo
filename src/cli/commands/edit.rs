//! `casulo edit`: a subagent, skill, MCP server or workflow, or the project's
//! memory notes, in the editor.

use anyhow::Result;

use super::configured_editor;
use super::project_memory::{current_branch, open_memory};
use crate::application::agent::services::resources::{self, EditOutcome};
use crate::application::memory::services::note_editing;
use crate::application::shared::domain::{CasuloHome, ResourceKind};
use crate::cli::args::{
    EditArgs, EditMemoryTarget, EditTarget, MemoryArgs, NameArgs, ResourceArgs, SkillArgs,
    TopicArgs,
};
use crate::cli::environment;
use crate::infrastructure::editor::EditorProcess;

use crate::application::memory;

/// `casulo edit`.
pub fn edit(arguments: EditArgs) -> Result<()> {
    match arguments.target {
        EditTarget::Agent(target) => edit_resource(resource(ResourceKind::Agent, target), false),
        EditTarget::Skill(target) => edit_skill(target),
        EditTarget::Mcp(target) => edit_resource(resource(ResourceKind::Mcp, target), false),
        EditTarget::Workflow(target) => {
            edit_resource(resource(ResourceKind::Workflow, target), false)
        }
        EditTarget::Memory(arguments) => match arguments.target {
            EditMemoryTarget::Note { path } => edit_memory_note(arguments.memory, &path),
            EditMemoryTarget::Topic(topic) => edit_memory_topic(arguments.memory, topic),
        },
    }
}

/// `ResourceArgs` for `kind` and the name given on the command line.
fn resource(kind: ResourceKind, target: NameArgs) -> ResourceArgs {
    ResourceArgs {
        kind,
        name: target.name,
    }
}

/// `casulo add` (`create` is true) and `casulo edit`.
pub(super) fn edit_resource(arguments: ResourceArgs, create: bool) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let editor_command = environment::editor_from_environment(configured_editor(&home));
    let (kind, name) = (arguments.kind, &arguments.name);
    let outcome = if create {
        resources::add(&home, kind, name, &editor_command, &EditorProcess)?
    } else {
        resources::edit(&home, kind, name, &editor_command, &EditorProcess)?
    };
    match outcome {
        EditOutcome::Saved(path) => eprintln!("saved {}", path.display()),
        EditOutcome::Unchanged if create => {
            eprintln!("nothing saved, so {kind} \"{name}\" was not created")
        }
        EditOutcome::Unchanged => eprintln!("no changes to {kind} \"{name}\""),
    }
    Ok(())
}

/// `casulo edit skill`: its `SKILL.md`, or with `--file` one of its files.
fn edit_skill(arguments: SkillArgs) -> Result<()> {
    let Some(file) = arguments.file else {
        let target = NameArgs {
            name: arguments.name,
        };
        return edit_resource(resource(ResourceKind::Skill, target), false);
    };
    let name = &arguments.name;
    let home = environment::casulo_home_from_environment()?;
    let editor_command = environment::editor_from_environment(configured_editor(&home));
    let outcome = resources::edit_skill_file(&home, name, &file, &editor_command, &EditorProcess)?;
    match outcome {
        EditOutcome::Saved(path) => eprintln!("saved {}", path.display()),
        EditOutcome::Unchanged => eprintln!("no changes to \"{file}\" in skill \"{name}\""),
    }
    Ok(())
}

/// Opens `text` in the user's editor as a copy under `edits/`, and returns
/// the saved text and the copy, or `None` when nothing changed.
fn edit_in_editor(
    home: &CasuloHome,
    file_name: &str,
    text: &str,
) -> Result<Option<(String, std::path::PathBuf)>> {
    let editor_command = environment::editor_from_environment(configured_editor(home));
    note_editing::edit_copy(home, file_name, text, &editor_command, &EditorProcess)
}

/// `casulo edit memory note <path>`.
fn edit_memory_note(arguments: MemoryArgs, path: &str) -> Result<()> {
    let mut opened = open_memory(arguments, true)?;
    let path = memory::domain::RepoPath::parse(path)?;
    let text = note_editing::note_file_text(&*opened.memory.database, &path)?;
    let file_name = format!("memory-{}-note.md", opened.project);
    let Some((edited, copy)) = edit_in_editor(&opened.home, &file_name, &text)? else {
        eprintln!("no changes");
        return Ok(());
    };
    let changed = note_editing::save_note_edit(
        &mut opened.memory,
        &crate::infrastructure::git::GitCli,
        &crate::infrastructure::clock::SystemClock,
        &path,
        &edited,
        &copy,
    )?;
    eprintln!("saved {changed} notes of {path}");
    Ok(())
}

/// `casulo edit memory topic <topic> [--branch]`.
fn edit_memory_topic(arguments: MemoryArgs, topic_args: TopicArgs) -> Result<()> {
    let mut opened = open_memory(arguments, true)?;
    let topic = memory::domain::Topic::parse(&topic_args.topic)?;
    let branch = if topic_args.branch {
        Some(current_branch(&opened.memory.workspace)?)
    } else {
        None
    };
    let text = note_editing::topic_file_text(&*opened.memory.database, branch.as_deref(), &topic)?;
    let file_name = format!("memory-{}-topic-{topic}.md", opened.project);
    let Some((edited, copy)) = edit_in_editor(&opened.home, &file_name, &text)? else {
        eprintln!("no changes");
        return Ok(());
    };
    let changed = note_editing::save_topic_edit(
        &mut opened.memory,
        &crate::infrastructure::clock::SystemClock,
        branch.as_deref(),
        &topic,
        &edited,
        &copy,
    )?;
    eprintln!("saved {changed} notes of topic {topic}");
    Ok(())
}
