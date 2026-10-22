//! The Indexer on a real repository: a small Rust crate with two branches,
//! read with the real git and tree-sitter adapters.

use std::fs;
use std::path::Path;

use casulo::application::memory::domain::event::Edge;
use casulo::application::memory::domain::{EdgeKind, RepoPath};
use casulo::application::memory::ports::MemoryStore;
use casulo::application::memory::services::indexer::{IndexReport, index};
use casulo::infrastructure::git::GitCli;
use casulo::infrastructure::sqlite::Database;
use casulo::infrastructure::tree_sitter_rust::TreeSitterRust;

use crate::common::{fixture, git, write};

const AT: &str = "2026-10-09T12:00:00Z";

fn run(root: &Path, database: &mut Database) -> IndexReport {
    index(root, &GitCli, &TreeSitterRust, database, AT).expect("index the fixture")
}

fn path(value: &str) -> RepoPath {
    RepoPath::parse(value).expect("valid path")
}

#[test]
fn first_index_reads_every_file_and_its_links() {
    let fixture = fixture();
    let mut database = Database::open_in_memory().unwrap();

    let report = run(fixture.path(), &mut database);

    assert_eq!(report.indexed, 5);
    assert_eq!(report.parsed, 4);
    assert!(report.branches_changed);
    assert!(report.warnings.is_empty());
    assert_eq!(
        database.edges_from(&path("src/main.rs")).unwrap(),
        [
            Edge {
                to: path("src/cli.rs"),
                kind: EdgeKind::Contains
            },
            Edge {
                to: path("src/domain/config.rs"),
                kind: EdgeKind::Imports
            },
            Edge {
                to: path("src/domain/mod.rs"),
                kind: EdgeKind::Contains
            },
        ]
    );
    assert_eq!(database.branches().unwrap().len(), 2);
}

#[test]
fn indexing_twice_records_nothing_the_second_time() {
    let fixture = fixture();
    let mut database = Database::open_in_memory().unwrap();
    run(fixture.path(), &mut database);
    let events = database.event_count().unwrap();

    let report = run(fixture.path(), &mut database);

    assert_eq!(report, IndexReport::default());
    assert_eq!(database.event_count().unwrap(), events);
}

#[test]
fn an_edit_without_git_add_records_one_event_with_the_hash_on_disk() {
    let fixture = fixture();
    let mut database = Database::open_in_memory().unwrap();
    run(fixture.path(), &mut database);
    let events = database.event_count().unwrap();
    let before = database.file_hash(&path("src/domain/config.rs")).unwrap();

    write(
        fixture.path(),
        "src/domain/config.rs",
        "pub struct Config;\npub struct More;\n",
    );
    let report = run(fixture.path(), &mut database);

    assert_eq!(report.indexed, 1);
    assert_eq!(database.event_count().unwrap(), events + 1);
    assert_ne!(
        database.file_hash(&path("src/domain/config.rs")).unwrap(),
        before
    );
}

#[test]
fn a_new_untracked_file_is_indexed_and_a_deleted_one_removed() {
    let fixture = fixture();
    let mut database = Database::open_in_memory().unwrap();
    run(fixture.path(), &mut database);

    write(fixture.path(), "notes.txt", "not ignored\n");
    fs::remove_file(fixture.path().join("README.md")).unwrap();
    let report = run(fixture.path(), &mut database);

    assert_eq!((report.indexed, report.removed), (1, 1));
    assert!(database.file_hash(&path("notes.txt")).unwrap().is_some());
    assert!(database.file_hash(&path("README.md")).unwrap().is_none());
}

#[test]
fn ignored_files_and_symbolic_links_are_left_out() {
    let fixture = fixture();
    write(fixture.path(), ".gitignore", "target/\n");
    write(fixture.path(), "target/debug/build.rs", "fn main() {}\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/passwd", fixture.path().join("link.rs")).unwrap();
    let mut database = Database::open_in_memory().unwrap();

    run(fixture.path(), &mut database);

    assert!(
        database
            .file_hash(&path("target/debug/build.rs"))
            .unwrap()
            .is_none()
    );
    assert!(database.file_hash(&path("link.rs")).unwrap().is_none());
    assert!(database.file_hash(&path(".gitignore")).unwrap().is_some());
}

#[test]
fn switching_branch_and_back_reparses_nothing() {
    let fixture = fixture();
    let mut database = Database::open_in_memory().unwrap();
    run(fixture.path(), &mut database);

    git(fixture.path(), &["checkout", "-q", "feature"]);
    let on_feature = run(fixture.path(), &mut database);
    git(fixture.path(), &["checkout", "-q", "main"]);
    let back_on_main = run(fixture.path(), &mut database);

    assert_eq!((on_feature.indexed, on_feature.parsed), (1, 1));
    assert_eq!((back_on_main.indexed, back_on_main.parsed), (1, 0));
}

#[test]
fn a_file_that_appears_adds_the_link_to_it() {
    let fixture = fixture();
    write(
        fixture.path(),
        "src/main.rs",
        "mod cli;\nmod domain;\nmod later;\nfn main() {}\n",
    );
    let mut database = Database::open_in_memory().unwrap();
    run(fixture.path(), &mut database);
    assert!(
        !database
            .edges_from(&path("src/main.rs"))
            .unwrap()
            .iter()
            .any(|edge| edge.to == path("src/later.rs"))
    );

    write(fixture.path(), "src/later.rs", "pub fn soon() {}\n");
    let report = run(fixture.path(), &mut database);

    // The new file, and main.rs, whose content did not change but whose
    // link now resolves.
    assert_eq!((report.indexed, report.parsed), (2, 1));
    assert!(
        database
            .edges_from(&path("src/main.rs"))
            .unwrap()
            .iter()
            .any(|edge| edge.to == path("src/later.rs"))
    );
}
