//! Let the agent push from the container, never to protected branches.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::application::project::domain::PushPolicy;
use crate::application::project::domain::push::CONTAINER_PUSH_DIR;
use crate::application::sandbox::domain::Mount;
use crate::application::sandbox::services::replace_files::replace_files;
use crate::application::shared::domain::{CasuloHome, ProjectName};

/// The read-only mount of the push settings, when push is enabled.
pub fn push_mounts(
    home: &CasuloHome,
    project: &ProjectName,
    policy: Option<&PushPolicy>,
) -> Vec<Mount> {
    match policy {
        Some(_) => vec![Mount {
            source: home.push_dir(project),
            target: CONTAINER_PUSH_DIR.to_string(),
            read_only: true,
        }],
        None => Vec::new(),
    }
}

/// Writes `push.gitconfig` and `hooks/pre-push` to the project's `push/`
/// directory, or removes it when push is off, so a disabled push leaves
/// nothing behind.
pub fn write_push_files(
    home: &CasuloHome,
    project: &ProjectName,
    policy: Option<&PushPolicy>,
) -> Result<()> {
    let directory = home.push_dir(project);
    let Some(policy) = policy else {
        return replace_files(&directory, &[]);
    };
    let gitconfig = directory.join("push.gitconfig");
    let hook = directory.join("hooks").join("pre-push");
    replace_files(
        &directory,
        &[
            (gitconfig, policy.gitconfig()),
            (hook.clone(), policy.pre_push_hook()),
        ],
    )?;
    make_executable(&hook)
}

/// git only runs hooks that are executable.
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    // `PermissionsExt` sets Unix file modes; it only exists on Unix.
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("cannot make {} executable", path.display()))
}

/// Windows has no executable bit; Docker Desktop shows bind-mounted files as
/// executable in the Linux container.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Whether the push reminder is still to be shown for `project`, and marks
/// it as shown. The reminder appears once, not on every run.
pub fn take_reminder(home: &CasuloHome, project: &ProjectName) -> Result<bool> {
    let marker = home.project_dir(project).join("push-reminder-shown");
    if marker.exists() {
        return Ok(false);
    }
    fs::write(&marker, "").with_context(|| format!("cannot write {}", marker.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::application::project::domain::GitHost;
    use crate::application::sandbox::test_support::built_in_harness;

    fn setup() -> (tempfile::TempDir, CasuloHome, ProjectName) {
        let directory = tempfile::tempdir().unwrap();
        let home = CasuloHome::resolve(Some(OsString::from(directory.path())), None).unwrap();
        let project = ProjectName::parse("demo").unwrap();
        fs::create_dir_all(home.project_dir(&project)).unwrap();
        (directory, home, project)
    }

    fn policy() -> PushPolicy {
        PushPolicy {
            protected_branches: vec!["main".to_string()],
            token: None,
            host: GitHost::parse("github.com").unwrap(),
        }
    }

    #[test]
    fn files_are_written_when_enabled_and_removed_when_disabled() {
        let (_directory, home, project) = setup();

        write_push_files(&home, &project, Some(&policy())).unwrap();

        let directory = home.push_dir(&project);
        assert!(directory.join("push.gitconfig").is_file());
        assert!(directory.join("hooks").join("pre-push").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(directory.join("hooks").join("pre-push"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }

        write_push_files(&home, &project, None).unwrap();
        assert!(!directory.exists());
    }

    #[test]
    fn mount_only_when_enabled() {
        let (_directory, home, project) = setup();

        assert!(push_mounts(&home, &project, None).is_empty());
        assert_eq!(
            push_mounts(&home, &project, Some(&policy())),
            [Mount {
                source: home.push_dir(&project),
                target: "/etc/casulo".to_string(),
                read_only: true,
            }]
        );
    }

    /// The `--dry-run` line with push on or off, with the temporary home
    /// shown as `~/.casulo` and `/` separators.
    fn dry_run(home: &CasuloHome, project: &ProjectName, policy: Option<&PushPolicy>) -> String {
        use crate::application::project::domain::GitIdentity;
        use crate::application::project::domain::push::TOKEN_VARIABLE;
        use crate::application::sandbox::domain::EnvironmentVariable;
        use crate::application::sandbox::ports::Containers;
        use crate::application::sandbox::services::run;
        use crate::infrastructure::docker::Docker;

        let claude = built_in_harness("claude");
        let mut spec = run::run_spec(
            home.workspace_dir(project),
            project,
            &claude,
            &GitIdentity::default(),
            None,
            claude.command.clone(),
            push_mounts(home, project, policy),
        );
        let mut secrets_set = vec!["ANTHROPIC_API_KEY".to_string()];
        if policy.is_some() {
            spec.environment.push(EnvironmentVariable::FromHost {
                name: TOKEN_VARIABLE.to_string(),
            });
            secrets_set.push(TOKEN_VARIABLE.to_string());
        }
        let line = Docker::new("docker").run_command_lines(&spec).remove(0);
        // Masking is checked here; the snapshot uses the raw arguments, which
        // `display` would quote differently where paths have `\`.
        assert!(
            line.display(&secrets_set)
                .starts_with("ANTHROPIC_API_KEY=*** ")
        );
        let args = line
            .args
            .join("\n")
            .replace(&home.root().display().to_string(), "~/.casulo")
            .replace('\\', "/");
        format!("{} {}", secrets_set.join("=*** ") + "=***", args)
    }

    #[test]
    fn dry_run_with_push_off_and_on() {
        let (_directory, home, project) = setup();

        let off = dry_run(&home, &project, None);
        let on = dry_run(&home, &project, Some(&policy()));

        assert!(!off.contains("CASULO_GIT_TOKEN"));
        assert!(on.starts_with("ANTHROPIC_API_KEY=*** CASULO_GIT_TOKEN=*** run"));
        assert!(!off.contains("/etc/casulo"));
        insta::assert_snapshot!(on);
    }

    #[test]
    fn reminder_is_shown_once() {
        let (_directory, home, project) = setup();

        assert!(take_reminder(&home, &project).unwrap());
        assert!(!take_reminder(&home, &project).unwrap());
    }
}
