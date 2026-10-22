//! Functional tests of `casulo config`.

use predicates::prelude::*;

use crate::support::{casulo, editor_writing};

#[test]
fn config_path_prints_the_global_file_without_creating_it() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let expected = home.path().join("config").join("casulo.toml");

    casulo()
        .args(["config", "--path"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(format!("{}\n", expected.display()));
    assert!(!expected.exists());
}

#[test]
fn config_path_prints_a_project_file() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let expected = home.path().join("projects").join("app").join("casulo.toml");

    casulo()
        .args(["config", "--path", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(format!("{}\n", expected.display()));
}

#[test]
fn config_names_the_editor_that_cannot_start() {
    let home = tempfile::tempdir().expect("create a temporary directory");

    casulo()
        .arg("config")
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", "casulo-test-missing-editor --wait")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot start the editor \"casulo-test-missing-editor\" (from CASULO_EDITOR)",
        ))
        .stderr(predicate::str::contains("change or unset CASULO_EDITOR"));
    // The file is created from the template before the editor starts.
    assert!(home.path().join("config").join("casulo.toml").is_file());
}

#[test]
fn config_opens_the_configured_editor_even_when_the_file_is_invalid() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let fixed = home.path().join("fixed.toml");
    std::fs::write(&fixed, "runtime = \"docker\"\n").expect("write the editor input");
    let config_file = home.path().join("config").join("casulo.toml");
    std::fs::create_dir_all(home.path().join("config")).expect("create the config directory");
    let editor = toml::Value::String(editor_writing(&fixed));
    std::fs::write(
        &config_file,
        format!("editor = {editor}\nruntme = \"podman\"\n"),
    )
    .expect("write an invalid config");

    casulo()
        .arg("config")
        .env("CASULO_HOME", home.path())
        .env_remove("CASULO_EDITOR")
        .env_remove("VISUAL")
        .env_remove("EDITOR")
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&config_file).expect("read the config"),
        "runtime = \"docker\"\n"
    );
}
