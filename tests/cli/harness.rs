//! Functional tests of `casulo harness`.

use crate::support::casulo;

#[test]
fn harness_list_shows_built_ins_and_overrides() {
    let home = tempfile::tempdir().expect("create a temporary directory");
    let overrides = home.path().join("harnesses");
    std::fs::create_dir_all(&overrides).expect("create the harnesses directory");
    let codex_override = overrides.join("codex.toml");
    std::fs::write(
        &codex_override,
        "name = \"codex\"\ninstall = [\"npm install -g @openai/codex@latest\"]\ncommand = [\"codex\"]\n",
    )
    .expect("write the override");

    casulo()
        .args(["harness", "list"])
        .env("CASULO_HOME", home.path())
        .assert()
        .success()
        .stdout(format!(
            "claude\tbuilt-in\tinstructions,skills,agents,mcp,hooks,model,effort\ncodex\t{}\tnothing injected\n",
            codex_override.display()
        ));
}
