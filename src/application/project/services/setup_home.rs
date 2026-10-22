//! `casulo setup`: create the casulo home with documented defaults.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::application::project::ports::ConfigStore;
use crate::application::shared::domain::CasuloHome;

/// Template of `agent/AGENTS.md`: a comment saying what the file is for.
const AGENT_INSTRUCTIONS_TEMPLATE: &str = include_str!("../../../../templates/AGENTS.md");

/// One file or directory of the casulo home, and whether setup created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeEntry {
    /// Where it is.
    pub path: PathBuf,
    /// Whether it is a file (`false` for a directory).
    pub is_file: bool,
    /// `true` when this run created it, `false` when it already existed.
    pub created: bool,
}

/// Creates whatever is missing in the casulo home: its directories, the
/// global `casulo.toml` and `agent/AGENTS.md`, each from a commented
/// template. Never changes anything that already exists, so it is safe to
/// run again.
pub fn initialize_home(
    home: &CasuloHome,
    config_store: &dyn ConfigStore,
) -> Result<Vec<HomeEntry>> {
    let mut entries = Vec::new();
    for directory in [
        home.root().to_path_buf(),
        home.harnesses_dir(),
        home.projects_dir(),
        home.agent_dir(),
        home.skills_dir(),
        home.agents_dir(),
        home.mcp_dir(),
    ] {
        let created = !directory.exists();
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        entries.push(HomeEntry {
            path: directory,
            is_file: false,
            created,
        });
    }
    // The global config may hold secrets, so the store makes it readable
    // only by its owner. The instructions are mounted into the agent's
    // container, which may run as another user, so they keep the usual
    // permissions.
    let global_config = home.global_config_file();
    let created = config_store.create_global_config(&global_config)?;
    entries.push(HomeEntry {
        path: global_config,
        is_file: true,
        created,
    });
    let instructions = home.agent_instructions_file();
    let created = config_store.create_if_missing(&instructions, AGENT_INSTRUCTIONS_TEMPLATE)?;
    entries.push(HomeEntry {
        path: instructions,
        is_file: true,
        created,
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::infrastructure::config_file::{self, ConfigFiles};

    fn home_in(directory: &tempfile::TempDir) -> CasuloHome {
        CasuloHome::resolve(Some(OsString::from(directory.path().join("home"))), None)
            .expect("override is set")
    }

    #[test]
    fn creates_directories_and_commented_files() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);

        let entries = initialize_home(&home, &ConfigFiles).unwrap();

        assert!(entries.iter().all(|entry| entry.created));
        assert!(home.skills_dir().is_dir());
        assert!(home.agents_dir().is_dir());
        assert!(home.mcp_dir().is_dir());
        assert!(home.projects_dir().is_dir());
        assert_eq!(
            fs::read_to_string(home.global_config_file()).unwrap(),
            config_file::global_config_text()
        );
        assert!(home.agent_instructions_file().is_file());
    }

    #[test]
    fn second_run_keeps_what_exists() {
        let directory = tempfile::tempdir().unwrap();
        let home = home_in(&directory);
        initialize_home(&home, &ConfigFiles).unwrap();
        fs::write(home.global_config_file(), "runtime = \"podman\"\n").unwrap();

        let entries = initialize_home(&home, &ConfigFiles).unwrap();

        assert!(entries.iter().all(|entry| !entry.created));
        assert_eq!(
            fs::read_to_string(home.global_config_file()).unwrap(),
            "runtime = \"podman\"\n"
        );
    }
}
