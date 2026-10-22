//! Harness manifests: the ones built into casulo and the overrides in
//! `~/.casulo/harnesses/`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::application::agent::domain::Harness;
use crate::application::shared::domain::HarnessName;

/// Manifests shipped inside the binary. `include_str!` reads the files at
/// compile time, so casulo needs no extra files at run time.
const BUILT_IN: [(&str, &str); 2] = [
    ("claude", include_str!("../../harnesses/claude.toml")),
    ("codex", include_str!("../../harnesses/codex.toml")),
];

/// Where a harness came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessSource {
    BuiltIn,
    Override(PathBuf),
}

/// A harness together with where it was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedHarness {
    pub harness: Harness,
    pub source: HarnessSource,
}

/// Loads the built-in harnesses, then the `<name>.toml` files in
/// `overrides_dir`, which replace a built-in of the same name or add a new
/// one. A missing directory means no overrides. Sorted by name.
pub fn load_harnesses(overrides_dir: &Path) -> Result<Vec<LoadedHarness>> {
    let mut harnesses = built_in_harnesses();
    for loaded in load_overrides(overrides_dir)? {
        harnesses.retain(|existing| existing.harness.name != loaded.harness.name);
        harnesses.push(loaded);
    }
    harnesses.sort_by(|left, right| left.harness.name.as_str().cmp(right.harness.name.as_str()));
    Ok(harnesses)
}

/// Finds one harness by name, with overrides applied.
pub fn find_harness(overrides_dir: &Path, name: &HarnessName) -> Result<Harness> {
    let harnesses = load_harnesses(overrides_dir)?;
    let names: Vec<&str> = harnesses
        .iter()
        .map(|loaded| loaded.harness.name.as_str())
        .collect();
    match harnesses.iter().find(|loaded| &loaded.harness.name == name) {
        Some(loaded) => Ok(loaded.harness.clone()),
        None => bail!(
            "unknown harness \"{name}\"; available: {}",
            names.join(", ")
        ),
    }
}

fn built_in_harnesses() -> Vec<LoadedHarness> {
    BUILT_IN
        .iter()
        .map(|(name, text)| LoadedHarness {
            harness: Harness::parse_manifest(text)
                .unwrap_or_else(|error| panic!("built-in harness {name} is invalid: {error:#}")),
            source: HarnessSource::BuiltIn,
        })
        .collect()
}

fn load_overrides(directory: &Path) -> Result<Vec<LoadedHarness>> {
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
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            paths.push(path);
        }
    }
    // Sorted so errors and results do not depend on the directory order.
    paths.sort();

    let mut loaded = Vec::new();
    for path in paths {
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let harness = Harness::parse_manifest(&text)
            .with_context(|| format!("invalid harness manifest {}", path.display()))?;
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if file_stem != harness.name.as_str() {
            bail!(
                "invalid harness manifest {}: it is named \"{}\", so the file must be {}.toml",
                path.display(),
                harness.name,
                harness.name
            );
        }
        loaded.push(LoadedHarness {
            harness,
            source: HarnessSource::Override(path),
        });
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agent::domain::harness::{AgentsFormat, McpMode};

    fn names(harnesses: &[LoadedHarness]) -> Vec<&str> {
        harnesses
            .iter()
            .map(|loaded| loaded.harness.name.as_str())
            .collect()
    }

    fn write(directory: &Path, file: &str, text: &str) {
        fs::write(directory.join(file), text).unwrap();
    }

    #[test]
    fn built_in_manifests_are_valid() {
        let harnesses = built_in_harnesses();

        assert_eq!(names(&harnesses), ["claude", "codex"]);
        assert_eq!(harnesses[0].harness.command, ["claude"]);
        assert_eq!(
            harnesses[0].harness.auth_env,
            ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]
        );
        assert_eq!(
            harnesses[1].harness.instructions_file.as_deref(),
            Some("AGENTS.md")
        );
    }

    #[test]
    fn built_in_manifests_inject_into_their_own_paths() {
        let harnesses = built_in_harnesses();
        let claude = &harnesses[0].harness;
        let codex = &harnesses[1].harness;

        assert_eq!(claude.effort_args, ["--effort", "{effort}"]);
        assert_eq!(
            claude.inject.instructions.as_deref(),
            Some(".claude/CLAUDE.md")
        );
        assert_eq!(
            claude.inject.agents.as_ref().map(|agents| agents.format),
            Some(AgentsFormat::ClaudeMarkdown)
        );
        assert_eq!(claude.inject.mcp, Some(McpMode::ClaudeMcpConfig));
        assert_eq!(codex.model_args, ["--model", "{model}"]);
        assert_eq!(codex.inject.skills.as_deref(), Some(".agents/skills"));
        assert_eq!(
            codex.inject.agents.as_ref().map(|agents| agents.format),
            Some(AgentsFormat::CodexToml)
        );
        assert_eq!(codex.inject.mcp, Some(McpMode::CodexConfigOverrides));
    }

    #[test]
    fn missing_overrides_directory_gives_built_ins() {
        let directory = tempfile::tempdir().unwrap();

        let harnesses = load_harnesses(&directory.path().join("missing")).unwrap();

        assert_eq!(names(&harnesses), ["claude", "codex"]);
        assert!(
            harnesses
                .iter()
                .all(|loaded| loaded.source == HarnessSource::BuiltIn)
        );
    }

    #[test]
    fn override_replaces_built_in_and_new_files_add_harnesses() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "claude.toml",
            "name = \"claude\"\ninstall = [\"echo custom\"]\ncommand = [\"claude\", \"--verbose\"]\n",
        );
        write(
            directory.path(),
            "aider.toml",
            "name = \"aider\"\ninstall = [\"pip install aider-chat\"]\ncommand = [\"aider\"]\n",
        );
        write(directory.path(), "notes.txt", "not a manifest");

        let harnesses = load_harnesses(directory.path()).unwrap();

        assert_eq!(names(&harnesses), ["aider", "claude", "codex"]);
        let claude = &harnesses[1];
        assert_eq!(claude.harness.install, ["echo custom"]);
        assert_eq!(
            claude.source,
            HarnessSource::Override(directory.path().join("claude.toml"))
        );
        assert_eq!(harnesses[2].source, HarnessSource::BuiltIn);
    }

    #[test]
    fn invalid_override_names_the_file() {
        let directory = tempfile::tempdir().unwrap();
        write(directory.path(), "claude.toml", "name = \"claude\"\n");

        let error = load_harnesses(directory.path()).unwrap_err();

        assert!(format!("{error:#}").contains("invalid harness manifest"));
        assert!(format!("{error:#}").contains("claude.toml"));
    }

    #[test]
    fn file_name_must_match_harness_name() {
        let directory = tempfile::tempdir().unwrap();
        write(
            directory.path(),
            "codex.toml",
            "name = \"claude\"\ninstall = [\"true\"]\ncommand = [\"claude\"]\n",
        );

        let error = load_harnesses(directory.path()).unwrap_err();

        assert!(error.to_string().contains("the file must be claude.toml"));
    }

    #[test]
    fn finds_harness_or_lists_the_available_ones() {
        let directory = tempfile::tempdir().unwrap();
        let codex = HarnessName::parse("codex").unwrap();
        let missing = HarnessName::parse("missing").unwrap();

        assert_eq!(find_harness(directory.path(), &codex).unwrap().name, codex);
        let error = find_harness(directory.path(), &missing).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown harness \"missing\"; available: claude, codex"
        );
    }
}
