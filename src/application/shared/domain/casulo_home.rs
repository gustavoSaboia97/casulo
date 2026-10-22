//! Where casulo keeps its files on the host, and the layout inside that directory.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::harness_name::HarnessName;
use super::project_name::ProjectName;
use super::resource::ResourceKind;
use super::resource_name::ResourceName;

/// Name of the casulo directory inside the user home.
const DEFAULT_DIRECTORY_NAME: &str = ".casulo";

/// The casulo home directory (`~/.casulo` by default) and the paths inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasuloHome {
    root: PathBuf,
}

impl CasuloHome {
    /// Returns the casulo home: the override when it is set and not empty,
    /// otherwise `.casulo` inside the user home.
    ///
    /// Returns `None` when there is no usable override and the user home is unknown.
    pub fn resolve(
        // `Option` is Rust's explicit "maybe a value", like `Optional` in Python,
        // but the compiler forces every caller to handle the `None` case.
        home_override: Option<OsString>,
        user_home: Option<PathBuf>,
    ) -> Option<Self> {
        // `OsString`, not `String`: environment values are not guaranteed to be
        // valid UTF-8, and paths must keep working when they are not.
        let root = match home_override {
            Some(directory) if !directory.is_empty() => PathBuf::from(directory),
            _ => user_home?.join(DEFAULT_DIRECTORY_NAME),
        };
        Some(Self { root })
    }

    /// The casulo home directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The global configuration file, `config/casulo.toml`.
    pub fn global_config_file(&self) -> PathBuf {
        self.root.join("config").join("casulo.toml")
    }

    /// The directory with the user's agent configuration, `agent/`: the
    /// instructions, skills, subagents and MCP servers given to every harness.
    pub fn agent_dir(&self) -> PathBuf {
        self.root.join("agent")
    }

    /// Instructions for every agent, `agent/AGENTS.md`.
    pub fn agent_instructions_file(&self) -> PathBuf {
        self.agent_dir().join("AGENTS.md")
    }

    /// Agent Skills, one folder each, `agent/skills/`.
    pub fn skills_dir(&self) -> PathBuf {
        self.agent_dir().join("skills")
    }

    /// Subagents, one file each, `agent/agents/`.
    pub fn agents_dir(&self) -> PathBuf {
        self.agent_dir().join("agents")
    }

    /// MCP servers, one file each, `agent/mcp/`.
    pub fn mcp_dir(&self) -> PathBuf {
        self.agent_dir().join("mcp")
    }

    /// Workflows of subagents, one file each, `agent/workflows/`.
    pub fn workflows_dir(&self) -> PathBuf {
        self.agent_dir().join("workflows")
    }

    /// The file the user edits for one resource: `agents/<name>.md`,
    /// `skills/<name>/SKILL.md`, `mcp/<name>.toml` or `workflows/<name>.toml`
    /// under `agent/`.
    pub fn resource_file(&self, kind: ResourceKind, name: &ResourceName) -> PathBuf {
        match kind {
            ResourceKind::Agent => self.agents_dir().join(format!("{name}.md")),
            ResourceKind::Skill => self.skills_dir().join(name.as_str()).join("SKILL.md"),
            ResourceKind::Mcp => self.mcp_dir().join(format!("{name}.toml")),
            ResourceKind::Workflow => self.workflows_dir().join(format!("{name}.toml")),
        }
    }

    /// Where `casulo add` and `edit` keep the copy being edited, `edits/`,
    /// so an invalid file never replaces a valid one.
    pub fn edits_dir(&self) -> PathBuf {
        self.root.join("edits")
    }

    /// The directory that holds every project, `projects/`.
    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    /// Where the memory image's Dockerfile is generated, `build/memory/`.
    /// Shared by every project, since the image is the same for all.
    pub fn memory_build_dir(&self) -> PathBuf {
        self.root.join("build").join("memory")
    }

    /// The directory with harness manifest overrides, `harnesses/`.
    pub fn harnesses_dir(&self) -> PathBuf {
        self.root.join("harnesses")
    }

    /// The directory of one project, `projects/<name>/`.
    pub fn project_dir(&self, project: &ProjectName) -> PathBuf {
        self.projects_dir().join(project.as_str())
    }

    /// The configuration file of one project, `projects/<name>/casulo.toml`.
    pub fn project_config_file(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("casulo.toml")
    }

    /// The clone of the project repository, `projects/<name>/workspace/`.
    pub fn workspace_dir(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("workspace")
    }

    /// Where the Dockerfile of one harness is generated and built from,
    /// `projects/<name>/build/<harness>/`. It holds nothing else, so the
    /// image build never sees the workspace.
    pub fn build_dir(&self, project: &ProjectName, harness: &HarnessName) -> PathBuf {
        self.project_dir(project)
            .join("build")
            .join(harness.as_str())
    }

    /// Files casulo renders for one harness on every run, such as
    /// subagents, `projects/<name>/generated/<harness>/`.
    pub fn generated_dir(&self, project: &ProjectName, harness: &HarnessName) -> PathBuf {
        self.project_dir(project)
            .join("generated")
            .join(harness.as_str())
    }

    /// The git config and hook that let the agent push, mounted at
    /// `/etc/casulo` only when push is enabled, `projects/<name>/push/`.
    pub fn push_dir(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("push")
    }

