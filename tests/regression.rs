//! Regression snapshots: the whole output of the real binary (exit code,
//! stdout and stderr) and the files it writes, for what casulo does today.
//!
//! A change that only moves code must leave every snapshot here as it is.
//! A snapshot that changes means behavior changed. Machine-specific parts
//! are replaced before the snapshot is taken, so it is the same on every
//! machine: `[HOME]` is the casulo home, `[OUTSIDE]` a directory outside it,
//! `[PID]` the process id, and `[UID]`/`[GID]` the workspace owner.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

// `CommandFactory` gives the clap definition of the CLI, so the help test
// finds every command without keeping a list by hand.
use clap::CommandFactory;

/// A temporary casulo home, and a directory outside it for projects created
/// from a directory and for the fake user home.
struct Sandbox {
    home: tempfile::TempDir,
    outside: tempfile::TempDir,
}

impl Sandbox {
    /// A casulo home whose runtime does not exist, so nothing ever starts a
    /// container, and with no projects.
    fn new() -> Self {
        let sandbox = Self {
            home: tempfile::tempdir().expect("create the casulo home"),
            outside: tempfile::tempdir().expect("create the outside directory"),
        };
        sandbox.write(
            "config/casulo.toml",
            "runtime = \"casulo-test-missing-runtime\"\n",
        );
        std::fs::create_dir_all(sandbox.outside.path().join("user")).expect("create the user home");
        sandbox
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn outside(&self) -> &Path {
        self.outside.path()
    }

    /// Writes `text` to `relative` under the casulo home, creating folders.
    fn write(&self, relative: &str, text: &str) {
        write_file(&under(self.home(), relative), text);
    }

    /// Creates the project `demo` with `config` as its `casulo.toml` and
    /// an empty workspace, and returns the workspace.
    fn project(&self, config: &str) -> PathBuf {
        let workspace = under(self.home(), "projects/demo/workspace");
        std::fs::create_dir_all(&workspace).expect("create the workspace");
        self.write("projects/demo/casulo.toml", config);
        workspace
    }

    /// Runs casulo with `arguments` in the casulo home.
    fn run(&self, arguments: &[&str]) -> String {
        self.run_in(self.home(), arguments, &[])
    }

    /// Runs each command line of `commands`, split on spaces, in the casulo
    /// home, one after the other, and shows them as a terminal session.
    fn session(&self, commands: &[&str]) -> String {
        commands
            .iter()
            .map(|command| {
                let arguments: Vec<&str> = command.split(' ').collect();
                format!("$ casulo {command}\n{}", self.run(&arguments))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Runs casulo with `arguments` in `directory`, with `environment` added
    /// to a clean environment, and renders the whole result.
    fn run_in(&self, directory: &Path, arguments: &[&str], environment: &[(&str, &str)]) -> String {
        let (text, _pid) = self.execute(directory, arguments, environment, "");
        text
    }

    /// Like `run_in`, with `input` typed on stdin, and also returns the
    /// process id, for normalizing what the process wrote elsewhere.
    fn execute(
        &self,
        directory: &Path,
        arguments: &[&str],
        environment: &[(&str, &str)],
        input: &str,
    ) -> (String, u32) {
        // `std::process::Command` instead of `assert_cmd`, because the
        // process id is only known after `spawn`, and it is in the output.
        let mut command = Command::new(env!("CARGO_BIN_EXE_casulo"));
        command
            .args(arguments)
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Nothing from the developer's shell may change the output:
            // no secrets, no editor and no locale.
            .env_clear()
            .envs(passed_through_environment())
            .env("CASULO_HOME", self.home())
            .env("HOME", self.outside().join("user"))
            .env("USERPROFILE", self.outside().join("user"))
            // If a bug made casulo fall back to the real docker, it must
            // find no daemon instead of building and starting containers.
            .env("DOCKER_HOST", "unix:///nonexistent/casulo-test.sock")
            .env("CONTAINER_HOST", "unix:///nonexistent/casulo-test.sock");
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("start casulo");
        let pid = child.id();
        // `take` moves stdin out of the child; dropping it at the end of
        // this statement closes it, so casulo sees the end of the input.
        std::io::Write::write_all(
            &mut child.stdin.take().expect("stdin is piped"),
            input.as_bytes(),
        )
        .expect("write to stdin");
        let output = child.wait_with_output().expect("wait for casulo");
        (self.normalize(&render(&output), Some(pid)), pid)
    }

    /// Replaces what differs between machines and runs with placeholders.
    /// `pid` is the casulo process that wrote `text`, if any.
    fn normalize(&self, text: &str, pid: Option<u32>) -> String {
        let mut text = text.to_string();
        for (directory, placeholder) in [(self.home(), "[HOME]"), (self.outside(), "[OUTSIDE]")] {
            for form in path_forms(directory) {
                text = text.replace(&form, placeholder);
            }
        }
        if cfg!(windows) {
            // This also hides a path built with a hardcoded `/` on Windows;
            // only review catches that one.
            text = text.replace('\\', "/").replace("casulo.exe", "casulo");
        }
        // The agent container is named `casulo-<project>-<harness>-<pid>`,
        // and the pid names its memory session.
        if let Some(pid) = pid {
            for harness in ["claude", "codex", "plain"] {
                text = text.replace(&format!("-{harness}-{pid}"), &format!("-{harness}-[PID]"));
            }
            text = text.replace(&format!("CASULO_SESSION={pid}"), "CASULO_SESSION=[PID]");
        }
        text
    }
}

/// Environment variables a process needs to start at all on each system.
fn passed_through_environment() -> Vec<(String, String)> {
    [
        "PATH",
        "TMPDIR",
        "SYSTEMROOT",
        "WINDIR",
        "TEMP",
        "TMP",
        "PATHEXT",
        "COMSPEC",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| (name.to_string(), value))
    })
    .collect()
}

/// The ways a path can show up in the output: as given, and resolved, since
/// macOS puts temporary directories behind a symlink and Windows adds `\\?\`.
fn path_forms(directory: &Path) -> Vec<String> {
    let mut forms = Vec::new();
    if let Ok(resolved) = directory.canonicalize() {
        let resolved = resolved.display().to_string();
        forms.push(resolved.clone());
        if let Some(plain) = resolved.strip_prefix(r"\\?\") {
            forms.push(plain.to_string());
        }
    }
    forms.push(directory.display().to_string());
    forms
}

/// `relative`, written with `/`, as a path under `base` on this system.
fn under(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// Writes `text` to `file`, creating folders. On Unix the file is readable
/// by everyone whatever the umask, so casulo's warning about secrets in a
/// readable config shows on every machine.
fn write_file(file: &Path, text: &str) {
    std::fs::create_dir_all(file.parent().expect("files have a parent")).expect("create folders");
    std::fs::write(file, text).expect("write the file");
    #[cfg(unix)]
    {
        // `PermissionsExt` sets Unix permission bits; it only exists on Unix.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644))
            .expect("set the permissions");
    }
}

/// The whole result of a run, as the snapshot shows it.
fn render(output: &Output) -> String {
    let code = output
        .status
        .code()
        .map_or("killed by a signal".to_string(), |code| code.to_string());
    format!(
        "exit code: {code}\n--- stdout\n{}--- stderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Every file under `directory`, sorted, with its path relative to it and
/// its contents, skipping the folders in `skip`. Only the modules that do
/// not run on Windows use it, so it is not built there.
#[cfg(not(windows))]
fn files_under(directory: &Path, skip: &[&str]) -> String {
    let mut files = Vec::new();
    collect_files(directory, directory, skip, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|(path, contents)| format!("=== {path}\n{contents}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(not(windows))]
fn collect_files(root: &Path, directory: &Path, skip: &[&str], files: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .expect("read the directory")
        .map(|entry| entry.expect("read the entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let relative = path
            .strip_prefix(root)
            .expect("the entry is under the root")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if skip.contains(&relative.as_str()) {
            continue;
        }
        if path.is_dir() {
            files.push((format!("{relative}/"), String::new()));
            collect_files(root, &path, skip, files);
        } else {
            let bytes = std::fs::read(&path).expect("read the file");
            let contents = String::from_utf8(bytes.clone())
                .unwrap_or_else(|_| format!("({} bytes, not text)\n", bytes.len()));
            files.push((relative, contents));
        }
    }
}

/// Writes an instructions file, a subagent, a skill and two MCP servers to
/// the user's agent setup.
fn write_agent_setup(sandbox: &Sandbox) {
    sandbox.write("agent/AGENTS.md", "Answer in English.\n");
    sandbox.write(
        "agent/agents/reviewer.md",
        "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems.\n",
    );
    sandbox.write(
        "agent/skills/tdd/SKILL.md",
        "---\nname: tdd\ndescription: Test first\n---\nWrite the test first.\n",
    );
    sandbox.write(
        "agent/mcp/context7.toml",
        "command = [\"npx\", \"-y\", \"@upstash/context7-mcp\"]\nenv = [\"CONTEXT7_TOKEN\"]\n",
    );
    sandbox.write(
        "agent/mcp/github.toml",
        "url = \"https://api.githubcopilot.com/mcp/\"\ntoken_env = \"GITHUB_TOKEN\"\n",
    );
}

/// A harness with no MCP support and nothing injected.
const PLAIN_HARNESS: &str =
    "name = \"plain\"\ninstall = [\"npm install -g plain-agent\"]\ncommand = [\"plain\"]\n";

const REPOSITORY: &str = "repository = \"https://example.com/demo.git\"\n";

// ---------------------------------------------------------------------------
// The CLI's interface

/// The names of every command and subcommand under `command`, as the
/// arguments that reach it, `help` excepted.
fn command_paths(command: &clap::Command, prefix: &[String]) -> Vec<Vec<String>> {
    let mut paths = vec![prefix.to_vec()];
    for subcommand in command.get_subcommands() {
        if subcommand.get_name() == "help" {
            continue;
        }
        let mut path = prefix.to_vec();
        path.push(subcommand.get_name().to_string());
        paths.extend(command_paths(subcommand, &path));
    }
    paths
}

#[test]
fn help_of_every_command() {
    let sandbox = Sandbox::new();

    for path in command_paths(&casulo::cli::Cli::command(), &[]) {
        let mut arguments: Vec<&str> = path.iter().map(String::as_str).collect();
        arguments.push("--help");
        let name = std::iter::once("help")
            .chain(path.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("_");

        insta::assert_snapshot!(name, sandbox.run(&arguments));
    }
}

#[test]
fn version() {
    let sandbox = Sandbox::new();

    insta::assert_snapshot!(sandbox.run(&["--version"]));
}

// ---------------------------------------------------------------------------
// What `run` and `shell` would start. Linux only: the workspace owner is
// passed to the container there, and paths and quoting differ elsewhere.

#[cfg(target_os = "linux")]
mod dry_run {
    use super::*;

    /// `run --dry-run` and `shell --dry-run` in the workspace, one after
    /// the other, with the workspace owner replaced.
    fn both(sandbox: &Sandbox, workspace: &Path, environment: &[(&str, &str)]) -> String {
        let run = sandbox.run_in(workspace, &["run", "--dry-run"], environment);
        let shell = sandbox.run_in(workspace, &["shell", "--dry-run"], environment);
        owner_replaced(
            &format!("$ casulo run --dry-run\n{run}\n$ casulo shell --dry-run\n{shell}"),
            workspace,
        )
    }

    /// Replaces the workspace owner's ids, which differ between machines.
    pub fn owner_replaced(text: &str, workspace: &Path) -> String {
        // `MetadataExt` gives the Unix owner of a file; it only exists on Unix.
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(workspace).expect("read the workspace");
        let (uid, gid) = (metadata.uid(), metadata.gid());
        // casulo never passes uid 0 to the container, so as root the output
        // has no ids to replace and every snapshot here would differ.
        assert_ne!(uid, 0, "run the regression tests as a non-root user");
        text.replace(&format!("--user={uid}:{gid}"), "--user=[UID]:[GID]")
            .replace(&format!("uid={uid},gid={gid}"), "uid=[UID],gid=[GID]")
    }

    #[test]
    fn plain_project() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(REPOSITORY);

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn secrets_from_the_config_and_the_environment_are_masked() {
        let sandbox = Sandbox::new();
        sandbox.write(
            "config/casulo.toml",
            "runtime = \"casulo-test-missing-runtime\"\n[secrets]\nCLAUDE_CODE_OAUTH_TOKEN = \"sk-ant-oat01-config-value\"\n",
        );
        let workspace = sandbox.project(REPOSITORY);

        let output = both(
            &sandbox,
            &workspace,
            &[("ANTHROPIC_API_KEY", "sk-test-environment-value")],
        );

        assert!(!output.contains("config-value"), "{output}");
        assert!(!output.contains("environment-value"), "{output}");
        insta::assert_snapshot!(output);
    }

    #[test]
    fn push_on_without_a_token() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}[git]\nname = \"Ada\"\nemail = \"ada@example.com\"\npush = true\n"
        ));

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn push_on_with_a_token() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}[git]\nname = \"Ada\"\nemail = \"ada@example.com\"\npush = true\ntoken = \"github_pat_config-value\"\nprotected_branches = [\"main\", \"release/*\"]\n"
        ));

        let output = both(&sandbox, &workspace, &[]);

        assert!(!output.contains("config-value"), "{output}");
        insta::assert_snapshot!(output);
    }

    #[test]
    fn memory_on() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(&format!("{REPOSITORY}[memory]\nenabled = true\n"));

        let output = both(
            &sandbox,
            &workspace,
            &[("ANTHROPIC_API_KEY", "sk-test-environment-value")],
        );

        assert!(!output.contains("environment-value"), "{output}");
        insta::assert_snapshot!(output);
    }

    #[test]
    fn memory_on_with_a_harness_without_mcp() {
        let sandbox = Sandbox::new();
        sandbox.write("harnesses/plain.toml", PLAIN_HARNESS);
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}harness = \"plain\"\n[memory]\nenabled = true\n"
        ));

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn masks_cover_existing_paths_only() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}mask = [\".env\", \".claude/agents\", \"missing.pem\"]\n"
        ));
        write_file(&workspace.join(".env"), "TOKEN=secret\n");
        std::fs::create_dir_all(workspace.join(".claude").join("agents"))
            .expect("create the folder");

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn image_install_commands() {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}[image]\ninstall = [\"apt-get update && apt-get install --yes make\"]\n"
        ));

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn agent_setup_for_claude() {
        let sandbox = Sandbox::new();
        write_agent_setup(&sandbox);
        sandbox.write(
            "config/casulo.toml",
            "runtime = \"casulo-test-missing-runtime\"\n[harness.claude]\nmodel = \"opus\"\neffort = \"high\"\n",
        );
        let workspace = sandbox.project(REPOSITORY);

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }

    #[test]
    fn agent_setup_for_codex() {
        let sandbox = Sandbox::new();
        write_agent_setup(&sandbox);
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}harness = \"codex\"\n[models.codex]\nmodel = \"gpt-5\"\n"
        ));

        insta::assert_snapshot!(both(&sandbox, &workspace, &[]));
    }
}

