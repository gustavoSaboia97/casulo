//! Reads the user's agent setup under `~/.casulo/agent`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::agent::domain::{McpServer, Subagent};

/// Loads every `<name>.md` in `directory` as a subagent, sorted by name. A
/// missing directory means no subagents. Any invalid file is an error that
/// names it, so a typo is never silently skipped.
pub fn load_subagents(directory: &Path) -> Result<Vec<Subagent>> {
    let mut subagents = Vec::new();
    for path in files_with_extension(directory, "md")? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let file_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        let subagent = Subagent::parse(file_name, &text)
            .with_context(|| format!("invalid subagent {}", path.display()))?;
        subagents.push(subagent);
    }
    Ok(subagents)
}

/// Loads every `<name>.toml` in `directory` as an MCP server named after
/// the file, sorted by name. A missing directory means no servers.
pub fn load_mcp_servers(directory: &Path) -> Result<Vec<McpServer>> {
    let mut servers = Vec::new();
    for path in files_with_extension(directory, "toml")? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let file_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        let server = McpServer::parse(file_name, &text)
            .with_context(|| format!("invalid MCP server {}", path.display()))?;
        servers.push(server);
    }
    Ok(servers)
}

/// Files in `directory` with `extension`, sorted, so results and errors do
/// not depend on the directory order. A missing directory has none.
fn files_with_extension(directory: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", directory.display()));
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("cannot read {}", directory.display()))?
            .path();
        if path.is_file() && path.extension().is_some_and(|found| found == extension) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT: &str = "+++\nname = \"NAME\"\ndescription = \"d\"\n+++\nDo it.\n";

    #[test]
    fn missing_directory_has_no_subagents() {
        let directory = tempfile::tempdir().unwrap();

        assert!(
            load_subagents(&directory.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn loads_markdown_files_sorted_and_ignores_others() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["zeta", "alpha"] {
            fs::write(
                directory.path().join(format!("{name}.md")),
                AGENT.replace("NAME", name),
            )
            .unwrap();
        }
        fs::write(directory.path().join("notes.txt"), "not an agent").unwrap();

        let names: Vec<String> = load_subagents(directory.path())
            .unwrap()
            .iter()
            .map(|agent| agent.name.to_string())
            .collect();

        assert_eq!(names, ["alpha", "zeta"]);
    }

    #[test]
    fn loads_mcp_servers_named_after_their_files() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("docs.toml"),
            "url = \"https://example.com/mcp\"\n",
        )
        .unwrap();
        fs::write(directory.path().join("Bad.toml"), "url = \"https://x\"\n").unwrap();

        let error = load_mcp_servers(directory.path()).unwrap_err();
        assert!(format!("{error:#}").contains("Bad.toml"));

        fs::remove_file(directory.path().join("Bad.toml")).unwrap();
        let servers = load_mcp_servers(directory.path()).unwrap();
        assert_eq!(servers[0].name.as_str(), "docs");
    }

    #[test]
    fn invalid_file_is_an_error_that_names_it() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("broken.md"), "no frontmatter").unwrap();

        let error = load_subagents(directory.path()).unwrap_err();

        assert!(format!("{error:#}").contains("invalid subagent"));
        assert!(format!("{error:#}").contains("broken.md"));
    }
}
