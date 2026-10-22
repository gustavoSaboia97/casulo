//! How Rust files link to each other: `mod foo;` and `use crate::...`,
//! resolved against the files of the checkout. Pure functions, so every
//! rule is tested without git or a parser.
//!
//! Only what can be resolved for sure becomes an edge: a missing edge is
//! better than a wrong one. `pub use` re-exports, `#[path]`, macros and
//! other crates are not followed.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::event::Edge;
use super::note::EdgeKind;
use super::repo_path::RepoPath;

/// What a parser found in one source file, before resolution. Cached by
/// content hash, so the same content is never parsed twice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedCode {
    /// Names of `mod name;` declarations (without a body or `#[path]`).
    pub modules: Vec<String>,
    /// Paths of `use` declarations, expanded, such as `crate::domain::config`.
    pub uses: Vec<String>,
    /// Names of top-level functions, structs, enums and traits.
    pub symbols: Vec<String>,
}

/// Expands the argument of a `use` into one path per imported item:
/// `crate::a::{b, c::{d, self}}` becomes `crate::a::b`, `crate::a::c::d`
/// and `crate::a::c`. Aliases (`as x`) and globs (`*`) keep the path before
/// them.
pub fn expand_use(argument: &str) -> Vec<String> {
    let mut paths = Vec::new();
    expand_into("", argument.trim(), &mut paths);
    paths
}

fn expand_into(prefix: &str, text: &str, paths: &mut Vec<String>) {
    let join = |rest: &str| match (prefix.is_empty(), rest.is_empty()) {
        (true, _) => rest.to_string(),
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix}::{rest}"),
    };
    if let Some(open) = text.find('{') {
        let head: String = text[..open]
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let inner = text[open + 1..]
            .trim_end()
            .strip_suffix('}')
            .unwrap_or(&text[open + 1..]);
        let base = join(head.trim_end_matches("::"));
        for item in split_top_level(inner) {
            expand_into(&base, item.trim(), paths);
        }
        return;
    }
    // `foo as bar` imports `foo`: the path is the first word.
    let leaf: String = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    // `foo::*` and `self` import the module itself.
    let trimmed = leaf
        .trim_end_matches('*')
        .trim_end_matches("::")
        .strip_suffix("self")
        .map(|rest| rest.trim_end_matches("::").to_string())
        .unwrap_or_else(|| {
            leaf.trim_end_matches('*')
                .trim_end_matches("::")
                .to_string()
        });
    let path = join(&trimmed);
    if !path.is_empty() && !paths.contains(&path) {
        paths.push(path);
    }
}

/// Splits `a, b::{c, d}, e` on the commas that are not inside braces.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect()
}

/// The library crates of the repository: crate name as used in code (with
/// `_`, such as `casulo_memory`) to the folder of its `lib.rs`.
pub type LocalCrates = BTreeMap<String, String>;