// ---------------------------------------------------------------------------
// A real `casulo run` against a fake runtime: the files casulo writes, every
// runtime call with its arguments, and which secrets reach the runtime.
// Linux only, for the fake runtime script and the workspace owner.

#[cfg(target_os = "linux")]
mod real_run {
    use super::*;

    /// The normalized model `docker compose config` prints for the project
    /// `demo` with one redis service.
    const COMPOSE_MODEL: &str = r#"{"name":"casulo-demo","networks":{"default":{"name":"casulo-demo_default","ipam":{}}},"services":{"cache":{"image":"redis:7","networks":{"default":null}}}}"#;

    /// Writes a runtime that logs each call to `runtime.log`, with the names
    /// (never the values) of the secrets in its environment. It has every
    /// image, answers `compose config` with `compose-model.json` (written as
    /// `COMPOSE_MODEL`, and changed by tests that need another), and stops the agent at
    /// `start`, so nothing ever runs.
    fn fake_runtime(sandbox: &Sandbox) -> PathBuf {
        // `PermissionsExt` sets the Unix executable bit; it only exists on Unix.
        use std::os::unix::fs::PermissionsExt;
        let runtime = sandbox.outside().join("fake-runtime");
        let log = sandbox.outside().join("runtime.log");
        let model = sandbox.outside().join("compose-model.json");
        write_file(&model, COMPOSE_MODEL);
        let script = format!(
            r#"#!/bin/sh
secrets=$(env | cut -d= -f1 | grep -x -e CASULO_GIT_TOKEN -e CONTEXT7_TOKEN -e GITHUB_TOKEN -e ANTHROPIC_API_KEY -e CLAUDE_CODE_OAUTH_TOKEN -e OPENAI_API_KEY | sort | tr '\n' ' ')
echo "$* [secrets: $secrets]" >> '{log}'
case "$1 $2" in
    "image inspect") echo sha256:fake; exit 0 ;;
    "container inspect") echo "Error: No such container: $5" >&2; exit 1 ;;
    "ps --filter") echo casulo-memory-demo; exit 0 ;;
    "start --attach") echo "fake runtime: the agent would start here" >&2; exit 1 ;;
