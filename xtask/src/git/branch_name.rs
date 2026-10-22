//! The branch name rule of this repository (AGENTS.md, *Commits and PRs*):
//! `<type>/<description>`, such as `feat/skill-files`.

use super::commit_message::TYPES;

/// The problem with the name `branch`, or `None` when it follows the rule.
pub fn branch_problem(branch: &str) -> Option<String> {
    let follows_the_rule = match branch.split_once('/') {
        Some((branch_type, description)) => {
            TYPES.contains(&branch_type) && is_words_joined_by_hyphens(description)
        }
        None => false,
    };
    if follows_the_rule {
        return None;
    }
    Some(format!(
        "branch \"{branch}\" must be named <type>/<description>, such as feat/skill-files, \
         where <type> is one of {} and the description is lowercase words joined by \"-\": \
         rename it with git branch -m <new-name>",
        TYPES.join(", ")
    ))
}

fn is_words_joined_by_hyphens(text: &str) -> bool {
    text.split('-').all(|word| {
        !word.is_empty()
            && word
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_type_and_a_description() {
        // Given a branch named after its change
        let branch = "feat/skill-files";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it follows the rule
        assert_eq!(problem, None);
    }

    #[test]
    fn accepts_digits_in_the_description() {
        // Given a description with a number
        let branch = "fix/windows-mount-2";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it follows the rule
        assert_eq!(problem, None);
    }

    #[test]
    fn rejects_a_prefix_that_is_not_a_commit_type() {
        // Given a branch named the way Claude Code sessions name theirs
        let branch = "claude/project-thread-x";

        // When it is checked
        let problem = branch_problem(branch);

        // Then the problem says the rule and how to rename the branch
        assert_eq!(
            problem.as_deref(),
            Some(
                "branch \"claude/project-thread-x\" must be named <type>/<description>, \
                 such as feat/skill-files, where <type> is one of feat, fix, docs, refactor, \
                 test, chore, perf, ci and the description is lowercase words joined by \"-\": \
                 rename it with git branch -m <new-name>"
            )
        );
    }

    #[test]
    fn rejects_a_name_without_a_type() {
        // Given a branch with no prefix
        let branch = "main";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it breaks the rule
        assert!(problem.is_some());
    }

    #[test]
    fn rejects_capitals_and_underscores_in_the_description() {
        // Given a description that is not lowercase words joined by hyphens
        let branch = "feat/Skill_files";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it breaks the rule
        assert!(problem.is_some());
    }

    #[test]
    fn rejects_an_empty_description() {
        // Given a type and nothing after it
        let branch = "feat/";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it breaks the rule
        assert!(problem.is_some());
    }

    #[test]
    fn rejects_a_hyphen_at_the_edge_of_the_description() {
        // Given a description starting with a hyphen
        let branch = "feat/-skill";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it breaks the rule
        assert!(problem.is_some());
    }

    #[test]
    fn rejects_a_nested_description() {
        // Given a description with another slash
        let branch = "feat/skill/files";

        // When it is checked
        let problem = branch_problem(branch);

        // Then it breaks the rule
        assert!(problem.is_some());
    }
}
