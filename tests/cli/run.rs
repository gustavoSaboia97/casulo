//! Functional tests of `casulo run` and `casulo shell`.

use predicates::prelude::*;

use crate::support::{casulo, editor_writing, home_with_missing_runtime, new_from_directory};

/// A casulo home with an existing project "demo" whose workspace is empty.
fn home_with_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let home = home_with_missing_runtime();
    let project_dir = home.path().join("projects").join("demo");
    let workspace = project_dir.join("workspace");
    std::fs::create_dir_all(&workspace).expect("create the workspace");
    std::fs::write(
        project_dir.join("casulo.toml"),
        "repository = \"https://example.com/demo.git\"\n",
    )
    .expect("write the project config");
    (home, workspace)
}

#[test]
fn run_dry_run_mounts_the_directory_at_workspace() {
    let parent = tempfile::tempdir().expect("create a temporary directory");
    let directory = parent.path().join("app");
    std::fs::create_dir_all(&directory).expect("create the directory");
    let home = tempfile::tempdir().expect("create a temporary directory");
    let (workspace, _) = new_from_directory(home.path(), &directory);

    casulo()
        .args(["run", "--dry-run", "--project", "app"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "source={workspace},target=/workspace"
        )));
}

#[test]
fn run_dry_run_prints_the_hardened_command_and_masks_secrets() {
    let (home, workspace) = home_with_project();

    let output = casulo()
        .args(["run", "--dry-run"])
        .env("CASULO_HOME", home.path())
        .env("ANTHROPIC_API_KEY", "sk-test-secret-value")
        .current_dir(&workspace)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let line = String::from_utf8(output).expect("utf-8 output");

    assert!(line.starts_with("ANTHROPIC_API_KEY=*** casulo-test-missing-runtime run --rm"));
    for flag in [
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--read-only",
        "--workdir=/workspace",
        "--env=ANTHROPIC_API_KEY ",
        "casulo/demo:claude claude",
    ] {
        assert!(line.contains(flag), "missing {flag} in {line}");
    }
    assert!(!line.contains("sk-test-secret-value"));
}

