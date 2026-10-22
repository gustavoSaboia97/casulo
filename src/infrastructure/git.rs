//! The git command-line tool.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::application::doctor::domain::{Fix, HostOs, LinuxDistro};
use crate::application::doctor::ports::GitDiagnostics;
use crate::application::memory::domain::RepoPath;
use crate::application::memory::domain::event::BranchHead;
// Renamed because the project's `Git` is imported too; `GitCli` implements
// both, each with its own methods.
use crate::application::memory::ports::Git as MemoryGit;
use crate::application::project::domain::RepositoryUrl;
use crate::application::project::ports::Git;

const GIT_DOCS: &str = "https://git-scm.com/downloads";

/// git, called through the `git` binary on the `PATH`.
pub struct GitCli;

impl GitDiagnostics for GitCli {
    fn version(&self) -> Result<String> {
        let output = Command::new("git")
            .arg("--version")
            .output()
            .context("cannot run git")?;
        if !output.status.success() {
            bail!("git --version failed");
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn install_fixes(&self, os: HostOs) -> Vec<Fix> {
        install_fixes(os)
    }
}

impl Git for GitCli {
    fn clone_repository(&self, url: &RepositoryUrl, destination: &Path) -> Result<()> {
        // stdin and stderr stay attached so git can show progress and ask for
        // credentials. `--` stops git from reading the URL as an option.
        let status = Command::new("git")
            .args(["clone", "--"])
            .arg(url.as_str())
            .arg(destination)
            .status()
            .context("cannot run git; install it or run `casulo doctor` to check")?;
        if !status.success() {
            bail!("git clone failed; check the URL and that you can clone it with git");
        }
        Ok(())
    }

    fn init(&self, directory: &Path) -> Result<()> {
        let output = git_in(directory, &["init", "--quiet"])?;
        if !output.status.success() {
            bail!(
                "git init failed in {}: {}",
                directory.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    fn remotes(&self, directory: &Path) -> Result<Vec<String>> {
        let output = git_in(directory, &["remote"])?;
        if !output.status.success() {
            bail!("git remote failed in {}", directory.display());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    fn remote_url(&self, directory: &Path, remote: &str) -> Result<Option<String>> {
        // Read from the config, not `git remote get-url`, so a remote name
        // can never be taken for an option.
        let key = format!("remote.{remote}.url");
        let output = git_in(directory, &["config", "--get", &key])?;
        Ok(first_line_if_success(&output))
    }

    fn current_branch(&self, directory: &Path) -> Result<Option<String>> {
        // `symbolic-ref` also works before the first commit, unlike `rev-parse`.
        let output = git_in(directory, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        Ok(first_line_if_success(&output))
    }

    fn upstream(&self, directory: &Path) -> Result<Option<String>> {
        let output = git_in(
            directory,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )?;
        Ok(first_line_if_success(&output))
    }
}

/// Runs a local git command in `directory` and captures its output. Only
/// local commands go through here, so no credential prompt can appear.
fn git_in(directory: &Path, arguments: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .context("cannot run git; install it or run `casulo doctor` to check")
}

fn first_line_if_success(output: &std::process::Output) -> Option<String> {
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or_default().trim();
    (output.status.success() && !line.is_empty()).then(|| line.to_string())
}

fn install_fixes(os: HostOs) -> Vec<Fix> {
    match os {
        HostOs::MacOs => vec![
            Fix::commands("install git with Homebrew", &["brew install git"], GIT_DOCS),
            Fix::commands(
                "install Apple's command line tools, which include git",
                &["xcode-select --install"],
                GIT_DOCS,
            ),
        ],
        HostOs::Windows => vec![Fix::commands(
            "install Git for Windows with winget",
            &["winget install --exact --id Git.Git"],
            GIT_DOCS,
        )],
        HostOs::Linux(LinuxDistro::Debian | LinuxDistro::Ubuntu) => vec![Fix::commands(
            "install git with apt",
            &["sudo apt update", "sudo apt install git"],
            GIT_DOCS,
        )],
        HostOs::Linux(LinuxDistro::Fedora) => vec![Fix::commands(
            "install git with dnf",
            &["sudo dnf install git"],
            GIT_DOCS,
        )],
        HostOs::Linux(LinuxDistro::Other) | HostOs::Other => vec![Fix::manual(
            "install git with your system's package manager",
            GIT_DOCS,
        )],
    }
}

/// File modes git uses for a symbolic link and for a submodule. Neither is
/// read: a link could point outside the repository.
const SYMLINK_MODE: &str = "120000";
const SUBMODULE_MODE: &str = "160000";

impl MemoryGit for GitCli {
    fn ls_files(&self, directory: &Path) -> Result<Vec<(RepoPath, String)>> {
        // Staged hashes are right for files that did not change on disk.
        let mut files = BTreeMap::new();
        for entry in split_nul(&git(directory, &["ls-files", "--stage", "-z"])?) {
            // "<mode> <hash> <stage>\t<path>"
            let Some((info, path)) = entry.split_once('\t') else {
                continue;
            };
            let mut fields = info.split(' ');
            let (Some(mode), Some(hash)) = (fields.next(), fields.next()) else {
                continue;
            };
            if mode == SYMLINK_MODE || mode == SUBMODULE_MODE {
                continue;
            }
            if let Ok(path) = RepoPath::parse(path) {
                files.insert(path, hash.to_string());
            }
        }
        for deleted in split_nul(&git(directory, &["ls-files", "--deleted", "-z"])?) {
            if let Ok(path) = RepoPath::parse(&deleted) {
                files.remove(&path);
            }
        }
        // Modified and untracked files are hashed from disk, with the same
        // filters (such as line endings) git would apply.
        let mut to_hash = Vec::new();
        for path in split_nul(&git(directory, &["ls-files", "--modified", "-z"])?)
            .into_iter()
            .chain(split_nul(&git(
                directory,
                &["ls-files", "--others", "--exclude-standard", "-z"],
            )?))
        {
            let Ok(path) = RepoPath::parse(&path) else {
                continue;
            };
            let on_disk = path
                .as_str()
                .split('/')
                .fold(directory.to_path_buf(), |path, part| path.join(part));
            let is_regular_file = std::fs::symlink_metadata(&on_disk)
                .is_ok_and(|metadata| metadata.file_type().is_file());
            if is_regular_file && !path.as_str().contains('\n') {
                to_hash.push(path);
            } else {
                files.remove(&path);
            }
        }
        for (path, hash) in to_hash.iter().zip(hash_objects(directory, &to_hash)?) {
            files.insert(path.clone(), hash);
        }
        Ok(files.into_iter().collect())
    }

    fn hash_file(&self, directory: &Path, path: &RepoPath) -> Result<String> {
        let output = git(directory, &["hash-object", "--", path.as_str()])?;
        Ok(output.trim().to_string())
    }

    fn current_branch(&self, directory: &Path) -> Result<Option<String>> {
        let output = git(directory, &["branch", "--show-current"])?;
        let branch = output.trim();
        Ok((!branch.is_empty()).then(|| branch.to_string()))
    }

    fn local_branches(&self, directory: &Path) -> Result<Vec<BranchHead>> {
        let output = git(
            directory,
            &[
                "for-each-ref",
                "--format=%(refname:short)%09%(objectname)",
                "refs/heads",
            ],
        )?;
        Ok(output
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(branch, head)| BranchHead {
                branch: branch.to_string(),
                head_commit: head.to_string(),
            })
            .collect())
    }

    fn ls_tree(&self, directory: &Path, commit: &str) -> Result<Vec<(RepoPath, String)>> {
        let mut files = Vec::new();
        for entry in split_nul(&git(directory, &["ls-tree", "-r", "-z", commit])?) {
            // "<mode> <type> <hash>\t<path>"
            let Some((info, path)) = entry.split_once('\t') else {
                continue;
            };
            let fields: Vec<&str> = info.split(' ').collect();
            if fields.len() != 3 || fields[1] != "blob" || fields[0] == SYMLINK_MODE {
                continue;
            }
            if let Ok(path) = RepoPath::parse(path) {
                files.push((path, fields[2].to_string()));
            }
        }
        Ok(files)
    }
}

/// Runs a local git command in `directory` and returns its output.
fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .context("cannot run git; is it installed?")?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            arguments.first().unwrap_or(&""),
            directory.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git printed invalid UTF-8")
}

/// Hashes many files with one `git hash-object --stdin-paths`.
fn hash_objects(directory: &Path, paths: &[RepoPath]) -> Result<Vec<String>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["hash-object", "--stdin-paths"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git; is it installed?")?;
    let mut input = String::new();
    for path in paths {
        input.push_str(path.as_str());
        input.push('\n');
    }
    child
        .stdin
        .take()
        .context("cannot write to git hash-object")?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "git hash-object failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let hashes: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(str::to_string)
        .collect();
    if hashes.len() != paths.len() {
        bail!(
            "git hash-object returned {} hashes for {} files",
            hashes.len(),
            paths.len()
        );
    }
    Ok(hashes)
}

fn split_nul(text: &str) -> Vec<String> {
    text.split('\0')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_system_gets_its_package_manager() {
        assert_eq!(
            install_fixes(HostOs::MacOs)[0].commands,
            ["brew install git"]
        );
        assert_eq!(
            install_fixes(HostOs::Windows)[0].commands,
            ["winget install --exact --id Git.Git"]
        );
        assert_eq!(
            install_fixes(HostOs::Linux(LinuxDistro::Debian))[0].commands,
            ["sudo apt update", "sudo apt install git"]
        );
        assert_eq!(
            install_fixes(HostOs::Linux(LinuxDistro::Fedora))[0].commands,
            ["sudo dnf install git"]
        );
    }

    #[test]
    fn unknown_systems_only_link_the_docs() {
        let fixes = install_fixes(HostOs::Linux(LinuxDistro::Other));

        assert!(fixes[0].commands.is_empty());
        assert_eq!(fixes[0].docs_url, GIT_DOCS);
    }
}
