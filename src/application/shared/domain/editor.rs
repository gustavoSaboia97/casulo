//! The text editor casulo opens for `casulo config`, chosen the way git
//! chooses its editor.

use std::fmt;

/// Variable that chooses the editor for casulo only, ahead of everything else.
pub const CASULO_EDITOR_VARIABLE: &str = "CASULO_EDITOR";

/// Where the editor setting came from, so an error can say how to change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSource {
    /// The `CASULO_EDITOR` environment variable.
    CasuloEditorVariable,
    /// The `editor` key in the global `casulo.toml`.
    ConfigKey,
    /// The `VISUAL` environment variable.
    VisualVariable,
    /// The `EDITOR` environment variable.
    EditorVariable,
    /// Nothing was set, so the default for the operating system is used.
    Default,
}

impl EditorSource {
    /// What the user can do to choose another editor.
    pub fn how_to_change(self) -> &'static str {
        match self {
            Self::CasuloEditorVariable => "change or unset CASULO_EDITOR",
            Self::ConfigKey => {
                "change `editor` in the global casulo.toml (`casulo config --path` prints where it is), or set CASULO_EDITOR"
            }
            Self::VisualVariable => "change VISUAL, or set CASULO_EDITOR to override it",
            Self::EditorVariable => "change EDITOR, or set CASULO_EDITOR to override it",
            Self::Default => {
                "set CASULO_EDITOR, `editor` in the global casulo.toml, VISUAL or EDITOR to an editor you have"
            }
        }
    }
}

impl fmt::Display for EditorSource {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Self::CasuloEditorVariable => "CASULO_EDITOR",
            Self::ConfigKey => "`editor` in the global casulo.toml",
            Self::VisualVariable => "VISUAL",
            Self::EditorVariable => "EDITOR",
            Self::Default => "the default editor",
        })
    }
}

/// The editor to start: a program, its own arguments, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommand {
    /// Name or path of the program, such as `code`.
    pub program: String,
    /// Arguments placed before the file, such as `--wait`.
    pub arguments: Vec<String>,
    /// Where the setting came from.
    pub source: EditorSource,
}

impl EditorCommand {
    /// Splits `value` on whitespace, so `code --wait` works. Returns `None`
    /// when it holds only whitespace, which counts as not set.
    pub fn parse(value: &str, source: EditorSource) -> Option<Self> {
        let mut words = value.split_whitespace().map(str::to_string);
        let program = words.next()?;
        Some(Self {
            program,
            arguments: words.collect(),
            source,
        })
    }
}

/// The editor settings that can choose an editor, in order of precedence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorSettings {
    /// Value of `CASULO_EDITOR`.
    pub casulo_editor: Option<String>,
    /// The `editor` key of the global `casulo.toml`.
    pub config: Option<String>,
    /// Value of `VISUAL`.
    pub visual: Option<String>,
    /// Value of `EDITOR`.
    pub editor: Option<String>,
}

/// The editor to use: `CASULO_EDITOR`, then the `editor` key, then `VISUAL`,
/// then `EDITOR`, else `vi` (`notepad` on Windows). `os` is a value of
/// `std::env::consts::OS`. Empty settings are skipped, as git does.
pub fn choose_editor(settings: &EditorSettings, os: &str) -> EditorCommand {
    let candidates = [
        (&settings.casulo_editor, EditorSource::CasuloEditorVariable),
        (&settings.config, EditorSource::ConfigKey),
        (&settings.visual, EditorSource::VisualVariable),
        (&settings.editor, EditorSource::EditorVariable),
    ];
    candidates
        .into_iter()
        .find_map(|(value, source)| EditorCommand::parse(value.as_deref()?, source))
        .unwrap_or_else(|| {
            let program = if os == "windows" { "notepad" } else { "vi" };
            EditorCommand {
                program: program.to_string(),
                arguments: Vec::new(),
                source: EditorSource::Default,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_set() -> EditorSettings {
        EditorSettings {
            casulo_editor: Some("casulo-editor".to_string()),
            config: Some("config-editor".to_string()),
            visual: Some("visual-editor".to_string()),
            editor: Some("editor-editor".to_string()),
        }
    }

    #[test]
    fn follows_the_git_precedence() {
        let mut settings = all_set();
        assert_eq!(
            choose_editor(&settings, "linux").source,
            EditorSource::CasuloEditorVariable
        );
        settings.casulo_editor = None;
        assert_eq!(choose_editor(&settings, "linux").program, "config-editor");
        settings.config = None;
        assert_eq!(
            choose_editor(&settings, "linux").source,
            EditorSource::VisualVariable
        );
        settings.visual = None;
        assert_eq!(choose_editor(&settings, "linux").program, "editor-editor");
    }

    #[test]
    fn defaults_to_vi_on_unix_and_notepad_on_windows() {
        let settings = EditorSettings::default();

        for os in ["linux", "macos"] {
            let chosen = choose_editor(&settings, os);
            assert_eq!(chosen.program, "vi");
            assert_eq!(chosen.source, EditorSource::Default);
        }
        assert_eq!(choose_editor(&settings, "windows").program, "notepad");
    }

    #[test]
    fn skips_empty_settings() {
        let settings = EditorSettings {
            casulo_editor: Some(String::new()),
            config: Some("   ".to_string()),
            visual: None,
            editor: Some("nano".to_string()),
        };

        let chosen = choose_editor(&settings, "linux");

        assert_eq!(chosen.program, "nano");
        assert_eq!(chosen.source, EditorSource::EditorVariable);
    }

    #[test]
    fn splits_arguments_on_whitespace() {
        let chosen = EditorCommand::parse("  code  --wait -n ", EditorSource::ConfigKey)
            .expect("has a program");

        assert_eq!(chosen.program, "code");
        assert_eq!(chosen.arguments, ["--wait", "-n"]);
    }
}
