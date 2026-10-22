//! What the functional tests of several commands share: running the real
//! binary, and the casulo homes and projects they start from.

use assert_cmd::Command;

/// Builds a command for the casulo binary with a clean casulo environment.
pub fn casulo() -> Command {
    let mut command = Command::cargo_bin("casulo").expect("cargo builds the casulo binary");
    // Keep the developer's own setting from leaking into the tests.
    command.env_remove("CASULO_HOME");
    command
}

/// A casulo home whose global config points at a runtime that does not exist,
/// so the doctor finds a problem without needing Docker.
pub fn home_with_missing_runtime() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let config_dir = home.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("create the config directory");
    std::fs::write(
        config_dir.join("casulo.toml"),
        "runtime = \"casulo-test-missing-runtime\"\n",
    )
    .expect("write the config");
    home
}

/// Runs `casulo new <directory>` and returns the workspace it printed.
pub fn new_from_directory(home: &std::path::Path, directory: &std::path::Path) -> (String, String) {
    let output = casulo()
        .arg("new")
        .arg(directory)
        .env("CASULO_HOME", home)
        .output()
        .expect("run casulo new");
    assert!(output.status.success(), "{output:?}");
    (
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// An editor command that replaces the edited file with `source`, like a
/// person typing it in and saving.
pub fn editor_writing(source: &std::path::Path) -> String {
    if cfg!(windows) {
        format!("cmd /C copy /Y {}", source.display())
    } else {
        format!("cp {}", source.display())
    }
}
