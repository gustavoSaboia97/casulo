//! The `[git]` settings: who the agent's commits are by, and whether and
//! how it may push. Also the git config and hook that enforce push rules in
//! the container.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::config::GitIdentity;
use super::git_host::GitHost;
use super::repository_url::RepositoryUrl;
use crate::application::shared::domain::Secret;

/// Branches protected when the config names none.
const DEFAULT_PROTECTED_BRANCHES: [&str; 2] = ["main", "master"];

/// The variable the token reaches the container in. A variable with this
/// name set on the host wins over `token` in the config.
pub const TOKEN_VARIABLE: &str = "CASULO_GIT_TOKEN";

/// Where the push settings are mounted in the container. The image's system
/// git config includes `push.gitconfig` from here, which only exists when
/// push is enabled.
pub const CONTAINER_PUSH_DIR: &str = "/etc/casulo";

/// `[git]` in a `casulo.toml`. In a project's file each value set there
/// wins over the global one; anything left out comes from the defaults.
// `try_from` reads the table into `UncheckedGitSettings` first, then checks
// the branch patterns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedGitSettings")]
pub struct GitSettings {
    /// Name the agent's commits are made with, such as "Ada Lovelace".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Email the agent's commits are made with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Whether the agent may push. Off by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push: Option<bool>,
    /// The token the agent pushes with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<Secret>,
    /// The only host the token is sent to. Defaults to the host of the
    /// project's repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<GitHost>,
    /// Branches the agent can never push to; `*` and `?` are globs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected_branches: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UncheckedGitSettings {
    name: Option<String>,
    email: Option<String>,
    push: Option<bool>,
    token: Option<Secret>,
    host: Option<GitHost>,
    protected_branches: Option<Vec<String>>,
}

impl TryFrom<UncheckedGitSettings> for GitSettings {
    type Error = anyhow::Error;

    fn try_from(unchecked: UncheckedGitSettings) -> Result<Self> {
        for pattern in unchecked.protected_branches.iter().flatten() {
            check_branch_pattern(pattern)?;
        }
        Ok(Self {
            name: unchecked.name,
            email: unchecked.email,
            push: unchecked.push,
            token: unchecked.token,
            host: unchecked.host,
            protected_branches: unchecked.protected_branches,
        })
    }
}

/// The identity of the agent's commits: each value from the project, else
/// the global config.
pub fn choose_identity(project: &GitSettings, global: &GitSettings) -> GitIdentity {
    GitIdentity {
        name: project.name.clone().or_else(|| global.name.clone()),
        email: project.email.clone().or_else(|| global.email.clone()),
    }
}

