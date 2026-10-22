//! Traits for the outside world used by several groups, implemented in
//! `crate::infrastructure` and replaced by fakes in tests.

use std::path::Path;

use anyhow::Result;

use crate::application::shared::domain::EditorCommand;

/// A text editor the user works in.
pub trait Editor {
    /// Opens `file` in `editor` and waits until the user closes it.
    fn edit(&self, editor: &EditorCommand, file: &Path) -> Result<()>;
}
