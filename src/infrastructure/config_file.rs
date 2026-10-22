//! Reads and writes `casulo.toml` files.
//!
//! Files casulo creates come from commented templates, so each one documents
//! every option. casulo never rewrites them afterwards, which keeps the
//! user's comments and edits.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

use crate::application::project::domain::{GitHost, GlobalConfig, ProjectConfig, ProjectSource};
use crate::application::project::ports::ConfigStore;
use crate::application::shared::domain::WorkspacePath;

/// Template of the global `casulo.toml`, with `{placeholders}` for defaults
/// that live in code.
const GLOBAL_TEMPLATE: &str = include_str!("../../templates/casulo.toml");

/// Template of a project's `casulo.toml`.
const PROJECT_TEMPLATE: &str = include_str!("../../templates/project.toml");

/// The `casulo.toml` files on disk, and the other files casulo creates once
/// in its home, through the functions of this module.
pub struct ConfigFiles;

impl ConfigStore for ConfigFiles {
    fn create_global_config(&self, path: &Path) -> Result<bool> {
        create_private_if_missing(path, &global_config_text())
    }

    fn create_project_config(
        &self,
        path: &Path,
        source: &ProjectSource,
        mask: &[WorkspacePath],
        host: Option<&GitHost>,
    ) -> Result<bool> {
        create_private_if_missing(path, &project_config_text(source, mask, host))
    }

    fn create_if_missing(&self, path: &Path, text: &str) -> Result<bool> {
        create_if_missing(path, text)
    }

    fn load_global_config(&self, path: &Path) -> Result<GlobalConfig> {
        load_global_config(path)
    }

    fn load_project_config(&self, path: &Path) -> Result<ProjectConfig> {
        load_project_config(path)
    }
}

/// The global `casulo.toml` as casulo first writes it: every option commented
/// out with its default value.
pub fn global_config_text() -> String {
    let defaults = GlobalConfig::default();
    GLOBAL_TEMPLATE
        .replace("{default_runtime}", &defaults.runtime)
        .replace("{default_harness}", defaults.default_harness.as_str())
}

/// A new project's `casulo.toml`: the values casulo fills in, and every other
/// option commented out. A directory source must be absolute.
///
/// `host` is the push host read from a directory's remote; it turns `[git]`
/// on with only that value set.
pub fn project_config_text(
    source: &ProjectSource,
    mask: &[WorkspacePath],
    host: Option<&GitHost>,
) -> String {
    // Written through `toml::Value`, so quotes and backslashes are escaped.
    let source = match source {
        ProjectSource::Repository(url) => format!(
            "# Git URL the workspace was cloned from.\nrepository = {}",
            toml::Value::String(url.to_string())
        ),
        ProjectSource::Directory(directory) => format!(
            "# Directory on the host used as the workspace. It is mounted in place\n# at /workspace, so the agent's changes land in it.\nworkspace = {}",
            toml::Value::String(directory.display().to_string())
        ),
    };
    let mask = toml::Value::Array(
        mask.iter()
            .map(|path| toml::Value::String(path.to_string()))
            .collect(),
    );
    let git = match host {
        Some(host) => format!(
            "[git]\n# Host the agent pushes to, read from the repository's remote when\n# the project was created.\nhost = {}",
            toml::Value::String(host.to_string())
        ),
        None => "# [git]".to_string(),
    };
    PROJECT_TEMPLATE
        .replace("{source}", &source)
        .replace("{mask}", &mask.to_string())
        .replace("{git}", &git)
}

/// Writes `text` to `path` only if no file is there yet, creating its
/// directory. Returns whether the file was created.
fn create_if_missing(path: &Path, text: &str) -> Result<bool> {
    create(path, text, false)
}

/// Like `create_if_missing`, but on Linux and macOS only the owner can read
/// the new file, because it may hold secrets (the global `[secrets]`).
fn create_private_if_missing(path: &Path, text: &str) -> Result<bool> {
    create(path, text, true)
}

/// Whether other users of this machine can read `path`. Always `false` on
/// Windows, where the user's profile is private by default.
pub fn readable_by_others(path: &Path) -> Result<bool> {
    let metadata = fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(group_or_others_can_read(&metadata))
}