esac
case "$*" in
    *" config --format json") cat '{model}'; exit 0 ;;
    *" up --detach --wait") cat > /dev/null; exit 0 ;;
esac
case "$1" in
    network|run|create|stop|rm) exit 0 ;;
esac
echo "fake runtime: unexpected call" >&2
exit 1
"#,
            log = log.display(),
            model = model.display()
        );
        write_file(&runtime, &script);
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755))
            .expect("make the runtime executable");
        runtime
    }

    /// A sandbox whose global config uses the fake runtime, with `extra`
    /// appended to that config.
    fn sandbox_with_fake_runtime(extra: &str) -> Sandbox {
        let sandbox = Sandbox::new();
        let runtime = fake_runtime(&sandbox);
        sandbox.write(
            "config/casulo.toml",
            &format!("runtime = \"{}\"\n{extra}", runtime.display()),
        );
        sandbox
    }

    /// Runs casulo in `workspace` with `input` on stdin and shows the
    /// output and the runtime calls it made, then empties the log.
    fn run_and_log(sandbox: &Sandbox, workspace: &Path, arguments: &[&str], input: &str) -> String {
        let (output, pid) = sandbox.execute(workspace, arguments, &[], input);
        let log_file = sandbox.outside().join("runtime.log");
        let log = std::fs::read_to_string(&log_file).unwrap_or_default();
        std::fs::remove_file(&log_file).ok();
        let text = format!(
            "$ casulo {}\n{output}--- runtime calls\n{}",
            arguments.join(" "),
            sandbox.normalize(&log, Some(pid))
        );
        dry_run::owner_replaced(&text, workspace)
    }

    /// Runs `casulo run` with the whole agent setup, push and memory on,
    /// for `harness`, and shows the output, the runtime calls and every
    /// file of the project except the config the test wrote.
    fn a_run(harness: &str) -> String {
        let sandbox = sandbox_with_fake_runtime("[secrets]\nCONTEXT7_TOKEN = \"config-value\"\n");
        write_agent_setup(&sandbox);
        let workspace = sandbox.project(&format!(
            "{REPOSITORY}harness = \"{harness}\"\nmask = [\".env\"]\n[memory]\nenabled = true\n[git]\nname = \"Ada\"\nemail = \"ada@example.com\"\npush = true\ntoken = \"github_pat_config-value\"\n"
        ));
        write_file(&workspace.join(".env"), "TOKEN=secret\n");

        let output = run_and_log(&sandbox, &workspace, &["run"], "");
        let files = sandbox.normalize(
            &files_under(&under(sandbox.home(), "projects/demo"), &["casulo.toml"]),
            None,
        );

        // Only the two config files the test wrote may hold a secret value.
        let everything = files_under(
            sandbox.home(),
            &["config/casulo.toml", "projects/demo/casulo.toml"],
        );
        assert!(!everything.contains("config-value"), "{everything}");
        let text = dry_run::owner_replaced(&format!("{output}\n{files}"), &workspace);
        assert!(!text.contains("config-value"), "{text}");
        text
    }

    #[test]
    fn a_claude_run() {
        insta::assert_snapshot!(a_run("claude"));
    }

    #[test]
    fn a_codex_run() {
        insta::assert_snapshot!(a_run("codex"));
    }

    /// A project with compose on and a compose file in its workspace.
    fn project_with_compose(sandbox: &Sandbox) -> PathBuf {
        let workspace = sandbox.project(&format!("{REPOSITORY}[compose]\nenabled = true\n"));
        write_file(
            &workspace.join("docker-compose.yml"),
            "services:\n  cache:\n    image: redis:7\n",
        );
        workspace
    }

    #[test]
    fn compose_check_and_dry_run() {
        let sandbox = sandbox_with_fake_runtime("");
        let workspace = project_with_compose(&sandbox);

        let check = run_and_log(&sandbox, &workspace, &["compose", "check"], "");
        let dry_run = run_and_log(&sandbox, &workspace, &["run", "--dry-run"], "");

        insta::assert_snapshot!(format!("{check}\n{dry_run}"));
    }

    #[test]
    fn compose_services_need_approval() {
        let sandbox = sandbox_with_fake_runtime("");
        let workspace = project_with_compose(&sandbox);

        let refused = run_and_log(&sandbox, &workspace, &["run"], "");
        let approved = run_and_log(&sandbox, &workspace, &["run"], "y\n");
        let up_to_date = run_and_log(&sandbox, &workspace, &["run"], "");

        assert!(!refused.contains(" up --detach"), "{refused}");
        insta::assert_snapshot!(format!("{refused}\n{approved}\n{up_to_date}"));
    }

    #[test]
    fn compose_services_that_could_reach_the_host_are_rejected() {
        let sandbox = sandbox_with_fake_runtime("");
        let workspace = project_with_compose(&sandbox);
        write_file(
            &sandbox.outside().join("compose-model.json"),
            r#"{"name":"casulo-demo","services":{"cache":{"image":"redis:7","privileged":true}}}"#,
        );

        let check = run_and_log(&sandbox, &workspace, &["compose", "check"], "");
        let run = run_and_log(&sandbox, &workspace, &["run"], "y\n");

        assert!(!run.contains(" up --detach"), "{run}");
        insta::assert_snapshot!(format!("{check}\n{run}"));
    }
}