/// The edges from `file` to other files of `files`, from what was parsed in
/// it. `mod foo;` gives `Contains`; `use crate::...`, `use super::...`,
/// `use self::...` and `use <local crate>::...` give `Imports`.
/// Unresolvable paths are dropped.
pub fn resolve_edges(
    file: &RepoPath,
    parsed: &ParsedCode,
    files: &BTreeSet<RepoPath>,
    local_crates: &LocalCrates,
) -> Vec<Edge> {
    let mut edges = Vec::new();
    let mut push = |to: RepoPath, kind: EdgeKind| {
        if &to != file
            && !edges
                .iter()
                .any(|edge: &Edge| edge.to == to && edge.kind == kind)
        {
            edges.push(Edge { to, kind });
        }
    };
    let children_dir = children_directory(file);
    for module in &parsed.modules {
        if let Some(child) = module_file(&join(&children_dir, module), files) {
            push(child, EdgeKind::Contains);
        }
    }
    let own_root = crate_root(file, files);
    let current = own_root
        .as_ref()
        .map(|root| module_path(file, root))
        .unwrap_or_default();
    for path in &parsed.uses {
        let mut segments: Vec<&str> = path.split("::").collect();
        let first = segments.first().copied().unwrap_or_default();
        // A binary importing its own library, or one crate of a workspace
        // importing another, names the library crate.
        let (root, base): (String, Vec<String>) = if let Some(library) = local_crates.get(first) {
            segments.remove(0);
            (library.clone(), Vec::new())
        } else {
            let Some(root) = own_root.clone() else {
                continue;
            };
            let base = match first {
                "crate" => {
                    segments.remove(0);
                    Vec::new()
                }
                "self" => {
                    segments.remove(0);
                    current.clone()
                }
                "super" => {
                    let mut base = current.clone();
                    while segments.first() == Some(&"super") {
                        segments.remove(0);
                        if base.pop().is_none() {
                            break;
                        }
                    }
                    base
                }
                _ => continue,
            };
            (root, base)
        };
        let mut full = base;
        full.extend(segments.iter().map(|segment| segment.to_string()));
        // The longest prefix that is a module file is the one imported:
        // `crate::domain::config::GlobalConfig` is `src/domain/config.rs`.
        for length in (1..=full.len()).rev() {
            let relative = full[..length].join("/");
            if let Some(target) = module_file(&join(&root, &relative), files) {
                push(target, EdgeKind::Imports);
                break;
            }
        }
    }
    edges
        .sort_by(|left, right| (left.to.as_str(), left.kind).cmp(&(right.to.as_str(), right.kind)));
    edges
}

