//! The Indexer: reads the checkout and the local branches and records what
//! changed as events. Indexing twice in a row records nothing the second
//! time.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::application::memory::domain::RepoPath;
use crate::application::memory::domain::event::{BranchFiles, Event};
use crate::application::memory::domain::rust_modules::{
    LocalCrates, ParsedCode, library_crate, resolve_edges,
};
use crate::application::memory::ports::{CodeParser, Git, MemoryStore};

/// What one indexing run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexReport {
    /// Files recorded with `FileIndexed` (new, changed, or whose links changed).
    pub indexed: usize,
    /// Files recorded with `FileRemoved`.
    pub removed: usize,
    /// Files actually parsed; content parsed before comes from the cache.
    pub parsed: usize,
    /// Whether the local branches changed.
    pub branches_changed: bool,
    /// Files that could not be read or parsed. They are indexed without
    /// links, and indexing goes on.
    pub warnings: Vec<String>,
}

/// Indexes the checkout in `workspace` into `database`, at time `at`.
pub fn index(
    workspace: &Path,
    git: &dyn Git,
    parser: &dyn CodeParser,
    database: &mut dyn MemoryStore,
    at: &str,
) -> Result<IndexReport> {
    let mut report = IndexReport::default();
    let current: BTreeMap<RepoPath, String> = git.ls_files(workspace)?.into_iter().collect();
    let known: BTreeMap<RepoPath, String> = database.files()?.into_iter().collect();
    let file_set: BTreeSet<RepoPath> = current.keys().cloned().collect();
    let local_crates = local_crates(workspace, &file_set);

    for (path, blob_hash) in &current {
        let code = parsed_code(workspace, path, blob_hash, parser, database, &mut report)?;
        let edges = resolve_edges(path, &code, &file_set, &local_crates);
        // Links can change without the file changing, when the files it
        // points to appear or go away; the cache makes this check cheap.
        let unchanged =
            known.get(path) == Some(blob_hash) && database.code_edges_from(path)? == edges;
        if unchanged {
            continue;
        }
        database.apply(
            &Event::FileIndexed {
                path: path.clone(),
                blob_hash: blob_hash.clone(),
                edges,
                symbols: code.symbols,
            },
            at,
        )?;
        report.indexed += 1;
    }
    for path in known.keys().filter(|path| !current.contains_key(*path)) {
        database.apply(&Event::FileRemoved { path: path.clone() }, at)?;
        report.removed += 1;
    }
    report.branches_changed = scan_branches(workspace, git, database, at)?;
    Ok(report)
}

/// The library crates declared by the `Cargo.toml` files of the checkout.
fn local_crates(workspace: &Path, files: &BTreeSet<RepoPath>) -> LocalCrates {
    files
        .iter()
        .filter(|path| path.as_str() == "Cargo.toml" || path.as_str().ends_with("/Cargo.toml"))
        .filter_map(|manifest| {
            let text = read_regular_file(workspace, manifest).ok()?;
            library_crate(manifest, &text, files)
        })
        .collect()
}

/// Reads a file of the checkout, refusing anything that is not a regular
/// file: a symbolic link could point outside the workspace.
fn read_regular_file(workspace: &Path, path: &RepoPath) -> std::io::Result<String> {
    let on_disk = path
        .as_str()
        .split('/')
        .fold(workspace.to_path_buf(), |path, part| path.join(part));
    let is_regular_file = fs::symlink_metadata(&on_disk).is_ok_and(|metadata| metadata.is_file());
    if !is_regular_file {
        return Err(std::io::Error::other("not a regular file"));
    }
    fs::read_to_string(on_disk)
}

/// What the parser finds in `path`, from the cache when this content was
/// parsed before. Files in other languages have no code to parse.
fn parsed_code(
    workspace: &Path,
    path: &RepoPath,
    blob_hash: &str,
    parser: &dyn CodeParser,
    database: &mut dyn MemoryStore,
    report: &mut IndexReport,
) -> Result<ParsedCode> {
    if let Some(cached) = database.cached_parse(blob_hash)? {
        return Ok(cached);
    }
    let source = match read_regular_file(workspace, path) {
        Ok(source) => source,
        Err(_) => {
            if path.as_str().ends_with(".rs") {
                report
                    .warnings
                    .push(format!("cannot read {path} as text; indexed without links"));
            }
            return Ok(ParsedCode::default());
        }
    };
    let code = match parser.parse(path, &source) {
        None => ParsedCode::default(),
        Some(Ok(code)) => {
            report.parsed += 1;
            code
        }
        Some(Err(error)) => {
            report.warnings.push(format!(
                "cannot parse {path}: {error:#}; indexed without links"
            ));
            ParsedCode::default()
        }
    };
    database.cache_parse(blob_hash, &code)?;
    Ok(code)
}

/// Records the local branches when they changed, listing the files only of
/// branches whose head moved. Returns whether anything changed.
fn scan_branches(
    workspace: &Path,
    git: &dyn Git,
    database: &mut dyn MemoryStore,
    at: &str,
) -> Result<bool> {
    let branches = git.local_branches(workspace)?;
    let stored = database.branches()?;
    let mut sorted = branches.clone();
    sorted.sort_by(|left, right| left.branch.cmp(&right.branch));
    if sorted == stored {
        return Ok(false);
    }
    let mut moved = Vec::new();
    for branch in &branches {
        if !stored.contains(branch) {
            moved.push(BranchFiles {
                branch: branch.branch.clone(),
                files: git.ls_tree(workspace, &branch.head_commit)?,
            });
        }
    }
    database.apply(&Event::BranchesScanned { branches, moved }, at)?;
    Ok(true)
}