// ---------------------------------------------------------------------------
// The Dockerfile `casulo build` writes before calling the runtime. Not on
// Windows, whose text for a missing program differs.

#[cfg(not(windows))]
mod build {
    use super::*;

    /// Runs `casulo build` for the project `demo` with `config` and shows
    /// the output and every file it wrote under `build/`.
    fn build_output(config: &str) -> String {
        let sandbox = Sandbox::new();
        let workspace = sandbox.project(config);

        let output = sandbox.run_in(&workspace, &["build"], &[]);
        let files = files_under(&under(sandbox.home(), "projects/demo/build"), &[]);
        format!("{output}\n{files}")
    }

    #[test]
    fn for_claude() {
        insta::assert_snapshot!(build_output(REPOSITORY));
    }

    #[test]
    fn for_codex() {
        insta::assert_snapshot!(build_output(&format!("{REPOSITORY}harness = \"codex\"\n")));
    }

    #[test]
    fn with_image_install_commands() {
        insta::assert_snapshot!(build_output(&format!(
            "{REPOSITORY}[image]\ninstall = [\"apt-get update && apt-get install --yes make\"]\n"
        )));
    }
}

// ---------------------------------------------------------------------------
// Lists of harnesses and of the user's agent setup

#[test]
fn harness_list_with_built_ins_and_overrides() {
    let sandbox = Sandbox::new();
    sandbox.write("harnesses/plain.toml", PLAIN_HARNESS);
    sandbox.write(
        "harnesses/codex.toml",
        "name = \"codex\"\ninstall = [\"npm install -g @openai/codex@latest\"]\ncommand = [\"codex\"]\n",
    );

    insta::assert_snapshot!(sandbox.run(&["harness", "list"]));
}

