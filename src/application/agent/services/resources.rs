//! `casulo add|edit|list|delete`: manage the subagents, skills, MCP
//! servers and workflows under `~/.casulo/agent`, one file or folder each.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::workflows;
use crate::application::agent::domain::mcp_server::McpTransport;
use crate::application::agent::domain::{McpServer, SkillFile, Subagent, Workflow};
use crate::application::shared::domain::resource::check_skill;
use crate::application::shared::domain::{CasuloHome, EditorCommand, ResourceKind, ResourceName};
use crate::application::shared::ports::Editor;

const AGENT_TEMPLATE: &str = include_str!("../../../../templates/agent.md");
const SKILL_TEMPLATE: &str = include_str!("../../../../templates/skill.md");
const MCP_TEMPLATE: &str = include_str!("../../../../templates/mcp.toml");
const WORKFLOW_TEMPLATE: &str = include_str!("../../../../templates/workflow.toml");

/// Lines casulo puts at the top of a file it reopens, to show the error.
const ERROR_PREFIX: &str = "# casulo: ";

/// What happened to the file after the editor closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOutcome {
    /// A valid file was written here.
    Saved(PathBuf),
    /// The user closed the editor without changes, so nothing was written.
    Unchanged,
}

/// One resource for `casulo list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSummary {
    /// Its kind.
    pub kind: ResourceKind,
    /// Its name.
    pub name: String,
    /// What it is for, or why it is invalid.
    pub description: String,
}

/// The commented template a new resource starts from.
pub fn template(kind: ResourceKind, name: &ResourceName) -> String {
    let template = match kind {
        ResourceKind::Agent => AGENT_TEMPLATE,
        ResourceKind::Skill => SKILL_TEMPLATE,
        ResourceKind::Mcp => MCP_TEMPLATE,
        ResourceKind::Workflow => WORKFLOW_TEMPLATE,
    };
    template.replace("{name}", name.as_str())
}

/// Checks a resource file with the same code `casulo run` uses, and returns
/// what `casulo list` shows for it. `agents` are the subagents a workflow
/// may use; the other kinds ignore them.
pub fn validate(
    kind: ResourceKind,
    name: &ResourceName,
    text: &str,
    agents: &[ResourceName],
) -> Result<String> {
    match kind {
        ResourceKind::Agent => Ok(Subagent::parse(name.as_str(), text)?.description),
        ResourceKind::Skill => check_skill(name, text),
        ResourceKind::Mcp => Ok(describe_mcp(&McpServer::parse(name.as_str(), text)?)),
        ResourceKind::Workflow => Ok(Workflow::parse(name.as_str(), text, agents)?.description),
    }
}

/// Creates a resource from its template in the editor. Fails if it exists.
pub fn add(
    home: &CasuloHome,
    kind: ResourceKind,
    name: &ResourceName,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let path = home.resource_file(kind, name);
    if path.exists() {
        bail!("{kind} \"{name}\" already exists; change it with `casulo edit {kind} {name}`");
    }
    edit_until_valid(
        home,
        kind,
        name,
        &template(kind, name),
        editor_command,
        editor,
    )
}

/// Opens an existing resource in the editor.
pub fn edit(
    home: &CasuloHome,
    kind: ResourceKind,
    name: &ResourceName,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let path = home.resource_file(kind, name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            bail!("{kind} \"{name}\" does not exist; create it with `casulo add {kind} {name}`")
        }
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    edit_until_valid(home, kind, name, &text, editor_command, editor)
}

/// Creates a supporting file in a skill's folder, such as a template or
/// examples, in the editor. The skill must exist and the file must not.
pub fn add_skill_file(
    home: &CasuloHome,
    name: &ResourceName,
    file: &SkillFile,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let path = skill_file_path(home, name, file)?;
    if path.exists() {
        bail!(
            "\"{file}\" already exists in skill \"{name}\"; change it with `casulo edit skill {name} --file {file}`"
        );
    }
    edit_skill_file_text(home, name, file, &path, "", editor_command, editor)
}

