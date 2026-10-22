//! The dependency rules between layers and groups, checked on the source
//! (see Architecture in `AGENTS.md`).
//!
//! Rust does not stop cycles between modules of one crate, so without this
//! test the rules would only live on paper. It reads each file under `src/`
//! up to its `#[cfg(test)] mod`, finds every `crate::` path, and reports
//! the ones that break a rule as `file:line`. A rule about a folder that
//! does not exist yet finds no files, so the test tightens as the code moves.
//!
//! Limits: paths through `super::` and the `casulo::` paths of `main.rs` are
//! not followed.

use std::path::Path;

/// What each group of `application/` may import besides itself.
const ALLOWED_GROUPS: &[(&str, &[&str])] = &[
    ("shared", &[]),
    ("project", &["shared"]),
    ("agent", &["shared"]),
    ("doctor", &["shared"]),
    ("memory", &["shared"]),
    ("sandbox", &["shared", "project", "agent", "memory"]),
];

#[test]
fn src_follows_the_dependency_rules() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&src, &src, &mut files);

    let mut broken = Vec::new();
    for (relative, text) in &files {
        for (line, reason) in violations(relative, text) {
            broken.push(format!("src/{relative}:{line}: {reason}"));
        }
    }

    assert!(
        broken.is_empty(),
        "dependency rules broken (see Architecture in AGENTS.md):\n{}",
        broken.join("\n")
    );
}