#[test]
fn lists_of_the_agent_setup() {
    let sandbox = Sandbox::new();
    write_agent_setup(&sandbox);

    insta::assert_snapshot!(sandbox.session(&["list", "list agent", "list skill", "list mcp"]));
}

// ---------------------------------------------------------------------------
// The project's memory, as the user sees it

/// Creates the project `app` from a directory with a small Rust crate,
/// indexes its memory, and adds one stale note on `src/main.rs`.
fn project_with_memory(sandbox: &Sandbox) -> String {
    use casulo::application::memory::domain::event::{AcceptedNote, Event};
    use casulo::application::memory::domain::{
        Author, NoteBody, NoteKind, NoteSummary, NoteTarget, RepoPath,
    };
    use casulo::application::memory::ports::MemoryStore;
    use casulo::infrastructure::sqlite::Database;

    let directory = sandbox.outside().join("app");
    write_file(
        &under(&directory, "Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    );
    write_file(
        &under(&directory, "src/main.rs"),
        "mod cli;\nuse app::domain::Config;\nfn main() {}\n",
    );
    write_file(&under(&directory, "src/cli.rs"), "pub fn run() {}\n");
    write_file(&under(&directory, "src/lib.rs"), "pub mod domain;\n");
    write_file(
        &under(&directory, "src/domain/mod.rs"),
        "pub struct Config;\n",
    );
    let directory_argument = directory.display().to_string();
    let created = sandbox.run(&["new", &directory_argument]);
    let indexed = sandbox.run(&["index", "memory", "--project", "app"]);

    let database_file = under(sandbox.home(), "projects/app/memory/memory.sqlite");
    let mut database = Database::open(&database_file).expect("open the memory");
    database
        .apply(
            &Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::File {
                        path: RepoPath::parse("src/main.rs").expect("valid path"),
                        code_hash: "an-older-version".to_string(),
                    },
                    kind: NoteKind::Purpose,
                    summary: NoteSummary::parse("Starts the program").expect("valid summary"),
                    body: NoteBody::parse("").expect("valid body"),
                    refs: Vec::new(),
                    author: Author::Harness("claude".to_string()),
                },
            },
            "2026-10-10T12:00:00Z",
        )
        .expect("apply the note");
    format!("$ casulo new [OUTSIDE]/app\n{created}\n$ casulo index memory --project app\n{indexed}")
}