/// Opens a supporting file of a skill in the editor.
pub fn edit_skill_file(
    home: &CasuloHome,
    name: &ResourceName,
    file: &SkillFile,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let path = skill_file_path(home, name, file)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => bail!(
            "\"{file}\" does not exist in skill \"{name}\"; create it with `casulo add skill {name} --file {file}`"
        ),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    edit_skill_file_text(home, name, file, &path, &text, editor_command, editor)
}

/// Where `file` lives in skill `name`'s folder. Fails when the skill does
/// not exist, so a file never creates a skill without a `SKILL.md`.
fn skill_file_path(home: &CasuloHome, name: &ResourceName, file: &SkillFile) -> Result<PathBuf> {
    if !home.resource_file(ResourceKind::Skill, name).is_file() {
        bail!("skill \"{name}\" does not exist; create it with `casulo add skill {name}`");
    }
    Ok(file.in_folder(&home.skills_dir().join(name.as_str())))
}

/// Edits a copy of `text` and writes it to `path` if it changed. A
/// supporting file can hold anything, so there is nothing to validate.
fn edit_skill_file_text(
    home: &CasuloHome,
    name: &ResourceName,
    file: &SkillFile,
    path: &Path,
    text: &str,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let edits_dir = home.edits_dir();
    fs::create_dir_all(&edits_dir)
        .with_context(|| format!("cannot create {}", edits_dir.display()))?;
    // The same file name keeps the extension, so the editor highlights it.
    let copy = edits_dir.join(format!("skill-{name}-{}", file.file_name()));
    fs::write(&copy, text).with_context(|| format!("cannot write {}", copy.display()))?;
    let result = editor.edit(editor_command, &copy).and_then(|()| {
        fs::read_to_string(&copy).with_context(|| format!("cannot read {}", copy.display()))
    });
    // Best effort: the outcome matters more than leaving a stale copy behind.
    let _ = fs::remove_file(&copy);
    let saved = result?;
    // Like an empty `git commit` message: nothing to save means give up.
    if saved.trim().is_empty() || saved == text {
        return Ok(EditOutcome::Unchanged);
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    fs::write(path, saved).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(EditOutcome::Saved(path.to_path_buf()))
}

/// Edits a copy of `text` until it is valid or saved unchanged, as
/// `kubectl edit` does. An invalid save is reopened with the error at the
/// top. Only a valid file reaches the resource's real path.
///
/// A workflow also writes its skill, even when unchanged, so opening and
/// closing it brings a stale skill up to date.
fn edit_until_valid(
    home: &CasuloHome,
    kind: ResourceKind,
    name: &ResourceName,
    text: &str,
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<EditOutcome> {
    let agents = if kind == ResourceKind::Workflow {
        // Before the editor opens, so no work is lost to a name already taken.
        workflows::ensure_skill_is_replaceable(home, name)?;
        workflows::agent_names(home)?
    } else {
        Vec::new()
    };
    let edits_dir = home.edits_dir();
    fs::create_dir_all(&edits_dir)
        .with_context(|| format!("cannot create {}", edits_dir.display()))?;
    let copy = edits_dir.join(format!("{kind}-{name}.{}", kind.extension()));
    let result = edit_loop(&copy, kind, name, text, &agents, editor_command, editor);
    // Best effort: the outcome matters more than leaving a stale copy behind.
    let _ = fs::remove_file(&copy);
    let path = home.resource_file(kind, name);
    let outcome = match result? {
        Some(valid) => {
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory)
                    .with_context(|| format!("cannot create {}", directory.display()))?;
            }
            fs::write(&path, valid).with_context(|| format!("cannot write {}", path.display()))?;
            EditOutcome::Saved(path.clone())
        }
        None => EditOutcome::Unchanged,
    };
    if kind == ResourceKind::Workflow && path.is_file() {
        workflows::refresh_skill(home, name)?;
    }
    Ok(outcome)
}

