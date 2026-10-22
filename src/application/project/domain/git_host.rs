//! The host the agent pushes to, such as `github.com`.

use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// A host name that is safe to write into a git config: letters, digits,
/// `.` and `-`, with no port, path or user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GitHost(String);

impl GitHost {
    /// Parses a host name such as `github.com` or `gitlab.example.com`.
    pub fn parse(value: &str) -> Result<Self> {
        let allowed = !value.is_empty()
            && !value.starts_with(['-', '.'])
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-".contains(character));
        if !allowed {
            bail!(
                "invalid git host \"{value}\"; write only the host name, such as github.com, without https:// or a path"
            );
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// The host of a git remote URL, in any form git accepts:
    /// `https://github.com/you/app`, `ssh://git@github.com/you/app.git` or
    /// `git@github.com:you/app.git`. `None` for a local path or a URL whose
    /// host is not a plain host name.
    pub fn from_url(url: &str) -> Option<Self> {
        let authority = match url.split_once("://") {
            Some((_scheme, rest)) => rest.split('/').next().unwrap_or_default(),
            None => {
                // The scp-like form `user@host:path`. A `/` before the `:`
                // means a local path, and a one-letter host is a Windows
                // drive such as `C:\`.
                let (authority, _path) = url.split_once(':')?;
                if authority.contains(['/', '\\']) || authority.len() == 1 {
                    return None;
                }
                authority
            }
        };
        let without_user = authority.rsplit('@').next().unwrap_or_default();
        let without_port = without_user.split(':').next().unwrap_or_default();
        Self::parse(without_port).ok()
    }

    /// Returns the host as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitHost {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for GitHost {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<GitHost> for String {
    fn from(host: GitHost) -> Self {
        host.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_of(url: &str) -> Option<String> {
        GitHost::from_url(url).map(|host| host.to_string())
    }

    #[test]
    fn accepts_host_names() {
        assert_eq!(GitHost::parse("github.com").unwrap().as_str(), "github.com");
        assert_eq!(
            GitHost::parse("GitLab.Example.com").unwrap().as_str(),
            "gitlab.example.com"
        );
    }

    #[test]
    fn rejects_anything_that_is_not_a_host_name() {
        for value in [
            "",
            "https://github.com",
            "github.com/you",
            "github.com:22",
            "git@github.com",
            "-github.com",
            ".github.com",
            "github.com\"]\n[core]",
            "git hub.com",
        ] {
            assert!(GitHost::parse(value).is_err(), "{value:?}");
        }
    }

    #[test]
    fn finds_the_host_in_every_remote_form() {
        assert_eq!(
            host_of("https://github.com/you/app").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("http://git.example.com:8080/app.git").as_deref(),
            Some("git.example.com")
        );
        assert_eq!(
            host_of("ssh://git@github.com/you/app.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("ssh://git@gitlab.example.com:2222/you/app.git").as_deref(),
            Some("gitlab.example.com")
        );
        assert_eq!(
            host_of("git@github.com:you/app.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("gitlab.example.com:you/app.git").as_deref(),
            Some("gitlab.example.com")
        );
    }

    #[test]
    fn local_paths_have_no_host() {
        for url in [
            "/home/you/app",
            "./app",
            "C:\\Users\\you\\app",
            "C:/Users/you/app",
            "../a:b",
            "file:///home/you/app",
        ] {
            assert_eq!(host_of(url), None, "{url}");
        }
    }
}
