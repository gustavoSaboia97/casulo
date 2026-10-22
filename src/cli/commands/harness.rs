//! `casulo harness list`: the available harnesses and what each receives.

use anyhow::Result;

use crate::cli::environment;
use crate::infrastructure::harness_files::{self, HarnessSource};

/// `casulo harness list`.
pub fn list() -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    for loaded in harness_files::load_harnesses(&home.harnesses_dir())? {
        let source = match loaded.source {
            HarnessSource::BuiltIn => "built-in".to_string(),
            HarnessSource::Override(path) => path.display().to_string(),
        };
        let pieces = loaded.harness.supported_pieces();
        let supports = if pieces.is_empty() {
            "nothing injected".to_string()
        } else {
            pieces.join(",")
        };
        println!("{}\t{source}\t{supports}", loaded.harness.name);
    }
    Ok(())
}
