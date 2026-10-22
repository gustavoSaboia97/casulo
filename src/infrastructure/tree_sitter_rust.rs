//! Reads Rust source with tree-sitter: top-level `mod`, `use` and item names.

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use crate::application::memory::domain::RepoPath;
use crate::application::memory::domain::rust_modules::{ParsedCode, expand_use};
use crate::application::memory::ports::CodeParser;

/// Top-level items whose names are indexed as symbols.
const SYMBOL_KINDS: [&str; 4] = ["function_item", "struct_item", "enum_item", "trait_item"];

/// A parser for `.rs` files.
pub struct TreeSitterRust;

impl CodeParser for TreeSitterRust {
    fn parse(&self, path: &RepoPath, source: &str) -> Option<Result<ParsedCode>> {
        path.as_str().ends_with(".rs").then(|| parse_rust(source))
    }
}

fn parse_rust(source: &str) -> Result<ParsedCode> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .context("cannot load the Rust grammar")?;
    let tree = parser
        .parse(source, None)
        .context("tree-sitter could not parse the file")?;
    let bytes = source.as_bytes();
    let text = |node: Node| node.utf8_text(bytes).unwrap_or_default().to_string();

    let mut parsed = ParsedCode::default();
    let root = tree.root_node();
    let mut cursor = root.walk();
    let mut follows_path_attribute = false;
    for node in root.children(&mut cursor) {
        let is_public = node
            .child(0)
            .is_some_and(|first| first.kind() == "visibility_modifier");
        match node.kind() {
            // `#[path = "..."] mod x;` points somewhere casulo does not follow.
            "attribute_item" => {
                follows_path_attribute = text(node).trim_start_matches("#[").starts_with("path");
                continue;
            }
            "mod_item" if node.child_by_field_name("body").is_none() && !follows_path_attribute => {
                if let Some(name) = node.child_by_field_name("name") {
                    parsed.modules.push(text(name));
                }
            }
            // `pub use` re-exports are not followed in v1.
            "use_declaration" if !is_public => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    parsed.uses.extend(expand_use(&text(argument)));
                }
            }
            kind if SYMBOL_KINDS.contains(&kind) => {
                if let Some(name) = node.child_by_field_name("name") {
                    parsed.symbols.push(text(name));
                }
            }
            _ => {}
        }
        follows_path_attribute = false;
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
mod config;
pub mod cli { pub fn x() {} }
#[path = "other.rs"]
mod weird;
#[cfg(test)]
mod tests;
use crate::domain::{config::GlobalConfig, Harness};
use super::name;
pub use self::x::Y;
pub fn run() {}
struct S;
enum E { A }
trait T {}
const C: u8 = 1;
impl S { fn method() {} }
"#;

    #[test]
    fn finds_modules_uses_and_top_level_symbols() {
        let parsed = parse_rust(SOURCE).unwrap();

        assert_eq!(parsed.modules, ["config", "tests"]);
        assert_eq!(
            parsed.uses,
            [
                "crate::domain::config::GlobalConfig",
                "crate::domain::Harness",
                "super::name"
            ]
        );
        assert_eq!(parsed.symbols, ["run", "S", "E", "T"]);
    }

    #[test]
    fn other_languages_are_not_parsed() {
        let readme = RepoPath::parse("README.md").unwrap();

        assert!(TreeSitterRust.parse(&readme, "# title").is_none());
    }

    #[test]
    fn broken_source_still_gives_what_it_can() {
        let parsed = parse_rust("mod ok;\nfn broken( {\n").unwrap();

        assert_eq!(parsed.modules, ["ok"]);
    }
}
