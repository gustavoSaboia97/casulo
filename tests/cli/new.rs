//! Functional tests of `casulo new`.

use predicates::prelude::*;

use crate::support::{casulo, new_from_directory};

/// Creates a git repository with one commit in `directory`, using only git.
fn make_source_repository(directory: &std::path::Path) {
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=casulo",
                "-c",
                "user.email=casulo@example.com",
            ])
            .args(args)
            .current_dir(directory)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    std::fs::create_dir_all(directory).expect("create the source directory");
    run(&["init", "--quiet"]);
    std::fs::write(directory.join("README.md"), "hello\n").expect("write a file");
    run(&["add", "README.md"]);
    run(&["commit", "--quiet", "-m", "first"]);
}

/// A `file://` URL for a local repository, so `casulo new` clones it
/// instead of using the directory in place.
fn file_url(path: &std::path::Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

#[test]
fn new_clones_a_local_repository() {
    let source_parent = tempfile::tempdir().expect("create a temporary directory");
    let source = source_parent.path().join("Source_Repo");
    make_source_repository(&source);
    let home = tempfile::tempdir().expect("create a temporary directory");
    let project_dir = home.path().join("projects").join("source-repo");

    casulo()
        .arg("new")
        .arg(file_url(&source))
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(format!("{}\n", project_dir.join("workspace").display()))
        .stderr(predicate::str::contains("created project \"source-repo\""));

    let readme = std::fs::read_to_string(project_dir.join("workspace").join("README.md"))
        .expect("the clone has the committed file");
    assert_eq!(readme.trim_end(), "hello");
    let config = std::fs::read_to_string(project_dir.join("casulo.toml")).expect("config exists");
    assert!(config.contains("repository = "));
}

#[test]
fn new_uses_the_given_name_and_refuses_to_overwrite() {
    let source_parent = tempfile::tempdir().expect("create a temporary directory");
    let source = source_parent.path().join("source");
    make_source_repository(&source);
    let home = tempfile::tempdir().expect("create a temporary directory");

    casulo()
        .args(["new", "--name", "demo"])
        .arg(file_url(&source))
        .env("CASULO_HOME", home.path())
        .assert()
        .success();
    assert!(
        home.path()
            .join("projects")
            .join("demo")
            .join("workspace")
            .join(".git")
            .exists()
    );

    casulo()
        .args(["new", "--name", "demo"])
        .arg(file_url(&source))
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("project \"demo\" already exists"));
}

#[test]
fn new_rejects_invalid_name_and_credentials() {
    casulo()
        .args(["new", "--name", "Bad Name", "https://example.com/repo.git"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid project name"));

    casulo()
        .args(["new", "https://user:secret-token@example.com/repo.git"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("contains credentials"))
        .stderr(predicate::str::contains("secret-token").not());
}

#[test]
fn new_cleans_up_when_clone_fails() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let missing = home.path().join("does-not-exist");

    casulo()
        .arg("new")
        .arg(file_url(&missing))
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("git clone failed"));
    assert!(!home.path().join("projects").join("does-not-exist").exists());
}

#[test]
fn new_uses_a_directory_with_a_repository_in_place() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("my-app");
    make_source_repository(&directory);
    let home = tempfile::tempdir().expect("create a temporary directory");

    let (workspace, stderr) = new_from_directory(home.path(), &directory);

    assert!(stderr.contains("created project \"my-app\""));
    assert!(!stderr.contains("git init"));
    assert!(stderr.contains("warning: no remote"));
    assert!(std::path::Path::new(&workspace).join("README.md").is_file());
    let project_dir = home.path().join("projects").join("my-app");
    assert!(!project_dir.join("workspace").exists());
    let config = std::fs::read_to_string(project_dir.join("casulo.toml")).expect("config exists");
    assert!(config.contains("workspace = "));
    assert!(config.contains(".git/hooks"));
    assert!(!config.contains("\nrepository = "));
}

#[test]
fn new_initializes_a_directory_without_a_repository() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("plain");
    std::fs::create_dir_all(&directory).expect("create the directory");
    std::fs::write(directory.join("notes.txt"), "mine\n").expect("write a file");
    let home = tempfile::tempdir().expect("create a temporary directory");

    let (_, stderr) = new_from_directory(home.path(), &directory);

    assert!(stderr.contains("had no git repository, so casulo ran git init there"));
    assert!(directory.join(".git").is_dir());
    assert!(directory.join("notes.txt").is_file());
}

#[test]
fn new_creates_a_directory_that_does_not_exist() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("brand-new");
    let home = tempfile::tempdir().expect("create a temporary directory");

    new_from_directory(home.path(), &directory);

    assert!(directory.join(".git").is_dir());
}

#[test]
fn new_rejects_a_bare_word() {
    casulo()
        .args(["new", "app"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use ./app"));
}
