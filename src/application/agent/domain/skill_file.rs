//! A supporting file inside a skill's folder, such as `assets/template.md`.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// A relative path inside a skill's folder, written with `/`, that is not
/// the skill's own `SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile(String);

impl SkillFile {
    /// Parses a path relative to the skill's folder. Absolute paths, `..`
    /// and Windows separators are rejected, so it can never point outside.
    pub fn parse(value: &str) -> Result<Self> {
        let inside = !value.is_empty()
            && !value.starts_with('/')
            && !value.contains(['\\', ':'])
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");
        if !inside {
            bail!(
                "invalid skill file \"{value}\": use a path inside the skill's folder, such as assets/template.md or references/examples.md"
            );
        }
        if value == "SKILL.md" {
            bail!("SKILL.md is the skill itself; change it with `casulo edit skill <name>`");
        }
        Ok(Self(value.to_string()))
    }

    /// The path under the skill's `folder` on the host.
    pub fn in_folder(&self, folder: &Path) -> PathBuf {
        self.0
            .split('/')
            .fold(folder.to_path_buf(), |path, part| path.join(part))
    }

    /// The last part of the path, such as `template.md`.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
}

impl fmt::Display for SkillFile {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_paths_inside_the_skill_folder() {
        for value in ["assets/template.md", "references/examples.md", "notes.md"] {
            assert!(SkillFile::parse(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_paths_that_could_leave_the_skill_folder() {
        for value in [
            "",
            "/etc/passwd",
            "../other/SKILL.md",
            "assets/../../x",
            "./x",
            "assets//x",
            "assets/",
            "assets\\x",
            "C:x",
        ] {
            assert!(SkillFile::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn rejects_the_skill_itself() {
        let error = SkillFile::parse("SKILL.md").unwrap_err();

        assert!(error.to_string().contains("casulo edit skill"));
    }

    #[test]
    fn joins_each_part_under_the_folder() {
        let file = SkillFile::parse("assets/template.md").unwrap();

        let path = file.in_folder(Path::new("skills").join("tdd").as_path());

        assert_eq!(
            path,
            Path::new("skills")
                .join("tdd")
                .join("assets")
                .join("template.md")
        );
        assert_eq!(file.file_name(), "template.md");
    }
}
