//! Terminal output of the memory commands.

use crate::application::memory::domain::{NoteStatus, StoredProjectNote};
use crate::application::memory::services::file_details::FileDetails;
use crate::application::memory::services::user_notes::{NoteRow, RowState};

/// One line of `casulo list memory note`, tab-separated for scripts.
pub fn format_note_row(row: &NoteRow) -> String {
    let state = match row.state {
        RowState::Ok => "ok",
        RowState::Stale => "stale",
        RowState::Conflict => "conflict",
        RowState::OtherVersion => "other-version",
    };
    format!(
        "{}\t{}\t{state}\t{}\t{}\n",
        row.path,
        row.kind,
        row.author.to_text(),
        row.summary
    )
}

/// One line of `casulo list memory topic`, tab-separated for scripts.
pub fn format_topic_row(note: &StoredProjectNote) -> String {
    let scope = note.branch.as_deref().unwrap_or("general");
    let conflict = if note.conflict.is_some() {
        "\tconflict"
    } else {
        ""
    };
    format!(
        "{}\t{}\t{scope}\t{}\t{}{conflict}\n",
        note.topic,
        note.kind,
        note.author.to_text(),
        note.text.summary.as_str()
    )
}

/// The text `casulo describe memory note` prints for a file.
pub fn format_file_details(details: &FileDetails) -> String {
    let mut text = format!("{}\n", details.path);
    match &details.blob_hash {
        Some(hash) => text.push_str(&format!("  content  {}\n", short_hash(hash))),
        None => text.push_str("  not in the checkout; its notes are kept for other branches\n"),
    }
    text.push_str("links to\n");
    push_edges(&mut text, &details.links_to);
    text.push_str("linked from\n");
    push_edges(&mut text, &details.linked_from);
    text.push_str("notes\n");
    if details.notes.is_empty() {
        text.push_str("  (none)\n");
    }
    for note in &details.notes {
        let status = match note.status {
            Some(NoteStatus::Ok) => "ok",
            Some(NoteStatus::Stale) => "stale",
            Some(NoteStatus::Conflict) => "conflict",
            None => "other branch",
        };
        text.push_str(&format!(
            "  {:<10} {:<12} {:<14} {}\n",
            note.kind.as_str(),
            status,
            note.author.to_text(),
            note.summary
        ));
    }
    text
}

fn push_edges(text: &mut String, edges: &[crate::application::memory::domain::event::Edge]) {
    if edges.is_empty() {
        text.push_str("  (none)\n");
    }
    for edge in edges {
        text.push_str(&format!("  {:<9} {}\n", edge.kind.as_str(), edge.to));
    }
}

/// The first 7 characters of a hash, as git shows them.
fn short_hash(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use crate::application::memory::domain::event::Edge;
    use crate::application::memory::domain::{Author, EdgeKind, NoteKind, RepoPath};
    use crate::application::memory::services::file_details::NoteLine;

    use super::*;

    fn path(value: &str) -> RepoPath {
        RepoPath::parse(value).unwrap()
    }

    #[test]
    fn file_details_text() {
        let details = FileDetails {
            path: path("src/main.rs"),
            blob_hash: Some("9f3c1e0a2b4d6f8".to_string()),
            links_to: vec![
                Edge {
                    to: path("src/cli.rs"),
                    kind: EdgeKind::Contains,
                },
                Edge {
                    to: path("src/domain/config.rs"),
                    kind: EdgeKind::Imports,
                },
            ],
            linked_from: Vec::new(),
            notes: vec![NoteLine {
                kind: NoteKind::Purpose,
                status: Some(NoteStatus::Stale),
                author: Author::Harness("claude".to_string()),
                summary: "Composition root: builds the adapters".to_string(),
            }],
        };

        insta::assert_snapshot!(format_file_details(&details));
    }
}
