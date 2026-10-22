//! The git rules of this repository: commit messages, pull request titles
//! and branch names (AGENTS.md, *Commits and PRs*).

pub mod branch_name;
pub mod commit_message;

use std::path::Path;
use std::process::Command;

/// Whether a merge is in progress, so the commit being made is that merge.
/// git keeps `MERGE_HEAD` until the merge is committed; `--git-path` finds
/// it in a worktree too. Outside a repository, nothing is merging.
pub fn merging() -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "MERGE_HEAD"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout);
            Path::new(path.trim()).exists()
        }
        _ => false,
    }
}
