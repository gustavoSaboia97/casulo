//! Value objects used by several groups. Everything here is already valid
//! once built.

pub mod casulo_home;
pub mod editor;
pub mod harness_name;
mod name;
pub mod project_name;
pub mod resource;
pub mod resource_name;
pub mod secret;
pub mod workspace_path;

// Re-exported so other modules can write `shared::domain::ProjectName`.
pub use casulo_home::CasuloHome;
pub use editor::{EditorCommand, EditorSettings, EditorSource};
pub use harness_name::HarnessName;
pub use project_name::ProjectName;
pub use resource::ResourceKind;
pub use resource_name::ResourceName;
pub use secret::Secret;
pub use workspace_path::WorkspacePath;