#[cfg(unix)]
fn group_or_others_can_read(metadata: &fs::Metadata) -> bool {
    // `PermissionsExt` gives the Unix mode bits; it only exists on Unix.
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o044 != 0
}

#[cfg(not(unix))]
fn group_or_others_can_read(_metadata: &fs::Metadata) -> bool {
    false
}

fn create(path: &Path, text: &str, private: bool) -> Result<bool> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    // `create_new` fails when the file exists, so an existing file is never touched.
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if private {
        // `OpenOptionsExt` sets the mode at creation, so the file is never
        // readable by others, not even for a moment.
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let file = options.open(path);
    match file {
        Ok(mut file) => {
            file.write_all(text.as_bytes())
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

/// Loads the global configuration, or the defaults when the file does not exist.
pub fn load_global_config(path: &Path) -> Result<GlobalConfig> {
    match fs::read_to_string(path) {
        Ok(text) => parse(path, &text),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(GlobalConfig::default()),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Reads only the `editor` key of the global configuration, even when the
/// rest of the file is invalid, so the editor that fixes it is still the
/// one the user chose. `None` when the file or the key does not exist.
pub fn load_configured_editor(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(editor_key(&text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// Finds `editor = "..."` among the top-level keys, which come before the
/// first `[table]`. Each line is parsed alone, so an error elsewhere in the
/// file cannot hide the key.
fn editor_key(text: &str) -> Option<String> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| toml::from_str::<toml::Table>(line).ok())
        .find_map(|table| table.get("editor")?.as_str().map(str::to_string))
}

/// Loads the configuration of a project. The file must exist.
pub fn load_project_config(path: &Path) -> Result<ProjectConfig> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse(path, &text)
}

// `<T: DeserializeOwned>` makes this one function work for any config type
// serde can read, like a function taking a class as a parameter in Python.
fn parse<T: DeserializeOwned>(path: &Path, text: &str) -> Result<T> {
    toml::from_str(text).with_context(|| format!("invalid configuration in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use serde::Serialize;

    use crate::application::project::domain::{
        ComposeSettings, GitSettings, ImageSettings, MemorySettings, ModelSettings, RepositoryUrl,
    };
    use crate::application::shared::domain::{HarnessName, Secret};

    /// Writes `config` as TOML, so a test can check that loading reads back
    /// every value. casulo itself never rewrites a config file.
    fn save<T: Serialize>(path: &Path, config: &T) {
        fs::write(path, toml::to_string(config).unwrap()).unwrap();
    }

    #[test]
    fn missing_global_file_gives_defaults() {
        let directory = tempfile::tempdir().unwrap();

        let config = load_global_config(&directory.path().join("casulo.toml")).unwrap();

        assert_eq!(config, GlobalConfig::default());
    }

    #[test]
    fn global_config_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("casulo.toml");
        let config = GlobalConfig {
            runtime: "/usr/local/bin/container-runtime".to_string(),
            default_harness: HarnessName::parse("codex").unwrap(),
            git: GitSettings {
                name: Some("Ada Lovelace".to_string()),
                email: Some("ada@example.com".to_string()),
                ..GitSettings::default()
            },
            editor: Some("code --wait".to_string()),
            harness: BTreeMap::new(),
            memory: MemorySettings::default(),
            secrets: BTreeMap::from([(
                "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                Secret::new("sk-ant-oat01-value"),
            )]),
        };

        save(&path, &config);

        assert_eq!(load_global_config(&path).unwrap(), config);
    }

    #[test]
    fn project_config_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("casulo.toml");
        for harness in [None, Some(HarnessName::parse("claude").unwrap())] {
            let config = ProjectConfig {
                repository: Some(RepositoryUrl::parse("https://example.com/repo.git").unwrap()),
                workspace: None,
                harness,
                mask: Vec::new(),
                models: BTreeMap::new(),
                git: GitSettings::default(),
                compose: ComposeSettings::default(),
                memory: MemorySettings::default(),
                image: ImageSettings::default(),
            };

            save(&path, &config);

            assert_eq!(load_project_config(&path).unwrap(), config);
        }
    }

    #[test]
    fn invalid_toml_names_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("casulo.toml");
        fs::write(&path, "runtime = ").unwrap();

        let error = load_global_config(&path).unwrap_err();

        assert!(format!("{error:#}").contains("invalid configuration in"));
        assert!(format!("{error:#}").contains("casulo.toml"));
    }

    /// Removes the "# " in front of every commented option, the way a user
    /// turns an option on. Prose comments stay commented.
    fn uncomment_options(text: &str) -> String {
        text.lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(rest) if rest.starts_with('[') || rest.contains(" = ") => rest,
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every key, with its table path, such as `git.name`.
    fn key_paths(table: &toml::Table, prefix: &str) -> Vec<String> {
        let mut paths = Vec::new();
        for (key, value) in table {
            let path = format!("{prefix}{key}");
            match value {
                toml::Value::Table(inner) => paths.extend(key_paths(inner, &format!("{path}."))),
                _ => paths.push(path),
            }
        }
        paths.sort();
        paths
    }

    fn full_global_config() -> GlobalConfig {
        GlobalConfig {
            runtime: "podman".to_string(),
            default_harness: HarnessName::parse("codex").unwrap(),
            git: full_git(),
            editor: Some("nano".to_string()),
            harness: full_models(),
            memory: full_memory(),
            secrets: BTreeMap::from([(
                "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                Secret::new("sk-ant-oat01-..."),
            )]),
        }
    }

    fn full_memory() -> MemorySettings {
        MemorySettings {
            enabled: Some(true),
        }
    }

    fn full_git() -> GitSettings {
        GitSettings {
            name: Some("Ada".to_string()),
            email: Some("ada@example.com".to_string()),
            push: Some(true),
            token: Some(Secret::new("github_pat_...")),
            host: Some(GitHost::parse("github.com").unwrap()),
            protected_branches: Some(vec!["main".to_string()]),
        }
    }

    fn full_models() -> BTreeMap<HarnessName, ModelSettings> {
        let settings = ModelSettings {
            model: Some("model".to_string()),
            effort: Some("effort".to_string()),
        };
        ["claude", "codex"]
            .iter()
            .map(|name| (HarnessName::parse(name).unwrap(), settings.clone()))
            .collect()
    }

    fn full_project_config() -> ProjectConfig {
        ProjectConfig {
            repository: Some(RepositoryUrl::parse("https://example.com/repo.git").unwrap()),
            workspace: None,
            harness: Some(HarnessName::parse("codex").unwrap()),
            mask: crate::application::project::domain::config::default_mask(),
            models: full_models(),
            git: full_git(),
            compose: ComposeSettings {
                enabled: true,
                file: WorkspacePath::parse("compose.yaml").unwrap(),
            },
            memory: full_memory(),
            image: ImageSettings {
                install: vec!["apt-get install --yes build-essential".to_string()],
            },
        }
    }

    #[test]
    fn global_template_parses_to_the_defaults() {
        let config: GlobalConfig = toml::from_str(&global_config_text()).unwrap();

        assert_eq!(config, GlobalConfig::default());
    }

    #[test]
    fn global_template_lists_every_option() {
        let uncommented = uncomment_options(&global_config_text());
        // Parsing as `GlobalConfig` rejects unknown keys, so this also catches
        // a template option that does not exist.
        let _: GlobalConfig = toml::from_str(&uncommented).unwrap();
        let template: toml::Table = toml::from_str(&uncommented).unwrap();
        let full = toml::Table::try_from(full_global_config()).unwrap();

        assert_eq!(key_paths(&template, ""), key_paths(&full, ""));
    }

    #[test]
    fn project_template_parses_to_the_given_values() {
        let url = RepositoryUrl::parse(r"C:\repos\'app'").unwrap();
        let mask = crate::application::project::domain::config::default_mask();

        let source = ProjectSource::Repository(url.clone());
        let config: ProjectConfig =
            toml::from_str(&project_config_text(&source, &mask, None)).unwrap();

        assert_eq!(config.repository, Some(url));
        assert_eq!(config.workspace, None);
        assert_eq!(config.harness, None);
        assert_eq!(config.mask, mask);
    }

    #[test]
    fn project_template_lists_every_option() {
        let directory = std::env::temp_dir().join("app");
        let mut directory_config = full_project_config();
        directory_config.repository = None;
        directory_config.workspace = Some(directory.clone());
        let cases = [
            (
                ProjectSource::Repository(
                    RepositoryUrl::parse("https://example.com/repo.git").unwrap(),
                ),
                full_project_config(),
            ),
            (ProjectSource::Directory(directory), directory_config),
        ];
        for (source, full) in cases {
            let text = project_config_text(
                &source,
                &crate::application::project::domain::config::default_mask(),
                None,
            );
            let uncommented = uncomment_options(&text);
            let _: ProjectConfig = toml::from_str(&uncommented).unwrap();
            let template: toml::Table = toml::from_str(&uncommented).unwrap();
            let full = toml::Table::try_from(full).unwrap();

            assert_eq!(key_paths(&template, ""), key_paths(&full, ""));
        }
    }

    #[test]
    fn project_template_sets_the_host_read_from_a_directory_remote() {
        let source = ProjectSource::Directory(std::env::temp_dir().join("app"));
        let host = GitHost::parse("gitlab.example.com").unwrap();

        let config: ProjectConfig =
            toml::from_str(&project_config_text(&source, &[], Some(&host))).unwrap();

        assert_eq!(config.git.host, Some(host));
        assert_eq!(config.git.push, None);
    }

    #[test]
    fn project_template_for_a_directory_escapes_the_path() {
        let directory = std::env::temp_dir().join("it's \"here\"");
        let source = ProjectSource::Directory(directory.clone());

        let config: ProjectConfig =
            toml::from_str(&project_config_text(&source, &[], None)).unwrap();

        assert_eq!(config.workspace, Some(directory));
        assert_eq!(config.repository, None);
    }

    #[test]
    fn create_if_missing_never_overwrites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config").join("casulo.toml");

        assert!(create_if_missing(&path, "first").unwrap());
        assert!(!create_if_missing(&path, "second").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
    }

    #[cfg(unix)]
    #[test]
    fn config_files_are_readable_only_by_their_owner() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let global = directory.path().join("casulo.toml");
        let project = directory.path().join("app").join("casulo.toml");
        let shared = directory.path().join("AGENTS.md");
        let source = ProjectSource::Directory(std::env::temp_dir().join("app"));

        assert!(ConfigFiles.create_global_config(&global).unwrap());
        assert!(
            ConfigFiles
                .create_project_config(&project, &source, &[], None)
                .unwrap()
        );
        ConfigFiles.create_if_missing(&shared, "text").unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o644)).unwrap();

        for private in [&global, &project] {
            let mode = fs::metadata(private).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{}", private.display());
            assert!(!readable_by_others(private).unwrap());
        }
        assert!(readable_by_others(&shared).unwrap());
    }

    #[test]
    fn missing_project_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();

        let error = load_project_config(&directory.path().join("casulo.toml")).unwrap_err();

        assert!(error.to_string().starts_with("cannot read"));
    }

    #[test]
    fn editor_is_read_from_a_file_with_an_unknown_key() {
        let text = "editor = \"code --wait\"\nruntme = \"podman\"\n";
        assert!(toml::from_str::<GlobalConfig>(text).is_err());

        assert_eq!(editor_key(text).as_deref(), Some("code --wait"));
    }

    #[test]
    fn editor_is_read_from_a_file_with_broken_toml() {
        let text = "editor = \"code --wait\"\n[git\npush = yes\n";

        assert_eq!(editor_key(text).as_deref(), Some("code --wait"));
    }

    #[test]
    fn editor_inside_a_table_is_not_the_global_editor() {
        assert_eq!(editor_key("[harness.claude]\neditor = \"vim\"\n"), None);
    }

    #[test]
    fn missing_global_file_has_no_editor() {
        let directory = tempfile::tempdir().unwrap();

        let editor = load_configured_editor(&directory.path().join("casulo.toml")).unwrap();

        assert_eq!(editor, None);
    }
}