/// Returns the valid text, or `None` when the user gave up.
fn edit_loop(
    copy: &Path,
    kind: ResourceKind,
    name: &ResourceName,
    text: &str,
    agents: &[ResourceName],
    editor_command: &EditorCommand,
    editor: &dyn Editor,
) -> Result<Option<String>> {
    let mut shown = text.to_string();
    let mut file_text = text.to_string();
    loop {
        fs::write(copy, &file_text).with_context(|| format!("cannot write {}", copy.display()))?;
        editor.edit(editor_command, copy)?;
        let saved =
            fs::read_to_string(copy).with_context(|| format!("cannot read {}", copy.display()))?;
        let content = without_error_comment(&saved);
        // Like an empty `git commit` message: nothing to save means give up.
        if content.trim().is_empty() || content == shown {
            return Ok(None);
        }
        match validate(kind, name, &content, agents) {
            Ok(_) => return Ok(Some(content)),
            Err(error) => {
                file_text = with_error_comment(&format!("{error:#}"), &content);
                shown = content;
            }
        }
    }
}

/// `text` with `error` as comment lines on top, and how to get out.
fn with_error_comment(error: &str, text: &str) -> String {
    let mut commented = String::new();
    for line in error.lines() {
        commented.push_str(&format!("{ERROR_PREFIX}{line}\n"));
    }
    commented.push_str(&format!(
        "{ERROR_PREFIX}fix the file and save it, or close it without changes to keep the last valid version\n"
    ));
    commented + text
}

/// Removes the error lines casulo added on top.
fn without_error_comment(text: &str) -> String {
    let mut rest = text;
    while rest.starts_with(ERROR_PREFIX.trim_end()) {
        rest = rest.split_once('\n').map_or("", |(_, after)| after);
    }
    rest.to_string()
}

/// Every resource of `kind`, sorted by name. An invalid one is listed with
/// the reason, so `list` still shows it.
pub fn list(home: &CasuloHome, kind: ResourceKind) -> Result<Vec<ResourceSummary>> {
    let agents = if kind == ResourceKind::Workflow {
        workflows::agent_names(home)?
    } else {
        Vec::new()
    };
    let mut summaries = Vec::new();
    for name in resource_names(home, kind)? {
        let path = home.resource_file(kind, &name);
        let description = match fs::read_to_string(&path) {
            Ok(text) => match validate(kind, &name, &text, &agents) {
                Ok(description) => description,
                Err(error) => {
                    format!("(invalid: {error}; fix it with `casulo edit {kind} {name}`)")
                }
            },
            Err(error) => format!("(cannot read {}: {error})", path.display()),
        };
        summaries.push(ResourceSummary {
            kind,
            name: name.to_string(),
            description: description.replace(['\n', '\t'], " "),
        });
    }
    Ok(summaries)
}

/// Names of the resources of `kind` on disk. Entries that are not valid
/// names are skipped, since no command can refer to them.
pub(super) fn resource_names(home: &CasuloHome, kind: ResourceKind) -> Result<Vec<ResourceName>> {
    let directory = match kind {
        ResourceKind::Agent => home.agents_dir(),
        ResourceKind::Skill => home.skills_dir(),
        ResourceKind::Mcp => home.mcp_dir(),
        ResourceKind::Workflow => home.workflows_dir(),
    };
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", directory.display()));
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("cannot read {}", directory.display()))?
            .path();
        let candidate = match kind {
            ResourceKind::Skill if path.join("SKILL.md").is_file() => path.file_name(),
            ResourceKind::Agent | ResourceKind::Mcp | ResourceKind::Workflow
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|found| found == kind.extension()) =>
            {
                path.file_stem()
            }
            _ => None,
        };
        if let Some(name) = candidate
            .and_then(|name| name.to_str())
            .and_then(|name| ResourceName::parse(name).ok())
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Deletes a resource: its file, or its folder for a skill. A workflow
/// takes the skill generated from it along.
pub fn delete(home: &CasuloHome, kind: ResourceKind, name: &ResourceName) -> Result<PathBuf> {
    let file = home.resource_file(kind, name);
    if !file.is_file() {
        bail!("{kind} \"{name}\" does not exist; `casulo list {kind}` shows the ones you have");
    }
    match kind {
        ResourceKind::Skill => {
            let folder = home.skills_dir().join(name.as_str());
            fs::remove_dir_all(&folder)
                .with_context(|| format!("cannot delete {}", folder.display()))?;
            Ok(folder)
        }
        ResourceKind::Agent | ResourceKind::Mcp => {
            fs::remove_file(&file).with_context(|| format!("cannot delete {}", file.display()))?;
            Ok(file)
        }
        ResourceKind::Workflow => {
            workflows::delete_skill(home, name)?;
            fs::remove_file(&file).with_context(|| format!("cannot delete {}", file.display()))?;
            Ok(file)
        }
    }
}