/// The library crate a `Cargo.toml` declares, as `(name in code, folder of
/// its lib.rs)`, when its library file is among `files`. `manifest` is the
/// path of the `Cargo.toml` and `text` its content.
pub fn library_crate(
    manifest: &RepoPath,
    text: &str,
    files: &BTreeSet<RepoPath>,
) -> Option<(String, String)> {
    let table: toml::Table = toml::from_str(text).ok()?;
    let package_name = table.get("package")?.get("name")?.as_str()?;
    let library = table.get("lib");
    let name = library
        .and_then(|library| library.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or(package_name)
        .replace('-', "_");
    let library_path = library
        .and_then(|library| library.get("path"))
        .and_then(|path| path.as_str())
        .unwrap_or("src/lib.rs");
    let library_file = RepoPath::parse(&join(&parent(manifest), library_path)).ok()?;
    files
        .contains(&library_file)
        .then(|| (name, parent(&library_file)))
}

/// Whether `file` is a crate root or a `mod.rs`, whose child modules live
/// next to it instead of in a folder named after it.
fn is_directory_owner(file: &RepoPath) -> bool {
    matches!(file_name(file), "lib.rs" | "main.rs" | "mod.rs")
}

fn file_name(file: &RepoPath) -> &str {
    file.as_str().rsplit('/').next().unwrap_or_default()
}

fn parent(file: &RepoPath) -> String {
    file.as_str()
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

fn join(directory: &str, relative: &str) -> String {
    if directory.is_empty() {
        relative.to_string()
    } else {
        format!("{directory}/{relative}")
    }
}

/// Where `mod foo;` in `file` looks for `foo`: next to a crate root or
/// `mod.rs`, else in the folder named after the file.
fn children_directory(file: &RepoPath) -> String {
    if is_directory_owner(file) {
        parent(file)
    } else {
        let stem = file.as_str().strip_suffix(".rs").unwrap_or(file.as_str());
        stem.to_string()
    }
}

/// `dir/name.rs`, else `dir/name/mod.rs`, if one of them is in `files`.
fn module_file(base: &str, files: &BTreeSet<RepoPath>) -> Option<RepoPath> {
    [format!("{base}.rs"), format!("{base}/mod.rs")]
        .into_iter()
        .filter_map(|candidate| RepoPath::parse(&candidate).ok())
        .find(|candidate| files.contains(candidate))
}

/// The folder of the crate `file` belongs to: the nearest folder above it
/// with a `lib.rs` or `main.rs`.
fn crate_root(file: &RepoPath, files: &BTreeSet<RepoPath>) -> Option<String> {
    let mut directory = parent(file);
    loop {
        for root_file in ["lib.rs", "main.rs"] {
            if RepoPath::parse(&join(&directory, root_file))
                .is_ok_and(|candidate| files.contains(&candidate))
            {
                return Some(directory);
            }
        }
        if directory.is_empty() {
            return None;
        }
        directory = match directory.rsplit_once('/') {
            Some((up, _)) => up.to_string(),
            None => String::new(),
        };
    }
}

/// The module path of `file` inside the crate at `root`: `src/domain/config.rs`
/// is `["domain", "config"]`, and a crate root or `mod.rs` is its folder.
fn module_path(file: &RepoPath, root: &str) -> Vec<String> {
    let relative = if root.is_empty() {
        file.as_str()
    } else {
        file.as_str()
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(file.as_str())
    };
    let without_extension = relative.strip_suffix(".rs").unwrap_or(relative);
    let mut segments: Vec<String> = without_extension.split('/').map(str::to_string).collect();
    let is_crate_root = segments.len() == 1 && matches!(segments[0].as_str(), "lib" | "main");
    if is_crate_root || segments.last().is_some_and(|last| last == "mod") {
        segments.pop();
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_use_lists_aliases_globs_and_self() {
        assert_eq!(expand_use("std::fs"), ["std::fs"]);
        assert_eq!(
            expand_use("crate::domain::{config::GlobalConfig, Harness, self as d}"),
            [
                "crate::domain::config::GlobalConfig",
                "crate::domain::Harness",
                "crate::domain"
            ]
        );
        assert_eq!(
            expand_use("super::{a::{b, c}, d::*}"),
            ["super::a::b", "super::a::c", "super::d"]
        );
        assert_eq!(expand_use("crate::alias as other"), ["crate::alias"]);
        assert_eq!(
            expand_use("crate::a::{\n    b as c,\n    d::{self, e},\n}"),
            ["crate::a::b", "crate::a::d", "crate::a::d::e"]
        );
        // `assets` starts with "as" but is not an alias.
        assert_eq!(expand_use("crate::assets"), ["crate::assets"]);
        assert_eq!(expand_use("crate::{}"), Vec::<String>::new());
    }

    fn files(paths: &[&str]) -> BTreeSet<RepoPath> {
        paths
            .iter()
            .map(|path| RepoPath::parse(path).unwrap())
            .collect()
    }

    fn edges(file: &str, parsed: &ParsedCode, all: &BTreeSet<RepoPath>) -> Vec<(String, EdgeKind)> {
        let local_crates =
            LocalCrates::from([("memory".to_string(), "crates/memory/src".to_string())]);
        resolve_edges(&RepoPath::parse(file).unwrap(), parsed, all, &local_crates)
            .into_iter()
            .map(|edge| (edge.to.to_string(), edge.kind))
            .collect()
    }

    fn crate_files() -> BTreeSet<RepoPath> {
        files(&[
            "src/main.rs",
            "src/cli.rs",
            "src/cli/doctor.rs",
            "src/domain/mod.rs",
            "src/domain/config.rs",
            "src/domain/harness.rs",
            "tests/cli.rs",
            "crates/memory/src/lib.rs",
            "crates/memory/src/domain/mod.rs",
        ])
    }

    #[test]
    fn mod_declarations_contain_child_files() {
        let parsed = ParsedCode {
            modules: vec!["cli".into(), "domain".into(), "missing".into()],
            ..ParsedCode::default()
        };

        assert_eq!(
            edges("src/main.rs", &parsed, &crate_files()),
            [
                ("src/cli.rs".to_string(), EdgeKind::Contains),
                ("src/domain/mod.rs".to_string(), EdgeKind::Contains)
            ]
        );
        let doctor = ParsedCode {
            modules: vec!["doctor".into()],
            ..ParsedCode::default()
        };
        assert_eq!(
            edges("src/cli.rs", &doctor, &crate_files()),
            [("src/cli/doctor.rs".to_string(), EdgeKind::Contains)]
        );
    }

    #[test]
    fn crate_super_and_self_paths_import_the_longest_module_file() {
        let parsed = ParsedCode {
            uses: vec![
                "crate::domain::config::GlobalConfig".into(),
                "crate::domain::Harness".into(),
                "super::harness::Harness".into(),
                "self::nothing".into(),
                "std::fs".into(),
                "anyhow::Result".into(),
            ],
            ..ParsedCode::default()
        };

        assert_eq!(
            edges("src/domain/config.rs", &parsed, &crate_files()),
            [
                ("src/domain/harness.rs".to_string(), EdgeKind::Imports),
                ("src/domain/mod.rs".to_string(), EdgeKind::Imports)
            ]
        );
    }

    #[test]
    fn each_crate_of_a_workspace_has_its_own_root() {
        let parsed = ParsedCode {
            uses: vec!["crate::domain::Thing".into(), "super::cli".into()],
            ..ParsedCode::default()
        };

        assert_eq!(
            edges("crates/memory/src/lib.rs", &parsed, &crate_files()),
            [(
                "crates/memory/src/domain/mod.rs".to_string(),
                EdgeKind::Imports
            )]
        );
        // `tests/cli.rs` is a crate of its own with no root file: nothing to resolve.
        assert!(edges("tests/cli.rs", &parsed, &crate_files()).is_empty());
    }

    #[test]
    fn a_binary_imports_its_library_by_the_crate_name() {
        let parsed = ParsedCode {
            uses: vec!["memory::domain::Thing".into(), "serde::Serialize".into()],
            ..ParsedCode::default()
        };

        assert_eq!(
            edges("src/main.rs", &parsed, &crate_files()),
            [(
                "crates/memory/src/domain/mod.rs".to_string(),
                EdgeKind::Imports
            )]
        );
    }

    #[test]
    fn library_crates_come_from_cargo_manifests() {
        let all = files(&[
            "Cargo.toml",
            "src/lib.rs",
            "crates/memory/Cargo.toml",
            "crates/memory/src/lib.rs",
            "tools/Cargo.toml",
        ]);
        let manifest = |path: &str| RepoPath::parse(path).unwrap();

        assert_eq!(
            library_crate(
                &manifest("Cargo.toml"),
                "[package]\nname = \"casulo\"\n",
                &all
            ),
            Some(("casulo".to_string(), "src".to_string()))
        );
        assert_eq!(
            library_crate(
                &manifest("crates/memory/Cargo.toml"),
                "[package]\nname = \"casulo-memory\"\n",
                &all
            ),
            Some(("casulo_memory".to_string(), "crates/memory/src".to_string()))
        );
        // No lib.rs: a binary-only package has no library to import.
        assert_eq!(
            library_crate(
                &manifest("tools/Cargo.toml"),
                "[package]\nname = \"tools\"\n",
                &all
            ),
            None
        );
        // A workspace manifest without [package], and invalid TOML.
        assert_eq!(
            library_crate(&manifest("Cargo.toml"), "[workspace]\n", &all),
            None
        );
        assert_eq!(
            library_crate(&manifest("Cargo.toml"), "not toml", &all),
            None
        );
    }

    #[test]
    fn module_paths_follow_rust_rules() {
        assert!(module_path(&RepoPath::parse("src/main.rs").unwrap(), "src").is_empty());
        assert_eq!(
            module_path(&RepoPath::parse("src/domain/mod.rs").unwrap(), "src"),
            ["domain"]
        );
        assert_eq!(
            module_path(&RepoPath::parse("src/domain/config.rs").unwrap(), "src"),
            ["domain", "config"]
        );
    }
}
