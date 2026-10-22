//! Rules shared by the names casulo uses for directories and container images.

use anyhow::{Result, bail};

/// Longest name accepted, short enough for directory and image names everywhere.
const MAX_LENGTH: usize = 63;

/// Names Windows reserves for devices, which cannot be used as directory names.
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Checks that `value` is safe to use as a directory name on every platform and
/// as part of a container image name.
///
/// `kind` names what is being checked (for example "project name") in the error.
pub(super) fn check(kind: &str, value: &str) -> Result<()> {
    let allowed_characters = value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    });
    let starts_and_ends_with_alphanumeric =
        value.starts_with(is_alphanumeric) && value.ends_with(is_alphanumeric);

    if value.is_empty()
        || value.len() > MAX_LENGTH
        || !allowed_characters
        || !starts_and_ends_with_alphanumeric
    {
        bail!(
            "invalid {kind} \"{value}\": use lowercase letters, digits and hyphens, \
             starting and ending with a letter or digit, at most {MAX_LENGTH} characters"
        );
    }
    if WINDOWS_RESERVED_NAMES.contains(&value) {
        bail!("invalid {kind} \"{value}\": the name is reserved on Windows, choose another one");
    }
    Ok(())
}

fn is_alphanumeric(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_letters_digits_and_hyphens() {
        for value in ["casulo", "my-project", "a", "project2", "2fast"] {
            assert!(check("name", value).is_ok(), "{value} should be valid");
        }
    }

    #[test]
    fn accepts_the_longest_allowed_name() {
        assert!(check("name", &"a".repeat(MAX_LENGTH)).is_ok());
    }

    #[test]
    fn rejects_names_that_are_unsafe_or_ambiguous() {
        let invalid = [
            "",
            "My-Project",
            "-leading",
            "trailing-",
            "has space",
            "has_underscore",
            "has.dot",
            "..",
            "../escape",
            "nested/path",
            "back\\slash",
            "ação",
        ];
        for value in invalid {
            assert!(check("name", value).is_err(), "{value:?} should be invalid");
        }
    }

    #[test]
    fn rejects_names_longer_than_the_limit() {
        assert!(check("name", &"a".repeat(MAX_LENGTH + 1)).is_err());
    }

    #[test]
    fn rejects_windows_reserved_names() {
        for value in ["con", "nul", "com1", "lpt9"] {
            assert!(check("name", value).is_err(), "{value} should be invalid");
        }
    }

    #[test]
    fn error_names_the_kind_and_the_value() {
        let error = check("project name", "Bad").expect_err("uppercase is invalid");

        assert!(
            error
                .to_string()
                .starts_with("invalid project name \"Bad\"")
        );
    }
}
