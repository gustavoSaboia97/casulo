//! What the sandbox tests share: the built-in harnesses.
// The whole file is test code: it is compiled only for `cargo test`, and
// the architecture test does not check what it imports.
#![cfg(test)]

use crate::application::agent::domain::Harness;
use crate::application::shared::domain::HarnessName;
use crate::infrastructure::harness_files;

/// The built-in harness `name`, with no override.
pub fn built_in_harness(name: &str) -> Harness {
    let directory = tempfile::tempdir().unwrap();
    harness_files::find_harness(directory.path(), &HarnessName::parse(name).unwrap()).unwrap()
}
