//! Rewrites a directory of files casulo generates before each run.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Writes each of `files` (a path under `directory` and its contents), then
/// removes every other file under `directory` and the directories left
/// empty, so a file from an earlier run does not linger.
///
/// Files are overwritten in place, and a directory that still holds a file is
/// never removed: Docker Desktop on macOS can miss a directory deleted and
/// created again just before `docker run`, and then fails to mount it.
pub fn replace_files(directory: &Path, files: &[(PathBuf, String)]) -> Result<()> {
    for (path, text) in files {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    }
    let keep: HashSet<&Path> = files.iter().map(|(path, _)| path.as_path()).collect();
    match remove_others(directory, &keep) {
        Ok(true) => remove_empty_directory(directory),
        Ok(false) => Ok(()),
        Err(error) if is_not_found(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Removes the files under `directory` that are not in `keep`, and the
/// directories left empty. Returns whether `directory` is empty now.
fn remove_others(directory: &Path, keep: &HashSet<&Path>) -> Result<bool> {
    let entries =
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?;
    let mut empty = true;
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("cannot read {}", path.display()))?;
        if file_type.is_dir() {
            if remove_others(&path, keep)? {
                remove_empty_directory(&path)?;
            } else {
                empty = false;
            }
        } else if keep.contains(path.as_path()) {
            empty = false;
        } else {
            fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
        }
    }
    Ok(empty)
}

fn remove_empty_directory(directory: &Path) -> Result<()> {
    fs::remove_dir(directory).with_context(|| format!("cannot remove {}", directory.display()))
}

/// Whether `error` comes from a directory that does not exist, which means
/// there was nothing to remove.
fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn inode(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(path).unwrap().ino()
    }

    #[test]
    fn keeps_the_directories_that_still_hold_files() {
        // Given a directory with a file from an earlier run
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join("generated");
        let agents = directory.join("agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("deleted.md"), "old").unwrap();
        #[cfg(unix)]
        let before = inode(&agents);

        // When it is replaced with another file in the same directory
        replace_files(
            &directory,
            &[(agents.join("reviewer.md"), "new".to_string())],
        )
        .unwrap();

        // Then only the new file is left, in the same directory as before
        assert!(!agents.join("deleted.md").exists());
        assert_eq!(
            fs::read_to_string(agents.join("reviewer.md")).unwrap(),
            "new"
        );
        #[cfg(unix)]
        assert_eq!(inode(&agents), before);
    }

    #[test]
    fn removes_the_directories_left_empty() {
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join("generated");
        fs::create_dir_all(directory.join("agents")).unwrap();
        fs::write(directory.join("agents").join("reviewer.md"), "old").unwrap();

        replace_files(&directory, &[]).unwrap();

        assert!(!directory.exists());
    }

    #[test]
    fn a_missing_directory_is_created_or_left_missing() {
        let home = tempfile::tempdir().unwrap();
        let directory = home.path().join("generated");

        replace_files(&directory, &[]).unwrap();
        assert!(!directory.exists());

        let file = directory.join("mcp.json");
        replace_files(&directory, &[(file.clone(), "{}".to_string())]).unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), "{}");
    }
}
