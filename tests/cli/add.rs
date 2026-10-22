//! Functional tests of `casulo add`, `edit`, `list` and `delete` on subagents, skills and workflows.

use predicates::prelude::*;

use crate::support::{casulo, editor_writing};

#[test]
fn add_list_and_delete_a_subagent() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let fixed = home.path().join("reviewer-input.md");
    std::fs::write(
        &fixed,
        "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems.\n",
    )
    .expect("write the editor input");
    let agent_file = home.path().join("agent").join("agents").join("reviewer.md");

    casulo()
        .args(["add", "agent", "reviewer"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&fixed))
        .assert()
        .success()
        .stderr(predicate::str::contains("saved"));
    assert!(agent_file.is_file());

    casulo()
        .args(["add", "agent", "reviewer"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&fixed))
        .assert()
        .failure()
        .stderr(predicate::str::contains("casulo edit agent reviewer"));

    casulo()
        .args(["list", "agent"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("reviewer\tReviews diffs\n");

    casulo()
        .arg("list")
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("agent\treviewer\tReviews diffs\n");

    // Without a terminal nobody says yes, so nothing is deleted.
    casulo()
        .args(["delete", "agent", "reviewer"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("nothing deleted"));
    assert!(agent_file.is_file());

    casulo()
        .args(["delete", "agent", "reviewer", "--yes"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success();
    assert!(!agent_file.exists());
}

#[test]
fn add_saved_unchanged_creates_nothing() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    // `true` exits without touching the file; on Windows `rem` does the same.
    let editor = if cfg!(windows) { "cmd /C rem" } else { "true" };

    casulo()
        .args(["add", "mcp", "docs"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor)
        .assert()
        .success()
        .stderr(predicate::str::contains("was not created"));
    assert!(
        !home
            .path()
            .join("agent")
            .join("mcp")
            .join("docs.toml")
            .exists()
    );
}

#[test]
fn resource_names_must_be_slugs() {
    casulo()
        .args(["add", "skill", "Code--Reviewer"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid name"));
}

#[test]
fn add_and_edit_a_file_inside_a_skill() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let skill_input = home.path().join("skill-input.md");
    std::fs::write(
        &skill_input,
        "---\nname: tdd\ndescription: Test first\n---\nWrite the test first.\n",
    )
    .expect("write the skill input");
    let template_input = home.path().join("template-input.md");
    std::fs::write(&template_input, "# Template\n").expect("write the template input");
    let template = home
        .path()
        .join("agent")
        .join("skills")
        .join("tdd")
        .join("assets")
        .join("template.md");

    // A file needs its skill first.
    casulo()
        .args(["add", "skill", "tdd", "--file", "assets/template.md"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&template_input))
        .assert()
        .failure()
        .stderr(predicate::str::contains("casulo add skill tdd"));

    casulo()
        .args(["add", "skill", "tdd"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&skill_input))
        .assert()
        .success();

    casulo()
        .args(["add", "skill", "tdd", "--file", "assets/template.md"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&template_input))
        .assert()
        .success()
        .stderr(predicate::str::contains("saved"));
    assert_eq!(
        std::fs::read_to_string(&template).expect("read the template"),
        "# Template\n"
    );

    std::fs::write(&template_input, "# Template v2\n").expect("write the new template input");
    casulo()
        .args(["edit", "skill", "tdd", "--file", "assets/template.md"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&template_input))
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&template).expect("read the template"),
        "# Template v2\n"
    );

    casulo()
        .args(["add", "agent", "reviewer", "--file", "notes.md"])
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--file only works with skills"));

    casulo()
        .args(["add", "skill", "tdd", "--file", "../escape.md"])
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid skill file"));
}

#[test]
fn add_list_and_delete_a_workflow_with_its_skill() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let agents = home.path().join("agent").join("agents");
    std::fs::create_dir_all(&agents).expect("create the agents directory");
    std::fs::write(
        agents.join("reviewer.md"),
        "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems.\n",
    )
    .expect("write the subagent");
    let fixed = home.path().join("workflow-input.toml");
    std::fs::write(
        &fixed,
        "description = \"Review the branch\"\n\n[[step]]\nagent = \"reviewer\"\n",
    )
    .expect("write the editor input");
    let workflow_file = home
        .path()
        .join("agent")
        .join("workflows")
        .join("review.toml");
    let skill_folder = home
        .path()
        .join("agent")
        .join("skills")
        .join("workflow-review");

    casulo()
        .args(["add", "workflow", "review"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&fixed))
        .assert()
        .success()
        .stderr(predicate::str::contains("saved"));
    assert!(workflow_file.is_file());
    let skill = std::fs::read_to_string(skill_folder.join("SKILL.md")).expect("read the skill");
    assert!(skill.starts_with("---\nname: workflow-review\n"));

    casulo()
        .args(["list", "workflow"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("review\tReview the branch\n");

    casulo()
        .args(["delete", "workflow", "review", "--yes"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success();
    assert!(!workflow_file.exists());
    assert!(!skill_folder.exists());
}