/// One line about an MCP server, for `casulo list`.
fn describe_mcp(server: &McpServer) -> String {
    match &server.transport {
        McpTransport::Command { command, .. } => command.join(" "),
        McpTransport::Url { url, .. } => url.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;

    use super::*;
    use crate::application::shared::domain::EditorSource;
    use crate::infrastructure::agent_files;

    /// Makes sure `agent_files` and this module agree on which files count.
    fn loaded_by_run(home: &CasuloHome) -> (usize, usize) {
        (
            agent_files::load_subagents(&home.agents_dir())
                .map(|agents| agents.len())
                .unwrap_or(0),
            agent_files::load_mcp_servers(&home.mcp_dir())
                .map(|servers| servers.len())
                .unwrap_or(0),
        )
    }

    /// An editor that replaces the file with each text in turn, recording
    /// what it was shown. `None` closes the editor without changes.
    struct ScriptedEditor {
        saves: RefCell<Vec<Option<String>>>,
        shown: RefCell<Vec<String>>,
    }

    impl ScriptedEditor {
        fn new(saves: &[Option<&str>]) -> Self {
            Self {
                saves: RefCell::new(
                    saves
                        .iter()
                        .rev()
                        .map(|save| save.map(str::to_string))
                        .collect(),
                ),
                shown: RefCell::new(Vec::new()),
            }
        }
    }

    impl Editor for ScriptedEditor {
        fn edit(&self, _editor: &EditorCommand, file: &Path) -> Result<()> {
            self.shown.borrow_mut().push(fs::read_to_string(file)?);
            let save = self
                .saves
                .borrow_mut()
                .pop()
                .expect("the test scripts every save");
            if let Some(text) = save {
                fs::write(file, text)?;
            }
            Ok(())
        }
    }

    fn home_in(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap()
    }

    fn vi() -> EditorCommand {
        EditorCommand::parse("vi", EditorSource::Default).unwrap()
    }

    fn name(value: &str) -> ResourceName {
        ResourceName::parse(value).unwrap()
    }

    const VALID_AGENT: &str =
        "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems.\n";

    #[test]
    fn templates_parse_or_are_rejected_as_unchanged() {
        let reviewer = name("reviewer");

        assert_eq!(
            validate(
                ResourceKind::Agent,
                &reviewer,
                &template(ResourceKind::Agent, &reviewer),
                &[]
            )
            .unwrap(),
            "Describe when to use reviewer"
        );
        assert!(
            validate(
                ResourceKind::Skill,
                &reviewer,
                &template(ResourceKind::Skill, &reviewer),
                &[]
            )
            .is_ok()
        );
        // Both MCP forms are commented out, so the template alone is not a server.
        assert!(
            validate(
                ResourceKind::Mcp,
                &reviewer,
                &template(ResourceKind::Mcp, &reviewer),
                &[]
            )
            .is_err()
        );
        // Saved as they are, every template counts as unchanged.
        for kind in ResourceKind::ALL {
            let directory = tempfile::tempdir().unwrap();
            let home = home_in(&directory);
            let editor = ScriptedEditor::new(&[None]);

            let outcome = add(&home, kind, &reviewer, &vi(), &editor).unwrap();

            assert_eq!(outcome, EditOutcome::Unchanged);
            assert!(!home.resource_file(kind, &reviewer).exists());
        }
    }

    #[test]
    fn mcp_template_works_once_a_form_is_uncommented() {
        let context7 = name("context7");
        let text = template(ResourceKind::Mcp, &context7)
            .replace("# command = ", "command = ")
            .replace("# env = ", "env = ");

        assert_eq!(
            validate(ResourceKind::Mcp, &context7, &text, &[]).unwrap(),
            "npx -y @upstash/context7-mcp"
        );
    }

    #[test]
    fn add_saves_a_valid_file() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = ScriptedEditor::new(&[Some(VALID_AGENT)]);

        let outcome = add(
            &home,
            ResourceKind::Agent,
            &name("reviewer"),
            &vi(),
            &editor,
        )
        .unwrap();

        let path = home.resource_file(ResourceKind::Agent, &name("reviewer"));
        assert_eq!(outcome, EditOutcome::Saved(path.clone()));
        assert_eq!(fs::read_to_string(path).unwrap(), VALID_AGENT);
        assert!(fs::read_dir(home.edits_dir()).unwrap().next().is_none());
        assert_eq!(loaded_by_run(&home), (1, 0));
    }

    #[test]
    fn invalid_save_is_reopened_with_the_error_until_valid() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let broken = "+++\nname = \"reviewer\"\n+++\nNo description.\n";
        let editor = ScriptedEditor::new(&[Some(broken), Some(VALID_AGENT)]);

        let outcome = add(
            &home,
            ResourceKind::Agent,
            &name("reviewer"),
            &vi(),
            &editor,
        )
        .unwrap();

        assert!(matches!(outcome, EditOutcome::Saved(_)));
        let shown = editor.shown.borrow();
        assert!(shown[1].starts_with("# casulo: "));
        assert!(shown[1].contains("description"));
        assert!(shown[1].ends_with(broken));
    }

    #[test]
    fn closing_an_invalid_file_unchanged_keeps_the_last_valid_version() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let reviewer = name("reviewer");
        let path = home.resource_file(ResourceKind::Agent, &reviewer);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, VALID_AGENT).unwrap();
        let editor = ScriptedEditor::new(&[Some("+++\nbroken\n"), None]);

        let outcome = edit(&home, ResourceKind::Agent, &reviewer, &vi(), &editor).unwrap();

        assert_eq!(outcome, EditOutcome::Unchanged);
        assert_eq!(fs::read_to_string(&path).unwrap(), VALID_AGENT);
    }

    #[test]
    fn emptied_file_is_not_created() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = ScriptedEditor::new(&[Some("  \n")]);

        let outcome = add(&home, ResourceKind::Mcp, &name("docs"), &vi(), &editor).unwrap();

        assert_eq!(outcome, EditOutcome::Unchanged);
    }

    #[test]
    fn add_fails_when_it_exists_and_edit_when_it_does_not() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let reviewer = name("reviewer");
        let editor = ScriptedEditor::new(&[]);

        let missing = edit(&home, ResourceKind::Agent, &reviewer, &vi(), &editor).unwrap_err();
        assert!(missing.to_string().contains("casulo add agent reviewer"));

        let path = home.resource_file(ResourceKind::Agent, &reviewer);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, VALID_AGENT).unwrap();
        let existing = add(&home, ResourceKind::Agent, &reviewer, &vi(), &editor).unwrap_err();
        assert!(existing.to_string().contains("casulo edit agent reviewer"));
    }

    #[test]
    fn list_shows_descriptions_and_invalid_files() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        fs::create_dir_all(home.agents_dir()).unwrap();
        fs::write(home.agents_dir().join("reviewer.md"), VALID_AGENT).unwrap();
        fs::write(home.agents_dir().join("broken.md"), "oops").unwrap();
        fs::write(home.agents_dir().join("notes.txt"), "ignored").unwrap();
        let skill = home.resource_file(ResourceKind::Skill, &name("notes"));
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\nname: notes\ndescription: Take notes\n---\n").unwrap();

        let agents = list(&home, ResourceKind::Agent).unwrap();
        let skills = list(&home, ResourceKind::Skill).unwrap();

        assert_eq!(agents[0].name, "broken");
        assert!(agents[0].description.starts_with("(invalid: "));
        assert_eq!(agents[1].name, "reviewer");
        assert_eq!(agents[1].description, "Reviews diffs");
        assert_eq!(skills[0].description, "Take notes");
        assert!(list(&home, ResourceKind::Mcp).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_the_file_or_the_skill_folder() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let notes = name("notes");
        let skill = home.resource_file(ResourceKind::Skill, &notes);
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "x").unwrap();
        fs::write(skill.parent().unwrap().join("script.sh"), "x").unwrap();

        let removed = delete(&home, ResourceKind::Skill, &notes).unwrap();

        assert_eq!(removed, home.skills_dir().join("notes"));
        assert!(!removed.exists());
        assert!(delete(&home, ResourceKind::Skill, &notes).is_err());
    }

    const VALID_WORKFLOW: &str =
        "description = \"Review the branch\"\n\n[[step]]\nagent = \"reviewer\"\napprove = true\n";

    /// A home with subagent `reviewer` in it, which workflows may use.
    fn home_with_reviewer(directory: &tempfile::TempDir) -> CasuloHome {
        let home = home_in(directory);
        fs::create_dir_all(home.agents_dir()).unwrap();
        fs::write(home.agents_dir().join("reviewer.md"), VALID_AGENT).unwrap();
        home
    }

    fn workflow_skill(home: &CasuloHome, workflow: &str) -> PathBuf {
        home.resource_file(ResourceKind::Skill, &name(&format!("workflow-{workflow}")))
    }

    #[test]
    fn add_workflow_saves_it_and_writes_its_skill() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_reviewer(&directory);
        let editor = ScriptedEditor::new(&[Some(VALID_WORKFLOW)]);

        let outcome = add(
            &home,
            ResourceKind::Workflow,
            &name("review"),
            &vi(),
            &editor,
        )
        .unwrap();

        let path = home.resource_file(ResourceKind::Workflow, &name("review"));
        assert_eq!(outcome, EditOutcome::Saved(path.clone()));
        assert_eq!(fs::read_to_string(path).unwrap(), VALID_WORKFLOW);
        let expected = Workflow::parse("review", VALID_WORKFLOW, &[name("reviewer")])
            .unwrap()
            .to_skill();
        assert_eq!(
            fs::read_to_string(workflow_skill(&home, "review")).unwrap(),
            expected
        );
        let listed = list(&home, ResourceKind::Workflow).unwrap();
        assert_eq!(listed[0].description, "Review the branch");
    }

    #[test]
    fn workflow_with_a_missing_subagent_is_reopened_with_the_error() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_reviewer(&directory);
        let ghost = "description = \"d\"\n[[step]]\nagent = \"ghost\"\n";
        let editor = ScriptedEditor::new(&[Some(ghost), None]);

        let outcome = add(
            &home,
            ResourceKind::Workflow,
            &name("review"),
            &vi(),
            &editor,
        )
        .unwrap();

        assert_eq!(outcome, EditOutcome::Unchanged);
        assert!(editor.shown.borrow()[1].contains("casulo add agent ghost"));
        assert!(!workflow_skill(&home, "review").exists());
    }

    #[test]
    fn a_skill_the_user_wrote_is_never_replaced_by_a_workflow() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_reviewer(&directory);
        let skill = workflow_skill(&home, "review");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        let mine = "---\nname: workflow-review\ndescription: Mine\n---\nMy own.\n";
        fs::write(&skill, mine).unwrap();
        // No saves scripted: the editor must not even open.
        let editor = ScriptedEditor::new(&[]);

        let error = add(
            &home,
            ResourceKind::Workflow,
            &name("review"),
            &vi(),
            &editor,
        )
        .unwrap_err();

        assert!(error.to_string().contains("casulo did not generate it"));
        assert_eq!(fs::read_to_string(&skill).unwrap(), mine);
    }

    #[test]
    fn editing_a_workflow_without_changes_brings_its_skill_up_to_date() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_reviewer(&directory);
        let review = name("review");
        let editor = ScriptedEditor::new(&[Some(VALID_WORKFLOW)]);
        add(&home, ResourceKind::Workflow, &review, &vi(), &editor).unwrap();
        let skill = workflow_skill(&home, "review");
        let current = fs::read_to_string(&skill).unwrap();
        fs::write(&skill, current.replace("approval", "nothing")).unwrap();

        let outcome = edit(
            &home,
            ResourceKind::Workflow,
            &review,
            &vi(),
            &ScriptedEditor::new(&[None]),
        )
        .unwrap();

        assert_eq!(outcome, EditOutcome::Unchanged);
        assert_eq!(fs::read_to_string(&skill).unwrap(), current);
    }

    #[test]
    fn delete_workflow_removes_its_skill_too() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_reviewer(&directory);
        let review = name("review");
        let editor = ScriptedEditor::new(&[Some(VALID_WORKFLOW)]);
        add(&home, ResourceKind::Workflow, &review, &vi(), &editor).unwrap();

        let removed = delete(&home, ResourceKind::Workflow, &review).unwrap();

        assert!(!removed.exists());
        assert!(!home.skills_dir().join("workflow-review").exists());
    }

    #[test]
    fn list_shows_a_workflow_whose_subagent_is_gone_as_invalid() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        fs::create_dir_all(home.workflows_dir()).unwrap();
        fs::write(home.workflows_dir().join("review.toml"), VALID_WORKFLOW).unwrap();

        let listed = list(&home, ResourceKind::Workflow).unwrap();

        assert!(listed[0].description.contains("casulo add agent reviewer"));
    }

    #[test]
    fn error_comment_is_removed_before_checking() {
        let text = with_error_comment("line one\nline two", "+++\nbody\n");

        assert_eq!(without_error_comment(&text), "+++\nbody\n");
    }

    const VALID_SKILL: &str =
        "---\nname: tdd\ndescription: Test first\n---\nWrite the test first.\n";

    fn skill_file(value: &str) -> SkillFile {
        SkillFile::parse(value).unwrap()
    }

    /// A home with skill `tdd` in it.
    fn home_with_skill(directory: &tempfile::TempDir) -> CasuloHome {
        let home = home_in(directory);
        let skill = home.resource_file(ResourceKind::Skill, &name("tdd"));
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, VALID_SKILL).unwrap();
        home
    }

    #[test]
    fn add_skill_file_saves_it_inside_the_skill_folder() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_skill(&directory);
        let editor = ScriptedEditor::new(&[Some("# Template\n")]);

        let outcome = add_skill_file(
            &home,
            &name("tdd"),
            &skill_file("assets/template.md"),
            &vi(),
            &editor,
        )
        .unwrap();

        let path = home
            .skills_dir()
            .join("tdd")
            .join("assets")
            .join("template.md");
        assert_eq!(outcome, EditOutcome::Saved(path.clone()));
        assert_eq!(fs::read_to_string(path).unwrap(), "# Template\n");
    }

    #[test]
    fn add_skill_file_saves_nothing_when_left_empty() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_skill(&directory);
        let editor = ScriptedEditor::new(&[None]);

        let outcome = add_skill_file(
            &home,
            &name("tdd"),
            &skill_file("assets/template.md"),
            &vi(),
            &editor,
        )
        .unwrap();

        assert_eq!(outcome, EditOutcome::Unchanged);
        assert!(!home.skills_dir().join("tdd").join("assets").exists());
    }

    #[test]
    fn add_skill_file_fails_when_the_skill_does_not_exist() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        let editor = ScriptedEditor::new(&[]);

        let error = add_skill_file(
            &home,
            &name("tdd"),
            &skill_file("assets/template.md"),
            &vi(),
            &editor,
        )
        .unwrap_err();

        assert!(error.to_string().contains("casulo add skill tdd"));
    }

    #[test]
    fn add_skill_file_fails_when_the_file_exists() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_skill(&directory);
        fs::write(home.skills_dir().join("tdd").join("notes.md"), "x").unwrap();
        let editor = ScriptedEditor::new(&[]);

        let error = add_skill_file(&home, &name("tdd"), &skill_file("notes.md"), &vi(), &editor)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("casulo edit skill tdd --file notes.md")
        );
    }

    #[test]
    fn edit_skill_file_shows_the_current_text_and_saves_the_change() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_skill(&directory);
        let path = home.skills_dir().join("tdd").join("notes.md");
        fs::write(&path, "old\n").unwrap();
        let editor = ScriptedEditor::new(&[Some("new\n")]);

        let outcome =
            edit_skill_file(&home, &name("tdd"), &skill_file("notes.md"), &vi(), &editor).unwrap();

        assert_eq!(outcome, EditOutcome::Saved(path.clone()));
        assert_eq!(editor.shown.borrow().as_slice(), ["old\n"]);
        assert_eq!(fs::read_to_string(path).unwrap(), "new\n");
    }

    #[test]
    fn edit_skill_file_fails_when_the_file_does_not_exist() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_skill(&directory);
        let editor = ScriptedEditor::new(&[]);

        let error = edit_skill_file(&home, &name("tdd"), &skill_file("notes.md"), &vi(), &editor)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("casulo add skill tdd --file notes.md")
        );
    }
}