#[test]
fn memory_seen_by_the_user() {
    let sandbox = Sandbox::new();
    let setup = project_with_memory(&sandbox);

    let output = sandbox.session(&[
        "describe memory --project app",
        "describe memory note src/main.rs --project app",
        "list memory note --project app",
        "list memory note --stale --project app",
        "list memory topic --project app",
        "search memory main --project app",
    ]);

    insta::assert_snapshot!(format!("{setup}\n{output}"));
}

#[test]
fn memory_of_a_project_without_memory() {
    let sandbox = Sandbox::new();
    sandbox.project(REPOSITORY);

    insta::assert_snapshot!(sandbox.run(&["describe", "memory", "--project", "demo"]));
}

// ---------------------------------------------------------------------------
// The main error messages

#[test]
fn invalid_global_config() {
    let sandbox = Sandbox::new();
    sandbox.write("config/casulo.toml", "runtime = \n");
    let workspace = sandbox.project(REPOSITORY);

    insta::assert_snapshot!(sandbox.run_in(&workspace, &["run", "--dry-run"], &[]));
}

#[test]
fn invalid_project_config() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.project(&format!("{REPOSITORY}harnes = \"codex\"\n"));

    insta::assert_snapshot!(sandbox.run_in(&workspace, &["run", "--dry-run"], &[]));
}

#[test]
fn unknown_project() {
    let sandbox = Sandbox::new();

    insta::assert_snapshot!(sandbox.run(&["run", "--dry-run", "--project", "missing"]));
}

#[test]
fn outside_any_project() {
    let sandbox = Sandbox::new();

    insta::assert_snapshot!(sandbox.run_in(sandbox.outside(), &["run", "--dry-run"], &[]));
}

#[test]
fn invalid_names() {
    let sandbox = Sandbox::new();

    let output = sandbox.session(&[
        "add skill Code--Reviewer",
        "run --dry-run --project Bad_Name",
        "new https://example.com/demo.git --name Bad_Name",
        "run --dry-run --harness Bad_Name",
    ]);

    insta::assert_snapshot!(output);
}

#[test]
fn unknown_command() {
    let sandbox = Sandbox::new();

    insta::assert_snapshot!(sandbox.run(&["not-a-command"]));
}
