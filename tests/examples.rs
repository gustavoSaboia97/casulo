//! The examples in `examples/` are checked with the same code `casulo add`
//! uses, so a user who copies them into `~/.casulo/agent` gets a valid setup.

use std::fs;
use std::path::{Path, PathBuf};

use casulo::application::agent::services::resources::validate;
use casulo::application::shared::domain::{ResourceKind, ResourceName};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// The name and text of each file in `examples/<folder>` with `extension`.
fn files(folder: &str, extension: &str) -> Vec<(ResourceName, String)> {
    let directory = examples_dir().join(folder);
    let mut found = Vec::new();
    for entry in fs::read_dir(&directory).expect("read the examples folder") {
        let path = entry.expect("read an example").path();
        if path.extension().is_some_and(|found| found == extension) {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("a UTF-8 file name");
            let name = ResourceName::parse(stem).expect("a valid resource name");
            found.push((name, fs::read_to_string(&path).expect("read an example")));
        }
    }
    found.sort();
    found
}

/// The example skills: each folder of `examples/skills` and its `SKILL.md`.
fn skills() -> Vec<(ResourceName, String)> {
    let directory = examples_dir().join("skills");
    let mut found = Vec::new();
    for entry in fs::read_dir(&directory).expect("read the skills folder") {
        let path = entry.expect("read a skill").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a UTF-8 folder name");
        let name = ResourceName::parse(name).expect("a valid skill name");
        let text = fs::read_to_string(path.join("SKILL.md")).expect("read SKILL.md");
        found.push((name, text));
    }
    found.sort();
    found
}

/// The skills a subagent names, written as "the `<name>` skill".
fn named_skills(instructions: &str) -> Vec<String> {
    // Split on backticks, the code spans are the odd parts.
    let parts: Vec<&str> = instructions.split('`').collect();
    let mut named = Vec::new();
    for index in (1..parts.len()).step_by(2) {
        if parts
            .get(index + 1)
            .is_some_and(|after| after.starts_with(" skill"))
        {
            named.push(parts[index].to_string());
        }
    }
    named
}

#[test]
fn every_example_is_valid() {
    let agents = files("agents", "md");
    let agent_names: Vec<ResourceName> = agents.iter().map(|(name, _)| name.clone()).collect();
    let workflows = files("workflows", "toml");
    let skills = skills();
    assert_eq!(agents.len(), 6);
    assert_eq!(workflows.len(), 3);
    assert_eq!(skills.len(), 10);

    for (kind, examples) in [
        (ResourceKind::Agent, &agents),
        (ResourceKind::Skill, &skills),
        (ResourceKind::Workflow, &workflows),
    ] {
        for (name, text) in examples {
            // Workflows are checked against the example agents only.
            if let Err(error) = validate(kind, name, text, &agent_names) {
                panic!("example {kind} {name} is invalid: {error:#}");
            }
        }
    }
}

#[test]
fn every_skill_an_example_agent_names_exists() {
    let skill_names: Vec<String> = skills().iter().map(|(name, _)| name.to_string()).collect();

    for (agent, text) in files("agents", "md") {
        let named = named_skills(&text);
        assert!(!named.is_empty(), "agent {agent} names no skill");
        for skill in named {
            assert!(
                skill_names.contains(&skill),
                "agent {agent} names skill {skill}, which is not in examples/skills"
            );
        }
    }
}
