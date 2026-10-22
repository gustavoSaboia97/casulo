//! Functional tests of `casulo doctor`.

use predicates::prelude::*;

use crate::support::{casulo, home_with_missing_runtime};

#[test]
fn doctor_reports_missing_runtime_with_a_fix() {
    let home = home_with_missing_runtime();

    casulo()
        .arg("doctor")
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("[ok]      git: git version"))
        .stdout(predicate::str::contains(
            "[problem] casulo-test-missing-runtime command-line tool",
        ))
        .stdout(predicate::str::contains("docs: https://"))
        .stdout(predicate::str::contains("[skipped]"))
        .stderr(predicate::str::contains("run `casulo setup`"));
}

#[test]
fn doctor_rejects_invalid_global_config() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let config_dir = home.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("create the config directory");
    std::fs::write(config_dir.join("casulo.toml"), "runtime = ").expect("write the config");

    casulo()
        .arg("doctor")
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid configuration in"));
}