/// Branch names and globs: letters, digits, `.`, `_`, `-`, `/`, `*` and `?`.
/// The pattern goes into a shell `case`, so nothing else is allowed.
fn check_branch_pattern(pattern: &str) -> Result<()> {
    let allowed = !pattern.is_empty()
        && !pattern.starts_with('-')
        && pattern
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-/*?".contains(character));
    if !allowed {
        bail!(
            "invalid protected branch \"{pattern}\"; use branch names or globs such as main or release/*"
        );
    }
    Ok(())
}

/// Push as allowed for one project, after applying the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushPolicy {
    /// Branches the agent can never push to.
    pub protected_branches: Vec<String>,
    /// The token from the config, if it has one. Without it, the token must
    /// be set on the host as `CASULO_GIT_TOKEN`.
    pub token: Option<Secret>,
    /// The only host git sends the token to.
    pub host: GitHost,
}

/// The push policy: each value from the project, else the global config,
/// else the default. The host comes from the project's `[git]`, else the
/// repository the project was cloned from, else the global `[git]`.
/// `None` when push is not enabled; an error when it is but no host is
/// known.
pub fn choose_push(
    project: &GitSettings,
    global: &GitSettings,
    repository: Option<&RepositoryUrl>,
) -> Result<Option<PushPolicy>> {
    let enabled = project.push.or(global.push).unwrap_or(false);
    if !enabled {
        return Ok(None);
    }
    let host = project
        .host
        .clone()
        .or_else(|| repository.and_then(|url| GitHost::from_url(url.as_str())))
        .or_else(|| global.host.clone());
    let Some(host) = host else {
        bail!(
            "push is on, but casulo does not know which host to push to; set host, such as host = \"github.com\", in [git] with `casulo config --project <name>`"
        );
    };
    let protected_branches = project
        .protected_branches
        .clone()
        .or_else(|| global.protected_branches.clone())
        .unwrap_or_else(|| DEFAULT_PROTECTED_BRANCHES.map(str::to_string).to_vec());
    let token = project.token.clone().or_else(|| global.token.clone());
    Ok(Some(PushPolicy {
        protected_branches,
        token,
        host,
    }))
}

impl PushPolicy {
    /// The git config included by the image's system config: a credential
    /// helper that answers with the token variable, so the token never shows
    /// in a command line, for the project's host only; SSH URLs of that host
    /// rewritten to HTTPS, since the container has no SSH key; and the
    /// casulo hooks instead of the repository's.
    pub fn gitconfig(&self) -> String {
        // The rewrite happens each time git reads a URL, so the repository's
        // own config, shared with the host, keeps its SSH remote.
        format!(
            "# Generated by casulo on every run. Read-only in the container.\n\
             [credential \"https://{host}\"]\n\
             \thelper = \"!f() {{ test $1 = get && echo username=x-access-token && echo password=${token}; }}; f\"\n\
             [url \"https://{host}/\"]\n\
             \tinsteadOf = git@{host}:\n\
             \tinsteadOf = ssh://git@{host}/\n\
             [core]\n\
             \thooksPath = {CONTAINER_PUSH_DIR}/hooks\n",
            host = self.host,
            token = TOKEN_VARIABLE
        )
    }

    /// A `pre-push` hook that rejects pushes to protected branches and the
    /// deletion of any remote branch.
    pub fn pre_push_hook(&self) -> String {
        // Validated patterns only hold characters that are safe in a `case`.
        let patterns = self.protected_branches.join("|");
        format!(
            r#"#!/bin/sh
# Generated by casulo on every run. Stops pushes to protected branches and
# branch deletions. It prevents mistakes only: `git push --no-verify` skips
# it, so protect your branches on the server too.
status=0
while read -r local_ref local_sha remote_ref remote_sha; do
    case "$local_sha" in
        *[!0]*) ;;
        *)
            echo "casulo: deleting $remote_ref is blocked; delete remote branches yourself" >&2
            status=1
            continue
            ;;
    esac
    branch=${{remote_ref#refs/heads/}}
    case "$branch" in
        {patterns})
            echo "casulo: pushing to $branch is blocked by protected_branches in casulo.toml; push another branch and open a pull request" >&2
            status=1
            ;;
    esac
done
exit $status
"#
        )
    }
}

/// The reminder shown once when push is enabled.
pub fn push_reminder(policy: &PushPolicy) -> String {
    format!(
        "push is enabled: the agent can push with your token. protected_branches only stops mistakes; to really protect {}, add a branch protection rule on the server: https://docs.github.com/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets",
        policy.protected_branches.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> GitSettings {
        toml::from_str(text).unwrap()
    }

    fn repository(url: &str) -> RepositoryUrl {
        RepositoryUrl::parse(url).unwrap()
    }

    #[test]
    fn push_is_off_by_default() {
        assert_eq!(
            choose_push(&GitSettings::default(), &GitSettings::default(), None).unwrap(),
            None
        );
        assert_eq!(
            choose_push(&settings("push = false"), &settings("push = true"), None).unwrap(),
            None
        );
    }

    #[test]
    fn enabled_push_uses_defaults_then_global_then_project() {
        let global = settings("push = true\ntoken = \"global-token\"\nhost = \"github.com\"\n");

        let defaults = choose_push(&GitSettings::default(), &global, None)
            .unwrap()
            .unwrap();
        let project = choose_push(
            &settings(
                "protected_branches = [\"main\", \"release/*\"]\ntoken = \"project-token\"\n",
            ),
            &global,
            None,
        )
        .unwrap()
        .unwrap();

        assert_eq!(defaults.protected_branches, ["main", "master"]);
        assert_eq!(defaults.token, Some(Secret::new("global-token")));
        assert_eq!(project.protected_branches, ["main", "release/*"]);
        assert_eq!(project.token, Some(Secret::new("project-token")));
    }

    #[test]
    fn the_host_comes_from_the_project_then_the_repository_then_global() {
        let global = settings("push = true\nhost = \"global.example.com\"\n");
        let ssh = repository("git@gitlab.example.com:you/app.git");
        let host = |project: &GitSettings, repository: Option<&RepositoryUrl>| {
            choose_push(project, &global, repository)
                .unwrap()
                .unwrap()
                .host
                .to_string()
        };

        assert_eq!(
            host(&settings("host = \"github.com\""), Some(&ssh)),
            "github.com"
        );
        assert_eq!(
            host(&GitSettings::default(), Some(&ssh)),
            "gitlab.example.com"
        );
        assert_eq!(
            host(&GitSettings::default(), Some(&repository("/home/you/app"))),
            "global.example.com"
        );
        assert_eq!(host(&GitSettings::default(), None), "global.example.com");
    }

    #[test]
    fn push_without_a_known_host_is_an_error() {
        let error = choose_push(&settings("push = true"), &GitSettings::default(), None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("set host"), "{error}");
    }

    #[test]
    fn the_project_identity_wins_one_value_at_a_time() {
        let global = settings("name = \"Ada\"\nemail = \"ada@example.com\"\n");
        let project = settings("email = \"ada@work.example\"\n");

        let identity = choose_identity(&project, &global);

        assert_eq!(identity.name.as_deref(), Some("Ada"));
        assert_eq!(identity.email.as_deref(), Some("ada@work.example"));
    }

    #[test]
    fn rejects_unsafe_patterns_and_unknown_keys() {
        for text in [
            "protected_branches = [\"main; rm -rf /\"]",
            "protected_branches = [\"\"]",
            "protected_branches = [\"a)b\"]",
            "token_env = \"CASULO_GIT_TOKEN\"",
            "enabled = true",
            "host = \"https://github.com\"",
        ] {
            assert!(toml::from_str::<GitSettings>(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_token_never_shows_in_debug_output_or_the_reminder() {
        let settings =
            settings("push = true\ntoken = \"ghp_secretvalue\"\nhost = \"github.com\"\n");
        let policy = choose_push(&settings, &GitSettings::default(), None)
            .unwrap()
            .unwrap();

        assert!(!format!("{settings:?}").contains("ghp_secretvalue"));
        assert!(!push_reminder(&policy).contains("ghp_secretvalue"));
    }

    fn policy() -> PushPolicy {
        PushPolicy {
            protected_branches: vec!["main".to_string(), "release/*".to_string()],
            token: None,
            host: GitHost::parse("github.com").unwrap(),
        }
    }

    #[test]
    fn generated_hook() {
        insta::assert_snapshot!(policy().pre_push_hook());
    }

    #[test]
    fn generated_gitconfig_names_the_variable_only() {
        insta::assert_snapshot!(policy().gitconfig());
    }

    /// Runs the hook like git does, with one line per ref on stdin.
    #[cfg(unix)]
    fn run_hook(lines: &str) -> bool {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        use std::process::{Command, Stdio};

        let directory = tempfile::tempdir().unwrap();
        let hook = directory.path().join("pre-push");
        std::fs::write(&hook, policy().pre_push_hook()).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut child = Command::new(&hook)
            .args(["origin", "https://example.com/app.git"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(lines.as_bytes())
            .unwrap();
        child.wait().unwrap().success()
    }

    #[cfg(unix)]
    #[test]
    fn hook_blocks_protected_branches_and_deletions() {
        let sha = "1".repeat(40);
        let zero = "0".repeat(40);
        let push = |local: &str, remote: &str| format!("refs/heads/x {local} {remote} {zero}\n");

        assert!(run_hook(&push(&sha, "refs/heads/feature/login")));
        assert!(run_hook(&push(&sha, "refs/tags/v1.0")));
        assert!(run_hook(""));
        assert!(!run_hook(&push(&sha, "refs/heads/main")));
        assert!(!run_hook(&push(&sha, "refs/heads/release/1.0")));
        assert!(!run_hook(&push(&zero, "refs/heads/feature/login")));
        assert!(!run_hook(&format!(
            "{}{}",
            push(&sha, "refs/heads/feature"),
            push(&sha, "refs/heads/main")
        )));
    }
}
