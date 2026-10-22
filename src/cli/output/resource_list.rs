//! Terminal side of `casulo list`: how subagents, skills, MCP servers
//! and workflows are shown.

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal};

use crate::application::agent::services::resources::ResourceSummary;

/// Where the description wraps, counting its indentation.
const LINE_WIDTH: usize = 80;
/// Indentation of the description under its name.
const INDENT: &str = "  ";
/// Width the kind is padded to in blocks, the length of "agent" and "skill".
const MIN_KIND_WIDTH: usize = 5;
/// ANSI escape codes: bold blue, then back to normal text.
const HIGHLIGHT: &str = "\x1b[1;34m";
const RESET: &str = "\x1b[0m";

/// How `casulo list` prints what it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    /// One tab-separated line per resource, for scripts and pipes.
    Lines,
    /// A block per resource, the name on top and the description below,
    /// for a person reading a terminal.
    Blocks {
        /// Whether the name is highlighted with color.
        color: bool,
    },
}

/// Blocks when stdout is a terminal, lines otherwise, so scripts that read
/// the output keep working.
pub fn style_from_environment() -> ListStyle {
    if !io::stdout().is_terminal() {
        return ListStyle::Lines;
    }
    ListStyle::Blocks {
        color: color_allowed(
            env::var_os("NO_COLOR"),
            env::consts::OS,
            env::var_os("WT_SESSION").is_some() || env::var_os("TERM").is_some(),
        ),
    }
}

/// Color is on unless `NO_COLOR` is set (https://no-color.org). On Windows
/// it also needs a terminal that understands ANSI codes, such as Windows
/// Terminal (`WT_SESSION`) or one that sets `TERM`; the old console would
/// print the codes as text.
fn color_allowed(no_color: Option<OsString>, os: &str, ansi_terminal: bool) -> bool {
    let no_color = no_color.is_some_and(|value| !value.is_empty());
    !no_color && (os != "windows" || ansi_terminal)
}

/// The text `casulo list` prints. `show_kind` adds the kind before each
/// name, for when several kinds are listed together.
pub fn render(summaries: &[ResourceSummary], show_kind: bool, style: ListStyle) -> String {
    match style {
        ListStyle::Lines => summaries
            .iter()
            .map(|summary| {
                if show_kind {
                    format!(
                        "{}\t{}\t{}\n",
                        summary.kind, summary.name, summary.description
                    )
                } else {
                    format!("{}\t{}\n", summary.name, summary.description)
                }
            })
            .collect(),
        ListStyle::Blocks { color } => {
            // Padded to the longest kind listed, at least "agent" and "skill",
            // so names line up and the output without workflows stays the same.
            let kind_width = summaries
                .iter()
                .map(|summary| summary.kind.to_string().len())
                .fold(MIN_KIND_WIDTH, usize::max);
            let blocks: Vec<String> = summaries
                .iter()
                .map(|summary| render_block(summary, show_kind, kind_width, color))
                .collect();
            // A blank line between blocks makes each one easy to find.
            blocks.join("\n")
        }
    }
}

/// One resource: its kind and name on the first line, then its description,
/// indented and wrapped.
fn render_block(
    summary: &ResourceSummary,
    show_kind: bool,
    kind_width: usize,
    color: bool,
) -> String {
    let title = if show_kind {
        format!(
            "{:<kind_width$}  {}",
            summary.kind.to_string(),
            summary.name
        )
    } else {
        summary.name.clone()
    };
    let mut block = if color {
        format!("{HIGHLIGHT}{title}{RESET}\n")
    } else {
        format!("{title}\n")
    };
    for line in wrap(&summary.description, LINE_WIDTH - INDENT.len()) {
        block.push_str(&format!("{INDENT}{line}\n"));
    }
    block
}

/// Splits `text` into lines of at most `width` characters, breaking between
/// words. A word longer than `width` gets a line of its own.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let needed = line.chars().count() + 1 + word.chars().count();
        if !line.is_empty() && needed > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::shared::domain::ResourceKind;

    fn summary(kind: ResourceKind, name: &str, description: &str) -> ResourceSummary {
        ResourceSummary {
            kind,
            name: name.to_string(),
            description: description.to_string(),
        }
    }

    #[test]
    fn lines_keep_the_tab_separated_format_for_scripts() {
        let summaries = [summary(ResourceKind::Agent, "reviewer", "Reviews diffs")];

        let with_kind = render(&summaries, true, ListStyle::Lines);
        let without_kind = render(&summaries, false, ListStyle::Lines);

        assert_eq!(with_kind, "agent\treviewer\tReviews diffs\n");
        assert_eq!(without_kind, "reviewer\tReviews diffs\n");
    }

    #[test]
    fn blocks_show_the_name_above_the_indented_description() {
        let summaries = [
            summary(ResourceKind::Agent, "reviewer", "Reviews diffs"),
            summary(
                ResourceKind::Mcp,
                "context7",
                "npx -y @upstash/context7-mcp",
            ),
        ];

        let text = render(&summaries, true, ListStyle::Blocks { color: false });

        assert_eq!(
            text,
            "agent  reviewer\n  Reviews diffs\n\nmcp    context7\n  npx -y @upstash/context7-mcp\n"
        );
    }

    #[test]
    fn blocks_pad_kinds_to_the_longest_one_listed() {
        let summaries = [
            summary(ResourceKind::Agent, "reviewer", "Reviews diffs"),
            summary(ResourceKind::Workflow, "feature", "Build a feature"),
        ];

        let text = render(&summaries, true, ListStyle::Blocks { color: false });

        assert_eq!(
            text,
            "agent     reviewer\n  Reviews diffs\n\nworkflow  feature\n  Build a feature\n"
        );
    }

    #[test]
    fn blocks_highlight_the_title_in_bold_blue_when_color_is_on() {
        let summaries = [summary(ResourceKind::Skill, "tdd", "Test first")];

        let text = render(&summaries, false, ListStyle::Blocks { color: true });

        assert_eq!(text, "\x1b[1;34mtdd\x1b[0m\n  Test first\n");
    }

    #[test]
    fn blocks_wrap_long_descriptions_between_words() {
        let description = "word ".repeat(30);
        let summaries = [summary(ResourceKind::Skill, "long", &description)];

        let text = render(&summaries, false, ListStyle::Blocks { color: false });

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| line.len() <= LINE_WIDTH));
        assert!(lines[1..].iter().all(|line| line.starts_with(INDENT)));
    }

    #[test]
    fn wrap_puts_a_word_longer_than_the_width_on_its_own_line() {
        let lines = wrap("a abcdefghij b", 5);

        assert_eq!(lines, ["a", "abcdefghij", "b"]);
    }

    #[test]
    fn color_is_off_when_no_color_is_set() {
        let allowed = color_allowed(Some(OsString::from("1")), "linux", true);

        assert!(!allowed);
    }

    #[test]
    fn color_is_on_when_no_color_is_empty() {
        let allowed = color_allowed(Some(OsString::new()), "macos", false);

        assert!(allowed);
    }

    #[test]
    fn color_on_windows_needs_a_terminal_that_understands_ansi() {
        let old_console = color_allowed(None, "windows", false);
        let windows_terminal = color_allowed(None, "windows", true);

        assert!(!old_console);
        assert!(windows_terminal);
    }
}
