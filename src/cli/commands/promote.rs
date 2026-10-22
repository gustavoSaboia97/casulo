//! `casulo promote memory topic`: turns a branch note into a general one.

use anyhow::Result;

use super::project_memory::{current_branch, open_memory};
use crate::application::memory::services::note_editing;
use crate::cli::args::{MemoryArgs, PromoteMemoryArgs, PromoteMemoryTarget};

use crate::application::memory;

/// `casulo promote memory`.
pub fn promote(arguments: PromoteMemoryArgs) -> Result<()> {
    match arguments.target {
        PromoteMemoryTarget::Topic { topic } => promote_memory_topic(arguments.memory, &topic),
    }
}

/// `casulo promote memory topic <topic>`.
fn promote_memory_topic(arguments: MemoryArgs, topic: &str) -> Result<()> {
    let mut opened = open_memory(arguments, true)?;
    let topic = memory::domain::Topic::parse(topic)?;
    let branch = current_branch(&opened.memory.workspace)?;
    let promoted = note_editing::promote_topic(
        &mut opened.memory,
        &crate::infrastructure::clock::SystemClock,
        &branch,
        &topic,
    )?;
    eprintln!("promoted {promoted} notes of topic {topic} from branch {branch}");
    Ok(())
}
