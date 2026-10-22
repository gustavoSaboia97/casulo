//! Functional tests of `casulo --version`, `--help` and unknown commands.

use predicates::prelude::*;

use crate::support::casulo;

#[test]
fn prints_version() {
    casulo()
        .arg("--version")
        .assert()
        .success()
        .stdout("casulo 0.1.0\n");
}

#[test]
fn prints_help_with_commands() {
    casulo()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: casulo"))
        .stdout(predicate::str::contains("home"));
}

#[test]
fn unknown_command_fails() {
    casulo()
        .arg("not-a-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}