/// Every `.rs` file under `directory`, as its path relative to `src` with
/// `/` separators, and its text.
fn collect_rust_files(src: &Path, directory: &Path, files: &mut Vec<(String, String)>) {
    let entries = std::fs::read_dir(directory).expect("read a source directory");
    for entry in entries {
        let path = entry.expect("read a directory entry").path();
        if path.is_dir() {
            collect_rust_files(src, &path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path
                .strip_prefix(src)
                .expect("the file is under src")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let text = std::fs::read_to_string(&path).expect("read a source file");
            files.push((relative, text));
        }
    }
}

/// Where a file lives, as far as the rules care.
#[derive(Debug, PartialEq)]
enum Place {
    /// A file in `application/<group>/`.
    Group(String),
    /// A file directly in `application/`, such as its `mod.rs`.
    Application,
    Infrastructure,
    /// `main.rs`, `lib.rs`, `cli/`, `api/` and the folders not moved yet:
    /// no rule applies to what they import.
    Free,
}

/// The place of a file from its path relative to `src`.
/// A group's own module may be `application/<group>.rs` or
/// `application/<group>/mod.rs`, so both belong to the group.
fn place_of(relative: &str) -> Place {
    let parts: Vec<&str> = relative.split('/').collect();
    // A slice pattern matches the parts by position: `[a, b, ..]` is "a,
    // then b, then anything", so each arm reads like the path it matches.
    match parts.as_slice() {
        ["application.rs"] | ["application", "mod.rs"] => Place::Application,
        ["application", group, _, ..] => Place::Group(group.to_string()),
        ["application", file] => Place::Group(file.trim_end_matches(".rs").to_string()),
        ["infrastructure.rs"] | ["infrastructure", ..] => Place::Infrastructure,
        _ => Place::Free,
    }
}

/// The rules `relative` breaks in `text`, as line numbers and reasons.
/// Only the code before the `#[cfg(test)] mod` is checked: unit tests may
/// use any module, such as the built-in harnesses in `infrastructure`.
fn violations(relative: &str, text: &str) -> Vec<(usize, String)> {
    let place = place_of(relative);
    if place == Place::Free {
        return Vec::new();
    }
    let code = code_before_tests(text);
    let mut found = Vec::new();
    if let Place::Group(group) = &place
        && !ALLOWED_GROUPS.iter().any(|(name, _)| name == group)
    {
        found.push((
            1,
            format!("unknown group {group}; add it to the allowed dependencies first"),
        ));
    }
    for (line, path) in crate_paths(&code) {
        if let Some(reason) = broken_rule(&place, &path) {
            found.push((line, reason));
        }
    }
    if matches!(place, Place::Group(_) | Place::Application) {
        for (index, line) in code.lines().enumerate() {
            if line.contains("clap::") || line.trim_start().starts_with("use clap") {
                found.push((index + 1, "application never uses clap".to_string()));
            }
        }
    }
    found
}

/// The rule that importing `path` (such as `application::sandbox::run`)
/// from `place` breaks, if any.
fn broken_rule(place: &Place, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split("::").collect();
    let target = parts[0];
    match place {
        Place::Group(_) | Place::Application
            if matches!(target, "infrastructure" | "cli" | "api") =>
        {
            Some(format!("application never imports crate::{target}"))
        }
        Place::Infrastructure if matches!(target, "cli" | "api") => {
            Some(format!("infrastructure never imports crate::{target}"))
        }
        Place::Group(group) if target == "application" && parts.len() > 1 => {
            let other = parts[1];
            let allowed = ALLOWED_GROUPS
                .iter()
                .find(|(name, _)| name == group)
                .map_or(&[][..], |(_, allowed)| *allowed);
            if other == group || allowed.contains(&other) {
                None
            } else {
                Some(format!(
                    "{group} may not depend on {other} (allowed: {})",
                    if allowed.is_empty() {
                        "nothing".to_string()
                    } else {
                        allowed.join(", ")
                    }
                ))
            }
        }
        _ => None,
    }
}

/// `text` up to its `#[cfg(test)] mod`, with `//` comments blanked so the
/// paths they mention do not count. Lines keep their numbers. A
/// `#[cfg(test)]` on anything but a module is checked like other code, so
/// a test helper in the middle of a file cannot hide the code after it.
fn code_before_tests(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut code = String::new();
    for (index, line) in lines.iter().enumerate() {
        // `#![cfg(test)]` makes the whole file test code, such as a module
        // of test fakes in its own file.
        if line.trim_start().starts_with("#![cfg(test)]") {
            return String::new();
        }
        let next = lines.get(index + 1).map_or("", |next| next.trim_start());
        if line.trim_start().starts_with("#[cfg(test)]")
            && (next.starts_with("mod ") || next.starts_with("pub mod "))
        {
            break;
        }
        let without_comment = match line.find("//") {
            Some(start) => &line[..start],
            None => line,
        };
        code.push_str(without_comment);
        code.push('\n');
    }
    code
}

/// Every path that follows a `crate::` in `code`, with the line it starts
/// on. A `use` with braces names several paths:
/// `crate::{a::b, c::{d, e}}` names `a::b`, `c::d` and `c::e`.
fn crate_paths(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (start, _) in code.match_indices("crate::") {
        // `casulo_crate::x` is not a `crate::` path.
        let before = code[..start].chars().next_back();
        if before.is_some_and(|character| character.is_alphanumeric() || character == '_') {
            continue;
        }
        let line = code[..start].matches('\n').count() + 1;
        let mut rest = &code[start + "crate::".len()..];
        for path in use_tree(&mut rest) {
            found.push((line, path));
        }
    }
    found
}

/// Reads one path, or one group in braces, from the start of `rest`, and
/// moves `rest` past it. Returns the full paths it names.
///
/// `rest: &mut &str` works as a cursor: the function can point the caller's
/// `&str` at a later part of the same text, without copying it, so each
/// call continues where the previous one stopped.
fn use_tree(rest: &mut &str) -> Vec<String> {
    let mut prefix: Vec<String> = Vec::new();
    loop {
        *rest = rest.trim_start();
        if let Some(inside) = rest.strip_prefix('{') {
            *rest = inside;
            return braced_paths(rest, &prefix);
        }
        let length = rest
            .find(|character: char| !(character.is_alphanumeric() || character == '_'))
            .unwrap_or(rest.len());
        if length == 0 {
            // `*`, or the end of the path.
            return vec![prefix.join("::")];
        }
        prefix.push(rest[..length].to_string());
        *rest = &rest[length..];
        match rest.strip_prefix("::") {
            Some(after) => *rest = after,
            None => return vec![prefix.join("::")],
        }
    }
}

/// Reads the paths inside braces, after the `{`, up to the matching `}`,
/// each one under `prefix`.
fn braced_paths(rest: &mut &str, prefix: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    loop {
        for path in use_tree(rest) {
            // A trailing comma before `}` reads as an empty path.
            if path.is_empty() && prefix.is_empty() {
                continue;
            }
            let mut full = prefix.to_vec();
            if !path.is_empty() {
                full.push(path);
            }
            paths.push(full.join("::"));
        }
        // Skip what may follow a path inside braces, such as `as name`.
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        *rest = &rest[end..];
        match rest.chars().next() {
            Some(',') => *rest = &rest[1..],
            Some('}') => {
                *rest = &rest[1..];
                return paths;
            }
            _ => return paths,
        }
    }
}

#[test]
fn reads_plain_grouped_and_nested_paths() {
    let code = "use crate::application::shared::Name;\nuse crate::{\n    application::{project::Config, sandbox},\n    infrastructure::git::GitCli as Git,\n};\nlet x = crate::cli::run();\n";

    assert_eq!(
        crate_paths(code),
        [
            (1, "application::shared::Name".to_string()),
            (2, "application::project::Config".to_string()),
            (2, "application::sandbox".to_string()),
            (2, "infrastructure::git::GitCli".to_string()),
            (6, "cli::run".to_string()),
        ]
    );
}

#[test]
fn a_group_may_not_import_a_group_outside_its_allowed_list() {
    let text = "use crate::application::shared::Name;\nuse crate::application::sandbox::RunSpec;\n";

    assert_eq!(
        violations("application/project/services/new_project.rs", text),
        [(
            2,
            "project may not depend on sandbox (allowed: shared)".to_string()
        )]
    );
}

#[test]
fn sandbox_may_import_project_agent_and_memory() {
    let text = "use crate::application::{project::Config, agent::Harness, memory::Note};\n";

    assert!(violations("application/sandbox/services/run.rs", text).is_empty());
}

#[test]
fn application_never_imports_infrastructure_input_or_clap() {
    let text = "use crate::infrastructure::config_file;\nuse crate::cli::ask;\n#[derive(clap::ValueEnum)]\nenum Kind {}\n";

    assert_eq!(
        violations("application/shared/domain/resource.rs", text),
        [
            (
                1,
                "application never imports crate::infrastructure".to_string()
            ),
            (2, "application never imports crate::cli".to_string()),
            (3, "application never uses clap".to_string()),
        ]
    );
}

#[test]
fn infrastructure_never_imports_input() {
    let text = "use crate::application::project::ports::Git;\nuse crate::cli::environment;\n";

    assert_eq!(
        violations("infrastructure/git.rs", text),
        [(2, "infrastructure never imports crate::cli".to_string())]
    );
}

#[test]
fn unit_tests_and_comments_are_exempt() {
    let text = "// crate::application::sandbox is mentioned here\nfn f() {}\n\n#[cfg(test)]\nmod tests {\n    use crate::application::sandbox::RunSpec;\n}\n";

    assert!(violations("application/project/domain/config.rs", text).is_empty());
}

#[test]
fn a_group_missing_from_the_rules_is_reported() {
    assert_eq!(
        violations("application/billing/mod.rs", ""),
        [(
            1,
            "unknown group billing; add it to the allowed dependencies first".to_string()
        )]
    );
}

#[test]
fn input_and_the_folders_not_moved_yet_are_free() {
    let text =
        "use crate::infrastructure::docker::Docker;\nuse crate::application::sandbox::run;\n";

    for relative in [
        "main.rs",
        "cli/commands/run.rs",
        "api/memory_mcp/tools.rs",
        "services/run.rs",
    ] {
        assert!(violations(relative, text).is_empty(), "{relative}");
    }
}

#[test]
fn a_group_module_file_follows_its_group_rules() {
    let text = "pub mod services;\npub use crate::application::sandbox::RunSpec;\n";

    assert_eq!(
        violations("application/project.rs", text),
        [(
            2,
            "project may not depend on sandbox (allowed: shared)".to_string()
        )]
    );
    assert_eq!(place_of("application/mod.rs"), Place::Application);
    assert_eq!(place_of("application.rs"), Place::Application);
}

#[test]
fn a_test_helper_in_the_middle_does_not_hide_the_code_after_it() {
    let text = "#[cfg(test)]\nfn helper() {}\n\nuse crate::application::sandbox::RunSpec;\n";

    assert_eq!(
        violations("application/project/domain/config.rs", text),
        [(
            4,
            "project may not depend on sandbox (allowed: shared)".to_string()
        )]
    );
}

#[test]
fn a_file_of_test_code_is_exempt() {
    let text = "//! Fakes for the project tests.\n#![cfg(test)]\nuse crate::infrastructure::harness_files;\n";

    assert!(violations("application/project/test_support.rs", text).is_empty());
}
