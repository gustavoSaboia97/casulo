//! The commit message rule of this repository (AGENTS.md, *Commits and PRs*):
//! a Conventional Commits subject, short, lowercase and without a period, a
//! short body and a sign-off.

/// The commit types, also the prefixes a branch name may take.
pub const TYPES: [&str; 8] = [
    "feat", "fix", "docs", "refactor", "test", "chore", "perf", "ci",
];

/// What a first line is: the subject of a commit or the title of a pull request.
pub enum Line {
    Subject,
    Title,
}

impl Line {
    fn name(&self) -> &'static str {
        match self {
            Line::Subject => "subject",
            Line::Title => "title",
        }
    }

    /// The longest line accepted. A subject must stay under 72 characters,
    /// and a squash merge appends " (#NNN)" to a title to make the subject.
    fn limit(&self) -> usize {
        match self {
            Line::Subject => 71,
            Line::Title => 64,
        }
    }

    /// Why the limit is what it is, when that is not obvious.
    fn limit_reason(&self) -> &'static str {
        match self {
            Line::Subject => "",
            Line::Title => " (a squash merge adds \" (#NNN)\")",
        }
    }
}

/// Every rule `line` breaks, each said so its author can fix it.
pub fn line_problems(line: &str, kind: Line) -> Vec<String> {
    let name = kind.name();
    let mut problems = Vec::new();
    match description(line) {
        None => problems.push(prefix_problem(name)),
        Some(text) => {
            let first_word = text.split(' ').next().unwrap_or(text);
            if first_word.starts_with(|character: char| character.is_uppercase()) {
                problems.push(format!(
                    "{name} starts with a capital letter: write \"{}\", not \"{first_word}\"",
                    first_word.to_lowercase()
                ));
            }
        }
    }
    if line.ends_with('.') {
        problems.push(format!("{name} ends with a period: remove it"));
    }
    let length = line.chars().count();
    if length > kind.limit() {
        problems.push(format!(
            "{name} has {length} characters, the limit is {}{}",
            kind.limit(),
            kind.limit_reason()
        ));
    }
    problems
}

/// The text after a valid `<type>(<scope>)!: ` prefix, or `None` when the
/// prefix is missing or wrong.
fn description(line: &str) -> Option<&str> {
    let (prefix, text) = line.split_once(": ")?;
    let prefix = prefix.strip_suffix('!').unwrap_or(prefix);
    let commit_type = match prefix.split_once('(') {
        None => prefix,
        Some((commit_type, scope)) => {
            let scope = scope.strip_suffix(')')?;
            let scope_is_valid = !scope.is_empty()
                && scope.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || "/_-".contains(character)
                });
            if !scope_is_valid {
                return None;
            }
            commit_type
        }
    };
    let starts_with_text = text.starts_with(|character: char| !character.is_whitespace());
    (TYPES.contains(&commit_type) && starts_with_text).then_some(text)
}

/// The problem of a line without a valid `<type>(<scope>)!: ` prefix.
fn prefix_problem(name: &str) -> String {
    format!(
        "{name} must start with \"<type>: \" or \"<type>(<scope>): \", where <type> is one of {}",
        TYPES.join(", ")
    )
}

/// The problem of a message without a `Signed-off-by:` trailer.
const SIGN_OFF_PROBLEM: &str = "missing Signed-off-by: commit with git commit --signoff";

/// git's scissors line: with `git commit -v`, the diff below it is not part
/// of the message.
const SCISSORS: &str = "# ------------------------ >8 ------------------------";

/// The longest body accepted, trailers apart: three lines of 72.
const BODY_LIMIT: usize = 216;

/// The trailers the body length does not count, lowercase: git compares
/// trailer keys ignoring case, and GitHub writes `Co-authored-by`.
const TRAILER_KEYS: [&str; 2] = ["signed-off-by:", "co-authored-by:"];

/// A commit message split into the parts the rules look at.
struct Message {
    subject: String,
    body: Vec<String>,
    trailers: Vec<String>,
}