#[test]
fn run_dry_run_uses_secrets_from_the_config_but_never_prints_them() {
    let (home, workspace) = home_with_project();
    std::fs::write(
        home.path().join("config").join("casulo.toml"),
        "runtime = \"casulo-test-missing-runtime\"\n[secrets]\nCLAUDE_CODE_OAUTH_TOKEN = \"sk-ant-oat01-config-value\"\n",
    )
    .expect("write the config");

    let output = casulo()
        .args(["run", "--dry-run"])
        .env("CASULO_HOME", home.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
        .current_dir(&workspace)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let line = String::from_utf8(output).expect("utf-8 output");

    assert!(line.starts_with("CLAUDE_CODE_OAUTH_TOKEN=*** casulo-test-missing-runtime run"));
    assert!(line.contains("--env=CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(!line.contains("sk-ant-oat01-config-value"));
}

#[test]
fn run_dry_run_with_memory_starts_the_memory_server_first() {
    let (home, workspace) = home_with_project();
    let project_dir = home.path().join("projects").join("demo");
    std::fs::write(
        project_dir.join("casulo.toml"),
        "repository = \"https://example.com/demo.git\"\n[memory]\nenabled = true\n",
    )
    .expect("write the project config");

    let output = casulo()
        .args(["run", "--dry-run"])
        .env("CASULO_HOME", home.path())
        .current_dir(&workspace)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("utf-8 output");
    let lines: Vec<&str> = stdout.lines().collect();

    assert_eq!(
        lines[0],
        "casulo-test-missing-runtime network create --internal casulo-memory-demo"
    );
    assert!(lines[1].contains(" run --rm --detach --init --name=casulo-memory-demo "));
    assert!(lines[1].contains("--network-alias=memory"));
    assert!(lines[1].contains("casulo memory serve"));
    assert!(lines[2].contains(" create --rm "));
    assert!(lines[2].contains("--mcp-config"));
    assert!(lines[3].contains(" network connect casulo-memory-demo casulo-demo-claude-"));
    assert!(lines[4].contains(" start --attach --interactive casulo-demo-claude-"));
    // Only the memory container mounts memory/.
    assert!(lines[1].contains("target=/memory"));
    assert!(!lines[2].contains("target=/memory"));
}

#[test]
fn shell_dry_run_opens_bash_with_the_project_agent_config() {
    let (home, workspace) = home_with_project();
    let config_dir = home
        .path()
        .join("projects")
        .join("demo")
        .join("config")
        .join("claude");

    casulo()
        .args(["shell", "--dry-run"])
        .env("CASULO_HOME", home.path())
        .current_dir(&workspace)
        .assert()
        .success()
        .stdout(predicate::str::ends_with("casulo/demo:claude bash\n"))
        // Claude Code keeps everything, .claude.json included, in the one
        // config directory, so no loose file is mounted next to it.
        .stdout(predicate::str::contains(
            config_dir.to_string_lossy().as_ref(),
        ))
        // Windows quotes each mount argument, so the target ends in a quote there.
        .stdout(predicate::function(|stdout: &str| {
            stdout.contains("target=/home/casulo/.claude ")
                || stdout.contains("target=/home/casulo/.claude'")
        }))
        .stdout(predicate::str::contains("target=/home/casulo/.claude.json").not())
        .stdout(predicate::str::contains(
            "--env=CLAUDE_CONFIG_DIR=/home/casulo/.claude",
        ));
    // A dry run changes nothing on disk.
    assert!(!config_dir.exists());
}

#[test]
fn run_reports_a_missing_runtime() {
    let (home, workspace) = home_with_project();

    casulo()
        .arg("run")
        .env("CASULO_HOME", home.path())
        .current_dir(&workspace)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot run casulo-test-missing-runtime",
        ));
}

#[test]
fn run_masks_existing_secret_files_read_only() {
    let (home, workspace) = home_with_project();
    std::fs::write(
        home.path()
            .join("projects")
            .join("demo")
            .join("casulo.toml"),
        "repository = \"https://example.com/demo.git\"\nmask = [\".env\", \"missing.pem\"]\n",
    )
    .expect("write the project config");
    std::fs::write(workspace.join(".env"), "TOKEN=secret\n").expect("write .env");

    casulo()
        .args(["run", "--dry-run"])
        .env("CASULO_HOME", home.path())
        .current_dir(&workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("target=/workspace/.env,readonly"))
        .stdout(predicate::str::contains("missing.pem").not());
}

#[test]
fn run_dry_run_warns_about_broken_workflows_but_still_runs() {
    let (home, workspace) = home_with_project();
    let agents = home.path().join("agent").join("agents");
    std::fs::create_dir_all(&agents).expect("create the agents directory");
    let reviewer = agents.join("reviewer.md");
    std::fs::write(
        &reviewer,
        "+++\nname = \"reviewer\"\ndescription = \"Reviews diffs\"\n+++\nReport real problems.\n",
    )
    .expect("write the subagent");
    let input = home.path().join("workflow-input.toml");
    std::fs::write(
        &input,
        "description = \"Review the branch\"\n\n[[step]]\nagent = \"reviewer\"\n",
    )
    .expect("write the editor input");
    casulo()
        .args(["add", "workflow", "review"])
        .env("CASULO_HOME", home.path())
        .env("CASULO_EDITOR", editor_writing(&input))
        .assert()
        .success();
    let dry_run = || {
        casulo()
            .args(["run", "--dry-run"])
            .current_dir(&workspace)
            .env("CASULO_HOME", home.path())
            .assert()
            .success()
    };

    dry_run().stderr(predicate::str::contains("workflow").not());

    let skill = home
        .path()
        .join("agent")
        .join("skills")
        .join("workflow-review")
        .join("SKILL.md");
    std::fs::write(
        &skill,
        "---\nname: workflow-review\ndescription: old\n---\n",
    )
    .expect("change the skill");
    dry_run().stderr(predicate::str::contains(
        "warning: the skill workflow-review of workflow \"review\" is out of date",
    ));

    std::fs::remove_file(&reviewer).expect("remove the subagent");
    dry_run().stderr(
        predicate::str::contains("warning: workflow \"review\" is invalid")
            .and(predicate::str::contains("casulo add agent reviewer")),
    );
}
