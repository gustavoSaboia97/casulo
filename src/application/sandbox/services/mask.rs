//! Secret masking: hide workspace paths such as `.env` from the agent.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::sandbox::domain::Mount;
use crate::application::sandbox::domain::container::CONTAINER_WORKSPACE;
use crate::application::shared::domain::{CasuloHome, ProjectName, WorkspacePath};

const EMPTY_FILE: &str = "empty-file";
const EMPTY_DIR: &str = "empty-dir";

/// Mounts that cover each masked path existing in the workspace with an
/// empty, read-only file (or directory, for a directory). Paths that do not
/// exist are skipped: there is nothing to hide.
pub fn mask_mounts(
    home: &CasuloHome,
    project: &ProjectName,
    workspace: &Path,
    mask: &[WorkspacePath],
) -> Vec<Mount> {
    let masks_dir = home.masks_dir(project);
    let mut mounts = Vec::new();
    for path in mask {
        // `metadata` follows symbolic links, so a link is covered like its target.
        let Ok(metadata) = fs::metadata(path.on_host(workspace)) else {
            continue;
        };
        let cover = if metadata.is_dir() {
            EMPTY_DIR
        } else {
            EMPTY_FILE
        };
        mounts.push(Mount {
            source: masks_dir.join(cover),
            target: path.in_container(CONTAINER_WORKSPACE),
            read_only: true,
        });
    }
    mounts
}

/// Creates the empty file and directory the mask mounts use, outside the
/// workspace, and makes sure the file is still empty.
pub fn prepare_mask_sources(home: &CasuloHome, project: &ProjectName) -> Result<()> {
    let masks_dir = home.masks_dir(project);
    let empty_dir: PathBuf = masks_dir.join(EMPTY_DIR);
    fs::create_dir_all(&empty_dir)
        .with_context(|| format!("cannot create {}", empty_dir.display()))?;
    let empty_file = masks_dir.join(EMPTY_FILE);
    fs::write(&empty_file, "").with_context(|| format!("cannot write {}", empty_file.display()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn setup() -> (tempfile::TempDir, CasuloHome, ProjectName) {
        let directory = tempfile::tempdir().unwrap();
        let home = CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap();
        let project = ProjectName::parse("demo").unwrap();
        let workspace = home.workspace_dir(&project);
        fs::create_dir_all(workspace.join("config").join("keys")).unwrap();
        fs::write(workspace.join(".env"), "TOKEN=secret").unwrap();
        fs::write(workspace.join("config").join("secrets.json"), "{}").unwrap();
        (directory, home, project)
    }

    fn paths(values: &[&str]) -> Vec<WorkspacePath> {
        values
            .iter()
            .map(|value| WorkspacePath::parse(value).unwrap())
            .collect()
    }

    /// The mounts as text, with the temporary home shown as `~/.casulo` and
    /// `/` separators, so the snapshot is the same on every OS.
    fn describe(mounts: &[Mount], home: &CasuloHome) -> String {
        mounts
            .iter()
            .map(|mount| {
                let source = mount.source.strip_prefix(home.root()).unwrap();
                let source: Vec<String> = source
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect();
                format!(
                    "~/.casulo/{} -> {}{}",
                    source.join("/"),
                    mount.target,
                    if mount.read_only { " (read-only)" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn existing_paths_are_covered_and_missing_ones_skipped() {
        let (_directory, home, project) = setup();
        let mask = paths(&[".env", "config/secrets.json", "config/keys", "missing.pem"]);

        let mounts = mask_mounts(&home, &project, &home.workspace_dir(&project), &mask);

        insta::assert_snapshot!(describe(&mounts, &home));
    }

    #[test]
    fn empty_mask_adds_no_mounts() {
        let (_directory, home, project) = setup();

        assert!(mask_mounts(&home, &project, &home.workspace_dir(&project), &[]).is_empty());
    }

    #[test]
    fn prepare_creates_an_empty_file_and_directory() {
        let (_directory, home, project) = setup();
        let masks_dir = home.masks_dir(&project);
        fs::create_dir_all(&masks_dir).unwrap();
        fs::write(masks_dir.join(EMPTY_FILE), "should be emptied").unwrap();

        prepare_mask_sources(&home, &project).unwrap();

        assert_eq!(fs::read_to_string(masks_dir.join(EMPTY_FILE)).unwrap(), "");
        assert!(masks_dir.join(EMPTY_DIR).is_dir());
    }
}