impl Message {
    /// Reads a message as git hands it to the commit-msg hook, dropping
    /// what git drops when it commits: comment lines, blank lines and the
    /// diff below the scissors line.
    fn parse(text: &str) -> Message {
        let mut lines = text
            .lines()
            .take_while(|line| *line != SCISSORS)
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty());
        let subject = lines.next().unwrap_or_default().to_string();
        let (trailers, body): (Vec<String>, Vec<String>) =
            lines.map(str::to_string).partition(|line| is_trailer(line));
        Message {
            subject,
            body,
            trailers,
        }
    }
}

fn is_trailer(line: &str) -> bool {
    let line = line.to_lowercase();
    TRAILER_KEYS.iter().any(|key| line.starts_with(key))
}

fn is_sign_off(trailer: &str) -> bool {
    trailer.to_lowercase().starts_with("signed-off-by:")
        && trailer.contains(" <")
        && trailer.ends_with('>')
}

/// Every rule the commit message `text` breaks.
pub fn message_problems(text: &str) -> Vec<String> {
    let message = Message::parse(text);
    let mut problems = Vec::new();
    // git writes a revert's subject itself, so the subject rules would only
    // get in the way of `git revert`.
    if !message.subject.starts_with("Revert \"") {
        problems.extend(line_problems(&message.subject, Line::Subject));
    }
    let body_length: usize = message.body.iter().map(|line| line.chars().count()).sum();
    if body_length > BODY_LIMIT {
        problems.push(format!(
            "body has {body_length} characters, the limit is {BODY_LIMIT}: keep it to three short lines on why"
        ));
    }
    if !message.trailers.iter().any(|trailer| is_sign_off(trailer)) {
        problems.push(SIGN_OFF_PROBLEM.to_string());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGN_OFF: &str = "Signed-off-by: Ana Lima <ana@example.com>";

    #[test]
    fn accepts_a_conventional_subject_with_a_scope() {
        // Given a subject with a type and a scope
        let subject = "feat(push): send the token over HTTPS";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn accepts_the_breaking_change_mark() {
        // Given a subject marked as a breaking change
        let subject = "refactor!: drop the old layout";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn accepts_capitals_after_the_first_letter() {
        // Given a subject naming a file in capitals
        let subject = "docs: describe AGENTS.md";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn accepts_a_subject_starting_with_code() {
        // Given a subject whose first character is not a letter
        let subject = "feat: `prek` hooks";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_subject_without_a_type() {
        // Given a subject with no prefix
        let subject = "added stuff";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the prefix rule is broken
        assert_eq!(problems, [prefix_problem("subject")]);
    }

    #[test]
    fn rejects_an_unknown_type() {
        // Given a subject with a type outside the list
        let subject = "wip: save the work";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the prefix rule is broken
        assert_eq!(problems, [prefix_problem("subject")]);
    }

    #[test]
    fn rejects_a_scope_with_capitals() {
        // Given a scope in capitals
        let subject = "feat(Push): send the token";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the prefix rule is broken
        assert_eq!(problems, [prefix_problem("subject")]);
    }

    #[test]
    fn rejects_a_subject_that_only_looks_like_a_merge() {
        // Given a plain commit named like a merge: merges are known by their
        // parents, not their text
        let subject = "Merge stuff";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the prefix rule is broken
        assert_eq!(problems, [prefix_problem("subject")]);
    }

    #[test]
    fn rejects_a_capital_after_the_type() {
        // Given a subject starting with a capital letter
        let subject = "feat: Add the hooks";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the lowercase rule is broken
        assert_eq!(
            problems,
            ["subject starts with a capital letter: write \"add\", not \"Add\""]
        );
    }

    #[test]
    fn rejects_a_trailing_period() {
        // Given a subject ending with a period
        let subject = "fix: handle the edge.";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then the period rule is broken
        assert_eq!(problems, ["subject ends with a period: remove it"]);
    }

    #[test]
    fn accepts_a_subject_of_71_characters() {
        // Given a subject at the limit
        let subject = format!("feat: {}", "a".repeat(65));

        // When it is checked
        let problems = line_problems(&subject, Line::Subject);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_subject_of_72_characters() {
        // Given a subject one character over the limit
        let subject = format!("feat: {}", "a".repeat(66));

        // When it is checked
        let problems = line_problems(&subject, Line::Subject);

        // Then the length rule is broken, with both numbers
        assert_eq!(problems, ["subject has 72 characters, the limit is 71"]);
    }

    #[test]
    fn reports_every_rule_broken() {
        // Given a subject breaking two rules
        let subject = "feat: Add the thing.";

        // When it is checked
        let problems = line_problems(subject, Line::Subject);

        // Then both rules are reported
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn accepts_the_subject_git_gives_a_revert() {
        // Given git's own subject for a revert, longer than the limit
        let message = format!("Revert \"feat: {}\"\n\n{SIGN_OFF}\n", "a".repeat(70));

        // When it is checked
        let problems = message_problems(&message);

        // Then the subject rules do not apply to it
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_revert_without_sign_off() {
        // Given a revert made without --signoff
        let message = "Revert \"feat: add the hooks\"\n\nThis reverts commit 1234567.\n";

        // When it is checked
        let problems = message_problems(message);

        // Then the sign-off rule is broken
        assert_eq!(problems, [SIGN_OFF_PROBLEM]);
    }

    #[test]
    fn accepts_a_signed_off_subject_without_a_body() {
        // Given a subject and a sign-off
        let message = format!("feat: add the hooks\n\n{SIGN_OFF}\n");

        // When it is checked
        let problems = message_problems(&message);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_message_without_sign_off() {
        // Given a message made without --signoff
        let message = "feat: add the hooks\n";

        // When it is checked
        let problems = message_problems(message);

        // Then the sign-off rule is broken
        assert_eq!(problems, [SIGN_OFF_PROBLEM]);
    }

    #[test]
    fn accepts_a_body_of_216_characters_besides_the_trailers() {
        // Given three body lines of 72 characters, a sign-off and a co-author
        // in the case GitHub writes it
        let line = "a".repeat(72);
        let message = format!(
            "feat: add the hooks\n\n{line}\n{line}\n{line}\n\n{SIGN_OFF}\nCo-authored-by: A <a@example.com>\n"
        );

        // When it is checked
        let problems = message_problems(&message);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_body_of_217_characters() {
        // Given a body one character over the limit
        let message = format!("feat: add the hooks\n\n{}\n\n{SIGN_OFF}\n", "a".repeat(217));

        // When it is checked
        let problems = message_problems(&message);

        // Then the body rule is broken, with both numbers
        assert_eq!(
            problems,
            ["body has 217 characters, the limit is 216: keep it to three short lines on why"]
        );
    }

    #[test]
    fn checks_what_git_keeps_of_a_message_from_the_editor() {
        // Given a message as git hands it to the hook after the editor, with
        // leading blank lines, its comments and, with -v, the diff below the
        // scissors line
        let diff = "+".repeat(500);
        let message = format!(
            "\n\nfeat: add the hooks\n\n{}\n\n{SIGN_OFF}\n\n\
             # Please enter the commit message for your changes. Lines starting\n\
             # with '#' will be ignored, and an empty message aborts the commit.\n\
             # ------------------------ >8 ------------------------\n\
             # Do not modify or remove the line above.\n\
             diff --git a/a b/a\n{diff}\n",
            "a".repeat(200)
        );

        // When it is checked
        let problems = message_problems(&message);

        // Then only the subject, body and trailers count
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn accepts_a_conventional_title() {
        // Given a pull request title in the commit form
        let title = "feat: git hooks with prek";

        // When it is checked
        let problems = line_problems(title, Line::Title);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_the_title_github_makes_from_a_branch_name() {
        // Given the title GitHub suggested for ci/prek-hooks, as #75 had
        let title = "Ci/prek hooks";

        // When it is checked
        let problems = line_problems(title, Line::Title);

        // Then the prefix rule is broken
        assert_eq!(problems, [prefix_problem("title")]);
    }

    #[test]
    fn accepts_a_title_of_64_characters() {
        // Given a title at the limit
        let title = format!("feat: {}", "a".repeat(58));

        // When it is checked
        let problems = line_problems(&title, Line::Title);

        // Then it breaks no rule
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn rejects_a_title_of_65_characters() {
        // Given a title one character over the limit
        let title = format!("feat: {}", "a".repeat(59));

        // When it is checked
        let problems = line_problems(&title, Line::Title);

        // Then the length rule is broken, saying why titles are shorter
        assert_eq!(
            problems,
            ["title has 65 characters, the limit is 64 (a squash merge adds \" (#NNN)\")"]
        );
    }
}
