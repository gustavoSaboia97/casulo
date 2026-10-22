//! The skill casulo generates from each workflow: writing it when the
//! workflow is saved, deleting it with the workflow, and checking it is
//! still in step before a run.

use std::fs;
use std::io::ErrorKind;

use anyhow::{Context, Result, bail};

use super::resources::resource_names;
use crate::application::agent::domain::Workflow;
use crate::application::shared::domain::{CasuloHome, ResourceKind, ResourceName};

/// Names of the subagents under `agents/`, which a workflow's steps may use.
pub fn agent_names(home: &CasuloHome) -> Result<Vec<ResourceName>> {
    resource_names(home, ResourceKind::Agent)
}

/// Fails when skill `workflow-<name>` exists and casulo did not generate
/// it, so a skill the user wrote is never overwritten.
pub fn ensure_skill_is_replaceable(home: &CasuloHome, name: &ResourceName) -> Result<()> {
    let skill = Workflow::skill_name(name)?;
    let path = home.resource_file(ResourceKind::Skill, &skill);
    match fs::read_to_string(&path) {
        Ok(text) if !Workflow::is_generated_skill(&text) => bail!(
            "skill \"{skill}\" exists and casulo did not generate it, so workflow \"{name}\" cannot use that name; choose another workflow name, or remove the skill with `casulo delete skill {skill}`"
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Writes the skill of `workflow`, replacing the one generated before.
pub fn write_skill(home: &CasuloHome, workflow: &Workflow) -> Result<()> {
    ensure_skill_is_replaceable(home, &workflow.name)?;
    let skill = Workflow::skill_name(&workflow.name)?;
    let path = home.resource_file(ResourceKind::Skill, &skill);
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    fs::write(&path, workflow.to_skill())
        .with_context(|| format!("cannot write {}", path.display()))
}

/// Writes the skill of the workflow saved as `name`, when it is valid, so
/// opening it and closing the editor brings a stale skill up to date.
pub fn refresh_skill(home: &CasuloHome, name: &ResourceName) -> Result<()> {
    let path = home.resource_file(ResourceKind::Workflow, name);
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if let Ok(workflow) = Workflow::parse(name.as_str(), &text, &agent_names(home)?) {
        write_skill(home, &workflow)?;
    }
    Ok(())
}

/// Deletes the skill generated from workflow `name`, if casulo generated
/// it. A skill the user wrote under that name is left alone.
pub fn delete_skill(home: &CasuloHome, name: &ResourceName) -> Result<()> {
    let skill = Workflow::skill_name(name)?;
    let path = home.resource_file(ResourceKind::Skill, &skill);
    let generated = fs::read_to_string(&path).is_ok_and(|text| Workflow::is_generated_skill(&text));
    if generated {
        let folder = home.skills_dir().join(skill.as_str());
        fs::remove_dir_all(&folder)
            .with_context(|| format!("cannot delete {}", folder.display()))?;
    }
    Ok(())
}

/// One warning per workflow that would not run as written: invalid (such
/// as a subagent that no longer exists), or with a skill that is missing or
/// out of date. A run goes on regardless, so problems are only reported.
pub fn check_skills(home: &CasuloHome, agents: &[ResourceName]) -> Vec<String> {
    let names = match resource_names(home, ResourceKind::Workflow) {
        Ok(names) => names,
        Err(error) => return vec![format!("{error:#}")],
    };
    let mut warnings = Vec::new();
    for name in names {
        let path = home.resource_file(ResourceKind::Workflow, &name);
        let workflow = match fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))
            .and_then(|text| Workflow::parse(name.as_str(), &text, agents))
        {
            Ok(workflow) => workflow,
            Err(error) => {
                warnings.push(format!(
                    "workflow \"{name}\" is invalid: {error:#}; fix it with `casulo edit workflow {name}`"
                ));
                continue;
            }
        };
        let Ok(skill) = Workflow::skill_name(&name) else {
            continue;
        };
        let skill_text = fs::read_to_string(home.resource_file(ResourceKind::Skill, &skill)).ok();
        let problem = match skill_text {
            None => "is missing",
            Some(text) if text != workflow.to_skill() => "is out of date",
            Some(_) => continue,
        };
        warnings.push(format!(
            "the skill {skill} of workflow \"{name}\" {problem}; open the workflow with `casulo edit workflow {name}` and close it to write it again"
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    const WORKFLOW: &str = "description = \"Review\"\n[[step]]\nagent = \"reviewer\"\n";

    fn name(value: &str) -> ResourceName {
        ResourceName::parse(value).unwrap()
    }

    /// A home with workflow `review` saved, and no skill for it yet.
    fn home_with_workflow(directory: &tempfile::TempDir) -> CasuloHome {
        let home = CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap();
        fs::create_dir_all(home.workflows_dir()).unwrap();
        fs::write(home.workflows_dir().join("review.toml"), WORKFLOW).unwrap();
        home
    }

    #[test]
    fn check_warns_about_a_missing_skill_and_is_quiet_once_written() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_workflow(&directory);
        let agents = [name("reviewer")];

        let missing = check_skills(&home, &agents);
        let workflow = Workflow::parse("review", WORKFLOW, &agents).unwrap();
        write_skill(&home, &workflow).unwrap();
        let written = check_skills(&home, &agents);

        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("workflow-review of workflow \"review\" is missing"));
        assert!(written.is_empty(), "{written:?}");
    }

    #[test]
    fn delete_skill_leaves_a_skill_the_user_wrote() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_with_workflow(&directory);
        let skill = home.resource_file(ResourceKind::Skill, &name("workflow-review"));
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(
            &skill,
            "---\nname: workflow-review\ndescription: Mine\n---\n",
        )
        .unwrap();

        delete_skill(&home, &name("review")).unwrap();

        assert!(skill.is_file());
    }
}
