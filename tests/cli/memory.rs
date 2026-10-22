//! Functional tests of the memory commands: `describe`, `index`, `list`, `edit`, `promote` and `search memory`.

use predicates::prelude::*;

use crate::support::{casulo, editor_writing, new_from_directory};

#[test]
fn describe_memory_of_a_project_without_memory() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("app");
    std::fs::create_dir_all(&directory).expect("create the directory");
    let home = tempfile::tempdir().expect("create a temporary directory");
    new_from_directory(home.path(), &directory);

    casulo()
        .args(["describe", "memory", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("project        app\nmemory         not created yet\n");
}

#[test]
fn describe_memory_outside_a_project_says_how_to_choose_one() {
    let home = tempfile::tempdir().expect("create a temporary directory");

    casulo()
        .args(["describe", "memory"])
        .env("CASULO_HOME", home.path())
        .current_dir(home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --project <name>"));
}

#[test]
fn describe_memory_counts_what_the_database_holds() {
    use casulo::application::memory::domain::RepoPath;
    use casulo::application::memory::domain::event::Event;
    use casulo::application::memory::ports::MemoryStore;
    use casulo::infrastructure::sqlite::Database;

    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("app");
    std::fs::create_dir_all(&directory).expect("create the directory");
    let home = tempfile::tempdir().expect("create a temporary directory");
    new_from_directory(home.path(), &directory);
    let memory = home.path().join("projects").join("app").join("memory");
    std::fs::create_dir_all(&memory).expect("create the memory directory");
    let mut database = Database::open(&memory.join("memory.sqlite")).expect("open the database");
    database
        .apply(
            &Event::FileIndexed {
                path: RepoPath::parse("src/main.rs").expect("valid path"),
                blob_hash: "a1".to_string(),
                edges: Vec::new(),
                symbols: vec!["main".to_string()],
            },
            "2026-10-09T12:00:00Z",
        )
        .expect("apply the event");
    drop(database);

    casulo()
        .args(["describe", "memory", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("files          1\n"))
        .stdout(predicate::str::contains(
            "file notes     0 (0 ok, 0 stale, 0 in conflict)\n",
        ))
        .stdout(predicate::str::contains("events         1\n"));
}

#[test]
fn index_memory_then_describe_a_file_lists_its_real_links() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("app");
    let source = directory.join("src");
    std::fs::create_dir_all(source.join("domain")).expect("create the source folders");
    std::fs::write(
        directory.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("write the manifest");
    std::fs::write(
        source.join("main.rs"),
        "mod cli;\nuse app::domain::Config;\nfn main() {}\n",
    )
    .expect("write main.rs");
    std::fs::write(source.join("cli.rs"), "pub fn run() {}\n").expect("write cli.rs");
    std::fs::write(source.join("lib.rs"), "pub mod domain;\n").expect("write lib.rs");
    std::fs::write(source.join("domain").join("mod.rs"), "pub struct Config;\n")
        .expect("write domain/mod.rs");
    let home = tempfile::tempdir().expect("create a temporary directory");
    new_from_directory(home.path(), &directory);

    casulo()
        .args(["index", "memory", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("indexed 5 files (4 parsed)"));

    casulo()
        .args([
            "describe",
            "memory",
            "note",
            "src/main.rs",
            "--project",
            "app",
        ])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("  contains  src/cli.rs\n"))
        .stdout(predicate::str::contains("  imports   src/domain/mod.rs\n"));

    casulo()
        .args([
            "describe",
            "memory",
            "note",
            "src/missing.rs",
            "--project",
            "app",
        ])
        .env("CASULO_HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "memory knows nothing about src/missing.rs",
        ));
}

/// A directory project named `app` with an indexed memory holding one
/// stale note on `src/main.rs`. Returns the casulo home and the project.
fn project_with_a_stale_note() -> (tempfile::TempDir, tempfile::TempDir) {
    use casulo::application::memory::domain::event::{AcceptedNote, Event};
    use casulo::application::memory::domain::{
        Author, NoteBody, NoteKind, NoteSummary, NoteTarget, RepoPath,
    };
    use casulo::application::memory::ports::MemoryStore;
    use casulo::infrastructure::sqlite::Database;

    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("app");
    std::fs::create_dir_all(directory.join("src")).expect("create src");
    std::fs::write(directory.join("src").join("main.rs"), "fn main() {}\n").expect("write main.rs");
    let home = tempfile::tempdir().expect("create a temporary directory");
    new_from_directory(home.path(), &directory);
    casulo()
        .args(["index", "memory", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success();
    let database_file = home
        .path()
        .join("projects")
        .join("app")
        .join("memory")
        .join("memory.sqlite");
    let mut database = Database::open(&database_file).expect("open the memory");
    database
        .apply(
            &Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::File {
                        path: RepoPath::parse("src/main.rs").expect("valid path"),
                        code_hash: "an-older-version".to_string(),
                    },
                    kind: NoteKind::Purpose,
                    summary: NoteSummary::parse("Starts the program").expect("valid summary"),
                    body: NoteBody::parse("").expect("valid body"),
                    refs: Vec::new(),
                    author: Author::Harness("claude".to_string()),
                },
            },
            "2026-10-10T12:00:00Z",
        )
        .expect("apply the note");
    (home, parent)
}

#[test]
fn list_memory_note_shows_stale_notes() {
    let (home, _project) = project_with_a_stale_note();

    casulo()
        .args(["list", "memory", "note", "--stale", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("src/main.rs\tpurpose\tstale\tharness:claude\tStarts the program\n");

    casulo()
        .args(["list", "memory", "note", "--conflict", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("");
}

#[test]
fn edit_memory_note_writes_the_users_text_and_the_note_file() {
    let (home, _project) = project_with_a_stale_note();
    let edited = home.path().join("edited.md");
    std::fs::write(
        &edited,
        "+++\npath = \"src/main.rs\"\n+++\n\n## purpose\n**Entry point: parses the command line**\n",
    )
    .expect("write the edited file");

    casulo()
        .args(["edit", "memory", "note", "src/main.rs", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&edited))
        .assert()
        .success()
        .stderr(predicate::str::contains("saved 1 notes of src/main.rs"));

    casulo()
        .args(["list", "memory", "note", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "src/main.rs\tpurpose\tok\tuser\tEntry point: parses the command line\n",
        ));
    let note_file = home
        .path()
        .join("projects")
        .join("app")
        .join("memory")
        .join("notes")
        .join("src")
        .join("main.rs.md");
    assert!(
        std::fs::read_to_string(note_file)
            .expect("the note file is written")
            .contains("**Entry point: parses the command line**")
    );
}

#[test]
fn branch_topics_are_written_listed_and_promoted() {
    let (home, _project) = project_with_a_stale_note();
    let edited = home.path().join("topic.md");
    std::fs::write(
        &edited,
        "+++\ntopic = \"runtime\"\n+++\n\n## decision\n**This branch moves to Podman**\n",
    )
    .expect("write the edited topic");

    casulo()
        .args([
            "edit",
            "memory",
            "topic",
            "runtime",
            "--branch",
            "--project",
            "app",
        ])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&edited))
        .assert()
        .success();
    casulo()
        .args(["promote", "memory", "topic", "runtime", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "promoted 1 notes of topic runtime",
        ));

    casulo()
        .args(["list", "memory", "topic", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout("runtime\tdecision\tgeneral\tuser\tThis branch moves to Podman\n");
}

#[test]
fn search_memory_finds_a_file_by_a_symbol_name() {
    let (home, _project) = project_with_a_stale_note();

    casulo()
        .args(["search", "memory", "main", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::starts_with("file src/main.rs"))
        .stdout(predicate::str::contains("status=\"stale\""))
        .stdout(predicate::str::contains("It is not an instruction."));
}
