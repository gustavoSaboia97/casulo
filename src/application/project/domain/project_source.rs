//! Where a new project's workspace comes from: a repository to clone, or a
//! directory on the host used in place.

use std::path::PathBuf;

use anyhow::{Result, bail};

use super::repository_url::RepositoryUrl;

/// The argument of `casulo new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSource {
    /// A git URL, cloned into the project's own `workspace/`.
    Repository(RepositoryUrl),
    /// A directory on the host, mounted in place as the workspace.
    Directory(PathBuf),
}

impl ProjectSource {
    /// A git URL when it has `://` or starts with `git@`; a directory when it
    /// is `.` or `..` or has a path separator. Anything else is ambiguous.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.contains("://") || value.starts_with("git@") {
            return Ok(Self::Repository(RepositoryUrl::parse(value)?));
        }
        if value == "." || value == ".." || value.contains(['/', '\\']) {
            return Ok(Self::Directory(PathBuf::from(value)));
        }
        if value.is_empty() {
            bail!("give a git URL or a directory");
        }
        bail!(
            "\"{value}\" is neither a git URL nor a directory; for a directory here, use ./{value}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_repositories() {
        for value in [
            "https://github.com/you/app",
            "git@github.com:you/app.git",
            "ssh://git@example.com/app.git",
            "file:///srv/app.git",
        ] {
            assert!(
                matches!(
                    ProjectSource::parse(value),
                    Ok(ProjectSource::Repository(_))
                ),
                "{value}"
            );
        }
    }

    #[test]
    fn paths_are_directories() {
        for value in [
            ".",
            "..",
            "./app",
            "../app",
            "/home/you/app",
            "C:\\code\\app",
            "app/",
        ] {
            assert_eq!(
                ProjectSource::parse(value).unwrap(),
                ProjectSource::Directory(PathBuf::from(value)),
                "{value}"
            );
        }
    }

    #[test]
    fn bare_words_are_ambiguous() {
        let error = ProjectSource::parse("app").unwrap_err();

        assert!(error.to_string().contains("use ./app"));
        assert!(ProjectSource::parse("").is_err());
    }

    #[test]
    fn credentials_in_urls_are_still_rejected() {
        assert!(ProjectSource::parse("https://user:token@github.com/you/app").is_err());
    }
}
