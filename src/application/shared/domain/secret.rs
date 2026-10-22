//! Secret values kept in casulo's own config, such as an agent's login token.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A secret value. It prints as `***` in debug output, so it never reaches a
/// log or an error message by accident; `expose` is the only way to read it.
// `transparent` reads and writes it as a plain TOML string.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps `value` as a secret.
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    /// The value itself, to hand to a child process environment.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

// Written by hand instead of derived, so the value is never printed.
impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_hides_the_value() {
        let secret = Secret::new("sk-ant-oat01-value");

        assert_eq!(format!("{secret:?}"), "***");
        assert_eq!(secret.expose(), "sk-ant-oat01-value");
    }
}
