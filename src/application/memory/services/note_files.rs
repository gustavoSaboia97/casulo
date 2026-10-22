//! The note files under `notes/`: a readable copy of every note, and the
//! only part of memory that cannot be rebuilt. casulo renders them; people
//! change them only through `casulo edit memory`, which parses them back.
//!
//! ```markdown
//! +++
//! path = "src/adapters/docker.rs"
//! +++
//!
//! ## convention @ 9f3c1e0
//! <!-- note: 7 author: harness:codex reinforced: 3 -->
//! **Every container runs read-only**
//! `docker run` always gets `--read-only`.
//! See: [[src/services/run.rs]]
//! ```

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::application::memory::domain::event::NoteText;
use crate::application::memory::domain::{NoteKind, RepoPath, StoredFileNote, StoredProjectNote};
use crate::application::memory::ports::MemoryStore;

/// One note as written in a note file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSection {
    /// What it says.
    pub kind: NoteKind,
    /// The stored note it shows, `None` for a section the user added.
    pub note_id: Option<i64>,
    /// One line.
    pub summary: String,
    /// The details.
    pub body: String,
    /// Related files.
    pub refs: Vec<String>,
    /// The other text of a note in conflict: summary and body.
    pub conflict: Option<(String, String)>,
}

/// A parsed note file: its TOML header and its notes.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteFile {
    /// The frontmatter: `path`, or `topic` and maybe `branch`.
    pub header: toml::Table,
    /// The notes, in file order.
    pub sections: Vec<NoteSection>,
}

/// The note file of `path` with every stored version of every kind, or
/// `None` when it has no notes.
pub fn render_file_notes(database: &dyn MemoryStore, path: &RepoPath) -> Result<Option<String>> {
    let mut notes = database.file_notes(path)?;
    if notes.is_empty() {
        return Ok(None);
    }
    notes.sort_by_key(|note| (note.kind, note.id));
    let mut header = toml::Table::new();
    header.insert("path".to_string(), path.as_str().into());
    let sections: Vec<String> = notes.iter().map(file_section).collect();
    Ok(Some(render(&header, &sections)))
}

/// The note file of a project topic, general (`branch` is `None`) or of a
/// branch, or `None` when it has no notes.
pub fn render_topic(
    database: &dyn MemoryStore,
    branch: Option<&str>,
    topic: &str,
) -> Result<Option<String>> {
    let notes: Vec<StoredProjectNote> = database
        .project_notes(branch)?
        .into_iter()
        .filter(|note| note.topic == topic && note.branch.as_deref() == branch)
        .collect();
    if notes.is_empty() {
        return Ok(None);
    }
    let mut header = toml::Table::new();
    header.insert("topic".to_string(), topic.into());
    if let Some(branch) = branch {
        header.insert("branch".to_string(), branch.into());
    }
    let sections: Vec<String> = notes
        .iter()
        .map(|note| {
            section(
                &format!("## {}", note.kind),
                note.id,
                &note.author.to_text(),
                note.reinforced,
                &note.text,
                &[],
                note.conflict.as_ref(),
            )
        })
        .collect();
    Ok(Some(render(&header, &sections)))
}

fn render(header: &toml::Table, sections: &[String]) -> String {
    format!("+++\n{}+++\n\n{}", header, sections.join("\n"))
}

fn file_section(note: &StoredFileNote) -> String {
    let refs: Vec<String> = note.refs.iter().map(|path| path.to_string()).collect();
    section(
        &format!("## {} @ {}", note.kind, short_hash(&note.code_hash)),
        note.id,
        &note.author.to_text(),
        note.reinforced,
        &note.text,
        &refs,
        note.conflict.as_ref(),
    )
}

fn section(
    heading: &str,
    id: i64,
    author: &str,
    reinforced: i64,
    text: &NoteText,
    refs: &[String],
    conflict: Option<&NoteText>,
) -> String {
    let mut out = format!(
        "{heading}\n<!-- note: {id} author: {author} reinforced: {reinforced} -->\n**{}**\n",
        text.summary.as_str()
    );
    if !text.body.as_str().is_empty() {
        out.push_str(text.body.as_str());
        out.push('\n');
    }
    if !refs.is_empty() {
        let links: Vec<String> = refs.iter().map(|path| format!("[[{path}]]")).collect();
        out.push_str(&format!("See: {}\n", links.join(" ")));
    }
    if let Some(other) = conflict {
        out.push_str(&format!("\n### conflict\n**{}**\n", other.summary.as_str()));
        if !other.body.as_str().is_empty() {
            out.push_str(other.body.as_str());
            out.push('\n');
        }
    }
    out
}

