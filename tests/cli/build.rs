//! Functional tests of `casulo build`.

use predicates::prelude::*;

use crate::support::{casulo, home_with_missing_runtime};

#[test]
fn build_needs_a_project() {
    let home = tempfile::tempdir().expect("create a temporary directory");

    casulo()
        .args(["build", "--project", "missing"])
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "project \"missing\" does not exist",
        ));

    casulo()
        .arg("build")
        .env("CASULO_HOME", home.path())
        .current_dir(home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not inside a casulo project"));
}

#[test]
fn build_writes_the_dockerfile_outside_the_workspace() {
    let home = home_with_missing_runtime();
    let project_dir = home.path().join("projects").join("demo");
    let workspace = project_dir.join("workspace");
    std::fs::create_dir_all(&workspace).expect("create the workspace");
    std::fs::write(
        project_dir.join("casulo.toml"),
        "repository = \"https://example.com/demo.git\"\nharness = \"codex\"\n",
    )
    .expect("write the project config");

    // The runtime does not exist, so the build fails after writing the Dockerfile.
    casulo()
        .arg("build")
        .env("CASULO_HOME", home.path())
        .current_dir(&workspace)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot run casulo-test-missing-runtime",
        ));

    let dockerfile =
        std::fs::read_to_string(project_dir.join("build").join("codex").join("Dockerfile"))
            .expect("the Dockerfile was written");
    assert!(dockerfile.contains("RUN npm install -g @openai/codex"));
    assert!(!workspace.join("Dockerfile").exists());
}
