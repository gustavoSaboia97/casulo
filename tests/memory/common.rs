//! A small Rust crate with two branches, built with the real git, shared by
//! the integration tests.
// Each test file uses only some of these helpers.
#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

/// The file that reads as empty: `NUL` on Windows, `/dev/null` elsewhere.
const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

pub fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        // The developer's global and system git config never apply, so
        // commit signing or global hooks cannot run in tests.
        .env("GIT_CONFIG_GLOBAL", NULL_DEVICE)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=casulo",
            "-c",
            "user.email=casulo@example.com",
        ])
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .status()
        .expect("run git");
    assert!(status.success(), "git {arguments:?} failed");
}

pub fn write(directory: &Path, path: &str, text: &str) {
    let file = path
        .split('/')
        .fold(directory.to_path_buf(), |file, part| file.join(part));
    fs::create_dir_all(file.parent().expect("files have a parent")).expect("create the folder");
    fs::write(file, text).expect("write the file");
}

/// A crate with `main` and `feature` branches; `main` is checked out.
pub fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("create a temporary directory");
    let root = directory.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(
        root,
        "src/main.rs",
        "mod cli;\nmod domain;\nuse crate::domain::config::Config;\nfn main() {}\n",
    );
    write(
        root,
        "src/cli.rs",
        "use crate::domain::config;\npub fn run() {}\n",
    );
    write(root, "src/domain/mod.rs", "pub mod config;\n");
    write(root, "src/domain/config.rs", "pub struct Config;\n");
    write(root, "README.md", "# fixture\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "first"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    write(
        root,
        "src/cli.rs",
        "use crate::domain::config;\npub fn run() {}\npub fn other() {}\n",
    );
    git(root, &["commit", "-q", "-am", "feature"]);
    git(root, &["checkout", "-q", "main"]);
    directory
}
