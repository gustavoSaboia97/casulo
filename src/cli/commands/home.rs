//! `casulo home`: prints the casulo home directory.

use anyhow::Result;

use crate::cli::environment;

/// `casulo home`.
pub fn home() -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    println!("{}", home.root().display());
    Ok(())
}
