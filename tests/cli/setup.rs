//! Functional tests of `casulo setup`.

use predicates::prelude::*;

use crate::support::{casulo, home_with_missing_runtime};

#[test]
fn setup_runs_nothing_without_a_yes() {
    let home = home_with_missing_runtime();

    // Every answer is empty: no fix is chosen or confirmed, so nothing runs.
    casulo()
        .arg("setup")
        .env("CASULO_HOME", home.path())
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "problem: casulo-test-missing-runtime command-line tool",
        ))
        .stderr(predicate::str::contains("setup stopped"));
}

#[test]
fn setup_creates_the_casulo_home_once() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let root = home.path().join("casulo-home");
    let config = root.join("config").join("casulo.toml");

    // The host checks fail without Docker, but the home is created first.
    casulo()
        .arg("setup")
        .env("CASULO_HOME", &root)
        .write_stdin("")
        .assert()
        .stderr(predicate::str::contains(format!(
            "created {}",
            config.display()
        )));
    let first = std::fs::read_to_string(&config).expect("setup writes the config");
    assert!(first.contains("# default_harness = \"claude\""));
    assert!(root.join("agent").join("skills").is_dir());
    assert!(root.join("agent").join("AGENTS.md").is_file());

    casulo()
        .arg("setup")
        .env("CASULO_HOME", &root)
        .write_stdin("")
        .assert()
        .stderr(predicate::str::contains("created").not())
        .stderr(predicate::str::contains(format!(
            "kept {} (it already exists)",
            config.display()
        )));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), first);
}