/// The first 7 characters of a hash, as git shows them.
fn short_hash(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// Parses a note file. Fails with the line that does not fit the format.
pub fn parse_note_file(text: &str) -> Result<NoteFile> {
    let text = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let rest = text.strip_prefix("+++\n").context(
        "a note file starts with a +++ line, then its TOML header, then another +++ line",
    )?;
    let (header, body) = rest
        .split_once("\n+++\n")
        .or_else(|| rest.strip_prefix("+++\n").map(|body| ("", body)))
        .context("the header has no closing +++ line")?;
    let header: toml::Table = toml::from_str(header).context("the header is not valid TOML")?;

    let mut sections: Vec<NoteSection> = Vec::new();
    let mut in_conflict = false;
    for (index, line) in body.lines().enumerate() {
        let number = index + 1;
        if let Some(heading) = line.strip_prefix("## ") {
            let kind_name = heading.split_whitespace().next().unwrap_or_default();
            let kind = NoteKind::parse(kind_name)
                .with_context(|| format!("line {number} of the notes: \"{line}\""))?;
            sections.push(NoteSection {
                kind,
                note_id: None,
                summary: String::new(),
                body: String::new(),
                refs: Vec::new(),
                conflict: None,
            });
            in_conflict = false;
            continue;
        }
        let Some(current) = sections.last_mut() else {
            if line.trim().is_empty() {
                continue;
            }
            bail!("line {number} of the notes is outside a section: start one with `## <kind>`");
        };
        if line.trim() == "### conflict" {
            current.conflict = Some((String::new(), String::new()));
            in_conflict = true;
            continue;
        }
        // The note comment is only read right after the heading, so text
        // further down that looks like one stays text and cannot point the
        // section at another note.
        let at_section_start =
            !in_conflict && current.note_id.is_none() && current.summary.is_empty();
        if let Some(comment) = line
            .trim()
            .strip_prefix("<!-- note:")
            .filter(|_| at_section_start)
        {
            let id = comment.split_whitespace().next().unwrap_or_default();
            current.note_id =
                Some(id.parse().with_context(|| {
                    format!("line {number}: the note id \"{id}\" is not a number")
                })?);
            continue;
        }
        if let Some(links) = line.strip_prefix("See:") {
            current.refs = links
                .split("]]")
                .filter_map(|part| part.trim().strip_prefix("[["))
                .map(str::to_string)
                .collect();
            continue;
        }
        let (summary, body) = match (in_conflict, current.conflict.as_mut()) {
            (true, Some((summary, body))) => (summary, body),
            _ => (&mut current.summary, &mut current.body),
        };
        if summary.is_empty() {
            if let Some(text) = line
                .trim()
                .strip_prefix("**")
                .and_then(|text| text.strip_suffix("**"))
            {
                *summary = text.to_string();
            } else if !line.trim().is_empty() {
                bail!("line {number}: a note starts with its summary in bold, like **One line**");
            }
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    for section in &mut sections {
        section.body = section.body.trim().to_string();
        if let Some((_, body)) = section.conflict.as_mut() {
            *body = body.trim().to_string();
        }
    }
    Ok(NoteFile { header, sections })
}

/// Where the note file of `path` lives: `notes/<path>.md`.
pub fn file_notes_path(notes_dir: &Path, path: &RepoPath) -> PathBuf {
    let mut file = path
        .as_str()
        .split('/')
        .fold(notes_dir.to_path_buf(), |file, part| file.join(part))
        .into_os_string();
    file.push(".md");
    PathBuf::from(file)
}

/// Where a topic's note file lives: `notes/_project/<topic>.md`, or
/// `notes/_branches/<branch>/<topic>.md`. `None` for a branch name that is
/// not a safe relative path.
pub fn topic_path(notes_dir: &Path, branch: Option<&str>, topic: &str) -> Option<PathBuf> {
    let folder = match branch {
        None => notes_dir.join("_project"),
        Some(branch) => {
            let branch = RepoPath::parse(branch).ok()?;
            branch
                .as_str()
                .split('/')
                .fold(notes_dir.join("_branches"), |folder, part| {
                    folder.join(part)
                })
        }
    };
    Some(folder.join(format!("{topic}.md")))
}

/// Rewrites the note file of `path` from the database, or removes it when
/// the file has no notes left.
pub fn sync_file_notes(
    notes_dir: &Path,
    database: &dyn MemoryStore,
    path: &RepoPath,
) -> Result<()> {
    write_or_remove(
        &file_notes_path(notes_dir, path),
        render_file_notes(database, path)?,
    )
}

/// Rewrites a topic's note file from the database, or removes it.
pub fn sync_topic(
    notes_dir: &Path,
    database: &dyn MemoryStore,
    branch: Option<&str>,
    topic: &str,
) -> Result<()> {
    let Some(file) = topic_path(notes_dir, branch, topic) else {
        return Ok(());
    };
    write_or_remove(&file, render_topic(database, branch, topic)?)
}

fn write_or_remove(file: &Path, text: Option<String>) -> Result<()> {
    match text {
        Some(text) => {
            if let Some(folder) = file.parent() {
                fs::create_dir_all(folder)
                    .with_context(|| format!("cannot create {}", folder.display()))?;
            }
            fs::write(file, text).with_context(|| format!("cannot write {}", file.display()))
        }
        None => match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("cannot remove {}", file.display())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::memory::domain::event::{AcceptedNote, Event, NoteRef};
    use crate::application::memory::domain::{Author, NoteBody, NoteSummary, NoteTarget, Topic};
    use crate::infrastructure::sqlite::Database;

    const AT: &str = "2026-10-09T12:00:00Z";

    fn text(summary: &str, body: &str) -> NoteText {
        NoteText {
            summary: NoteSummary::parse(summary).unwrap(),
            body: NoteBody::parse(body).unwrap(),
        }
    }

    fn database() -> Database {
        let mut database = Database::open_in_memory().unwrap();
        let note =
            |hash: &str, kind: NoteKind, text: NoteText, refs: Vec<RepoPath>, author: Author| {
                Event::NoteAccepted {
                    note: AcceptedNote {
                        target: NoteTarget::File {
                            path: RepoPath::parse("src/adapters/docker.rs").unwrap(),
                            code_hash: hash.to_string(),
                        },
                        kind,
                        summary: text.summary,
                        body: text.body,
                        refs,
                        author,
                    },
                }
            };
        for event in [
            note(
                "9f3c1e0a2b4d",
                NoteKind::Convention,
                text(
                    "Every container runs read-only",
                    "`docker run` always gets `--read-only`.\nOnly `/workspace` is writable.",
                ),
                vec![RepoPath::parse("src/services/run.rs").unwrap()],
                Author::Harness("codex".to_string()),
            ),
            note(
                "41ab2d8ffff",
                NoteKind::Convention,
                text("Feature-x adds a cache volume", ""),
                Vec::new(),
                Author::Harness("claude".to_string()),
            ),
            note(
                "9f3c1e0a2b4d",
                NoteKind::Purpose,
                text("Runs containers through the docker binary", ""),
                Vec::new(),
                Author::Writer,
            ),
            Event::NoteConflicted {
                note: NoteRef::File(2),
                proposal_id: 1,
                other: text("Feature-x writes to the workspace only", "Nothing else."),
            },
            Event::NoteAccepted {
                note: AcceptedNote {
                    target: NoteTarget::Branch {
                        branch: "feature/podman".to_string(),
                        topic: Topic::parse("runtime").unwrap(),
                    },
                    kind: NoteKind::Decision,
                    summary: NoteSummary::parse("This branch moves to Podman").unwrap(),
                    body: NoteBody::parse("Ignore the old adapter.").unwrap(),
                    refs: Vec::new(),
                    author: Author::Harness("codex".to_string()),
                },
            },
        ] {
            database.apply(&event, AT).unwrap();
        }
        database
    }

    fn docker() -> RepoPath {
        RepoPath::parse("src/adapters/docker.rs").unwrap()
    }

    #[test]
    fn renders_every_version_and_kind_of_a_file() {
        let rendered = render_file_notes(&database(), &docker()).unwrap().unwrap();

        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn rendering_then_parsing_round_trips() {
        let database = database();
        let rendered = render_file_notes(&database, &docker()).unwrap().unwrap();

        let parsed = parse_note_file(&rendered).unwrap();

        assert_eq!(
            parsed.header["path"].as_str(),
            Some("src/adapters/docker.rs")
        );
        let stored = database.file_notes(&docker()).unwrap();
        assert_eq!(parsed.sections.len(), stored.len());
        for section in &parsed.sections {
            let note = stored
                .iter()
                .find(|note| Some(note.id) == section.note_id)
                .unwrap();
            assert_eq!(section.kind, note.kind);
            assert_eq!(section.summary, note.text.summary.as_str());
            assert_eq!(section.body, note.text.body.as_str());
            let refs: Vec<String> = note.refs.iter().map(|path| path.to_string()).collect();
            assert_eq!(section.refs, refs);
            assert_eq!(
                section.conflict,
                note.conflict.as_ref().map(|other| (
                    other.summary.as_str().to_string(),
                    other.body.as_str().to_string()
                ))
            );
        }
    }

    #[test]
    fn topic_files_round_trip_with_their_branch() {
        let database = database();
        let rendered = render_topic(&database, Some("feature/podman"), "runtime")
            .unwrap()
            .unwrap();

        let parsed = parse_note_file(&rendered).unwrap();

        assert_eq!(parsed.header["branch"].as_str(), Some("feature/podman"));
        assert_eq!(parsed.sections[0].summary, "This branch moves to Podman");
        assert_eq!(render_topic(&database, None, "runtime").unwrap(), None);
    }

    #[test]
    fn a_section_added_by_hand_has_no_id() {
        let parsed = parse_note_file("+++\npath = \"src/a.rs\"\n+++\n\n## gotcha\n**Watch the order of mounts**\nNested mounts come last.\n").unwrap();

        assert_eq!(parsed.sections[0].note_id, None);
        assert_eq!(parsed.sections[0].body, "Nested mounts come last.");
    }

    #[test]
    fn a_note_comment_inside_a_body_stays_text() {
        // Written before bodies refused such lines: the comment must not
        // point the section at note 3 when the user saves the file.
        let parsed = parse_note_file("+++\npath = \"src/a.rs\"\n+++\n\n## gotcha @ abc1234\n<!-- note: 7 author: harness:codex reinforced: 0 -->\n**Watch the order of mounts**\n<!-- note: 3 -->\nNested mounts come last.\n").unwrap();

        assert_eq!(parsed.sections[0].note_id, Some(7));
        assert_eq!(
            parsed.sections[0].body,
            "<!-- note: 3 -->\nNested mounts come last."
        );
    }

    #[test]
    fn broken_files_say_where() {
        for (text, expected) in [
            ("no header", "starts with a +++ line"),
            ("+++\npath = \n+++\n", "not valid TOML"),
            ("+++\npath = \"a\"\n+++\n\n## todo\n**x**\n", "line 2"),
            (
                "+++\npath = \"a\"\n+++\n\nstray text\n",
                "outside a section",
            ),
            (
                "+++\npath = \"a\"\n+++\n\n## purpose\nno bold summary\n",
                "summary in bold",
            ),
        ] {
            let error = parse_note_file(text).unwrap_err();
            assert!(
                format!("{error:#}").contains(expected),
                "{text:?}: {error:#}"
            );
        }
    }

    #[test]
    fn files_go_under_notes_mirroring_the_repository() {
        let notes = Path::new("notes");

        assert_eq!(
            file_notes_path(notes, &docker()),
            notes.join("src").join("adapters").join("docker.rs.md")
        );
        assert_eq!(
            topic_path(notes, Some("feature/podman"), "runtime"),
            Some(
                notes
                    .join("_branches")
                    .join("feature")
                    .join("podman")
                    .join("runtime.md")
            )
        );
        assert_eq!(
            topic_path(notes, None, "errors"),
            Some(notes.join("_project").join("errors.md"))
        );
        assert_eq!(topic_path(notes, Some("../escape"), "x"), None);
    }

    #[test]
    fn sync_writes_and_removes_files() {
        let directory = tempfile::tempdir().unwrap();
        let mut database = database();

        sync_file_notes(directory.path(), &database, &docker()).unwrap();
        let file = file_notes_path(directory.path(), &docker());
        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("Every container runs read-only")
        );

        for id in [1, 2, 3] {
            database
                .apply(
                    &Event::NoteDeleted {
                        note: NoteRef::File(id),
                    },
                    AT,
                )
                .unwrap();
        }
        sync_file_notes(directory.path(), &database, &docker()).unwrap();
        assert!(!file.exists());
    }
}