    /// The project's memory, `projects/<name>/memory/`. Written only by the
    /// memory server and never mounted into an agent container.
    pub fn memory_dir(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("memory")
    }

    /// Notes as Markdown, `memory/notes/`: the only part of memory that
    /// cannot be rebuilt.
    pub fn memory_notes_dir(&self, project: &ProjectName) -> PathBuf {
        self.memory_dir(project).join("notes")
    }

    /// The memory index, `memory/memory.sqlite`, built from the code and the
    /// notes.
    pub fn memory_database_file(&self, project: &ProjectName) -> PathBuf {
        self.memory_dir(project).join("memory.sqlite")
    }

    /// The compose model the user approved, `projects/<name>/compose/`.
    pub fn compose_dir(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("compose")
    }

    /// Empty files and directories that cover masked paths,
    /// `projects/<name>/masks/`.
    pub fn masks_dir(&self, project: &ProjectName) -> PathBuf {
        self.project_dir(project).join("masks")
    }

    /// The project whose directory contains `path`, if any. Both this home
    /// and `path` should be canonical, so symbolic links do not hide a match.
    pub fn project_containing(&self, path: &Path) -> Option<ProjectName> {
        let inside_projects = path.strip_prefix(self.projects_dir()).ok()?;
        let first = inside_projects.components().next()?;
        ProjectName::parse(first.as_os_str().to_str()?).ok()
    }

    /// The agent's login, settings and history in one project,
    /// `projects/<name>/config/<harness>/`, mounted at the harness's
    /// `config_dir` on every run. Kept outside the workspace, so the
    /// repository can never ship it.
    pub fn agent_config_dir(&self, project: &ProjectName, harness: &HarnessName) -> PathBuf {
        self.project_dir(project)
            .join("config")
            .join(harness.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from("home")), None).expect("override is set")
    }

    #[test]
    fn uses_override_when_set() {
        let home = CasuloHome::resolve(
            Some(OsString::from("custom-home")),
            Some(PathBuf::from("user")),
        )
        .expect("override is set");

        assert_eq!(home.root(), Path::new("custom-home"));
    }

    #[test]
    fn uses_dot_casulo_in_user_home_when_override_is_missing() {
        let home =
            CasuloHome::resolve(None, Some(PathBuf::from("user"))).expect("user home is set");

        assert_eq!(home.root(), PathBuf::from("user").join(".casulo"));
    }

    #[test]
    fn ignores_empty_override() {
        let home = CasuloHome::resolve(Some(OsString::new()), Some(PathBuf::from("user")))
            .expect("user home is set");

        assert_eq!(home.root(), PathBuf::from("user").join(".casulo"));
    }

    #[test]
    fn uses_override_even_without_user_home() {
        let home = CasuloHome::resolve(Some(OsString::from("custom-home")), None);

        assert!(home.is_some());
    }

    #[test]
    fn returns_none_without_override_or_user_home() {
        assert_eq!(CasuloHome::resolve(None, None), None);
        assert_eq!(CasuloHome::resolve(Some(OsString::new()), None), None);
    }

    #[test]
    fn builds_the_documented_layout() {
        let home = home();
        let project = ProjectName::parse("demo").expect("valid name");
        let harness = HarnessName::parse("claude").expect("valid name");
        let project_dir = PathBuf::from("home").join("projects").join("demo");

        assert_eq!(
            home.global_config_file(),
            PathBuf::from("home").join("config").join("casulo.toml")
        );
        assert_eq!(
            home.harnesses_dir(),
            PathBuf::from("home").join("harnesses")
        );
        assert_eq!(home.project_dir(&project), project_dir);
        assert_eq!(
            home.project_config_file(&project),
            project_dir.join("casulo.toml")
        );
        assert_eq!(home.workspace_dir(&project), project_dir.join("workspace"));
        assert_eq!(
            home.agent_config_dir(&project, &harness),
            project_dir.join("config").join("claude")
        );
        assert_eq!(
            home.build_dir(&project, &harness),
            project_dir.join("build").join("claude")
        );
    }

    #[test]
    fn memory_lives_in_the_project_outside_the_workspace() {
        let project = ProjectName::parse("demo").unwrap();
        let memory = home().project_dir(&project).join("memory");

        assert_eq!(home().memory_dir(&project), memory);
        assert_eq!(home().memory_notes_dir(&project), memory.join("notes"));
        assert_eq!(
            home().memory_database_file(&project),
            memory.join("memory.sqlite")
        );
        assert!(!memory.starts_with(home().workspace_dir(&project)));
    }

    #[test]
    fn finds_the_project_containing_a_path() {
        let home = home();
        let projects = PathBuf::from("home").join("projects");

        assert_eq!(
            home.project_containing(&projects.join("demo").join("workspace").join("src")),
            Some(ProjectName::parse("demo").unwrap())
        );
        assert_eq!(
            home.project_containing(&projects.join("demo")),
            Some(ProjectName::parse("demo").unwrap())
        );
        assert_eq!(home.project_containing(&projects), None);
        assert_eq!(
            home.project_containing(&PathBuf::from("elsewhere").join("demo")),
            None
        );
        assert_eq!(home.project_containing(&projects.join("Not Valid")), None);
    }
}
