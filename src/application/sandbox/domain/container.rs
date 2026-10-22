//! Fixed facts about the container casulo builds and runs.

use crate::application::shared::domain::{HarnessName, ProjectName};

/// Unprivileged user the agent runs as inside the container.
pub const CONTAINER_USER: &str = "casulo";

/// User and group id of that user in the image.
pub const CONTAINER_UID: u32 = 1000;

/// Home directory of that user inside the container.
pub const CONTAINER_HOME: &str = "/home/casulo";

/// Where the project workspace is mounted inside the container.
pub const CONTAINER_WORKSPACE: &str = "/workspace";

/// Name of the image built for one project and harness, such as `casulo/demo:claude`.
pub fn image_tag(project: &ProjectName, harness: &HarnessName) -> String {
    format!("casulo/{project}:{harness}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_tag_combines_project_and_harness() {
        let project = ProjectName::parse("demo").unwrap();
        let harness = HarnessName::parse("claude").unwrap();

        assert_eq!(image_tag(&project, &harness), "casulo/demo:claude");
    }
}
