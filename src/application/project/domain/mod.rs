//! The configuration, where a project comes from, and its git settings.
//! Everything here is already valid once built.

pub mod config;
pub mod git_host;
pub mod project_source;
pub mod push;
pub mod repository_url;

// Re-exported so other modules can write `project::domain::GlobalConfig`.
pub use config::{
    ComposeSettings, GitIdentity, GlobalConfig, ImageSettings, MemorySettings, ModelSettings,
    ProjectConfig,
};
pub use git_host::GitHost;
pub use project_source::ProjectSource;
pub use push::{GitSettings, PushPolicy};
pub use repository_url::RepositoryUrl;
