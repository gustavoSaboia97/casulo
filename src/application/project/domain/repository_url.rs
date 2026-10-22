//! The URL of a git repository to clone.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::application::shared::domain::ProjectName;

/// A repository URL or path that is safe to hand to `git clone`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepositoryUrl(String);

impl RepositoryUrl {
    /// Parses a repository URL, such as `https://github.com/you/project`,
    /// `git@github.com:you/project.git` or a local path.
    ///
    /// URLs with credentials (`https://user:token@host/...`) are rejected: the
    /// URL is saved in the workspace, which the agent can read.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() {
            bail!("the repository URL is empty");
        }
        if value.starts_with('-') {
            bail!("invalid repository URL \"{value}\": it cannot start with \"-\"");
        }
        if value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            bail!(
                "invalid repository URL \"{value}\": it cannot contain spaces or control characters"
            );
        }
        if has_http_credentials(value) {
            bail!(
                "the repository URL contains credentials; remove them from the URL and let git use your credential helper or SSH key"
            );
        }
        Ok(Self(value.to_string()))
    }

    /// Returns the URL as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Suggests a project name from the last part of the URL, for example
    /// `my-project` for `https://github.com/you/My_Project.git`.
    pub fn suggested_project_name(&self) -> Result<ProjectName> {
        let last_part = self
            .0
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\', ':'])
            .next()
            .unwrap_or_default();
        let without_suffix = last_part.strip_suffix(".git").unwrap_or(last_part);
        match ProjectName::slug_of(without_suffix) {
            Some(name) => Ok(name),
            None => bail!(
                "cannot make a project name from \"{}\"; choose one with --name",
                self.0
            ),
        }
    }
}

/// Whether an http(s) URL has a user or password before the host.
fn has_http_credentials(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    let Some(rest) = ["https://", "http://"]
        .iter()
        .find_map(|scheme| lowercase.strip_prefix(scheme))
    else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    authority.contains('@')
}

impl fmt::Display for RepositoryUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RepositoryUrl {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<RepositoryUrl> for String {
    fn from(url: RepositoryUrl) -> Self {
        url.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggested(url: &str) -> String {
        RepositoryUrl::parse(url)
            .unwrap()
            .suggested_project_name()
            .unwrap()
            .to_string()
    }

    #[test]
    fn accepts_common_url_forms() {
        for url in [
            "https://github.com/you/project",
            "git@github.com:you/project.git",
            "ssh://git@github.com/you/project.git",
            "/home/you/project",
            "C:\\Users\\you\\project",
        ] {
            assert!(RepositoryUrl::parse(url).is_ok(), "{url} should be valid");
        }
    }

    #[test]
    fn rejects_option_like_and_blank_urls() {
        for url in ["", "   ", "--upload-pack=touch /tmp/x", "https://host/a b"] {
            assert!(
                RepositoryUrl::parse(url).is_err(),
                "{url:?} should be invalid"
            );
        }
    }

    #[test]
    fn rejects_credentials_in_http_urls() {
        for url in [
            "https://user:token@github.com/you/project",
            "https://token@github.com/you/project",
            "HTTP://user@host/project",
        ] {
            let error = RepositoryUrl::parse(url).unwrap_err();

            assert!(error.to_string().contains("contains credentials"));
            assert!(
                !error.to_string().contains("token"),
                "the error must not echo the secret"
            );
        }
    }

    #[test]
    fn suggests_name_from_last_part() {
        assert_eq!(suggested("https://github.com/you/project"), "project");
        assert_eq!(suggested("https://github.com/you/project.git/"), "project");
        assert_eq!(suggested("git@github.com:you/My_Project.git"), "my-project");
        assert_eq!(suggested("git@host:project.git"), "project");
        assert_eq!(suggested("/home/you/some.dir/repo"), "repo");
        assert_eq!(suggested("C:\\Users\\you\\Repo__Name"), "repo-name");
    }

    #[test]
    fn asks_for_a_name_when_none_can_be_made() {
        let error = RepositoryUrl::parse("https://host/___.git")
            .unwrap()
            .suggested_project_name()
            .unwrap_err();

        assert!(error.to_string().contains("--name"));
    }
}
