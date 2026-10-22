//! Checks for casulo's own repository, run as `cargo xtask <task>`.
//!
//! `xtask` is a Rust convention: the project's tooling is a small binary
//! crate of its workspace, run through the `cargo xtask` alias in
//! `.cargo/config.toml`, instead of shell scripts. It is not part of casulo
//! and is never published or installed.

mod git;

use std::path::Path;
use std::process::ExitCode;

use git::{branch_name, commit_message};

/// What to type, printed when the task is missing or unknown.
const USAGE: &str =
    "usage: cargo xtask commit-msg <file> | pr-title <title> | branch-name [<branch>]";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    // A slice pattern matches the task and its arguments in one go, and
    // anything else (a missing or extra argument) falls to the usage.
    match arguments.as_slice() {
        ["commit-msg", file] => commit_msg(Path::new(file)),
        ["pr-title", title] => report(&commit_message::line_problems(
            title,
            commit_message::Line::Title,
        )),
        ["branch-name", branch] => branch_name(branch),
        ["branch-name"] => pushed_branch(),
        _ => usage(),
    }
}

/// Checks the branch a push creates or updates, as git's pre-push hook
/// does. prek names it as a full ref in `PRE_COMMIT_REMOTE_BRANCH`: the
/// name on the server, which `git push origin HEAD:<name>` can change. A
/// tag is not a branch, so it passes.
fn pushed_branch() -> ExitCode {
    let reference = std::env::var("PRE_COMMIT_REMOTE_BRANCH").unwrap_or_default();
    match reference.strip_prefix("refs/heads/") {
        Some(branch) => branch_name(branch),
        None => ExitCode::SUCCESS,
    }
}

/// Checks the name `branch`.
fn branch_name(branch: &str) -> ExitCode {
    let problems: Vec<String> = branch_name::branch_problem(branch).into_iter().collect();
    report(&problems)
}

/// Checks the commit message in `file`, as git's commit-msg hook does.
/// A merge is not checked: its subject and content come from git.
fn commit_msg(file: &Path) -> ExitCode {
    if git::merging() {
        return ExitCode::SUCCESS;
    }
    match std::fs::read_to_string(file) {
        Ok(message) => report(&commit_message::message_problems(&message)),
        Err(error) => {
            eprintln!(
                "error: cannot read the commit message in {}: {error}",
                file.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// Prints each problem to stderr; the exit code says whether there was one.
fn report(problems: &[String]) -> ExitCode {
    if problems.is_empty() {
        return ExitCode::SUCCESS;
    }
    for problem in problems {
        eprintln!("error: {problem}");
    }
    eprintln!("the rule is in AGENTS.md, under Conventions");
    ExitCode::FAILURE
}

/// Prints the usage and returns the exit code for a wrong command line.
fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
