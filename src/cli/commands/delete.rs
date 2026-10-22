//! `casulo delete`: a subagent, skill, MCP server or workflow.

use anyhow::{Result, bail};

use crate::application::agent::services::resources;
use crate::cli::args::DeleteArgs;
use crate::cli::environment;

/// `casulo delete`: asks first, unless `--yes`.
pub fn delete(arguments: DeleteArgs) -> Result<()> {
    let home = environment::casulo_home_from_environment()?;
    let (kind, name) = (arguments.resource.kind, &arguments.resource.name);
    if !home.resource_file(kind, name).is_file() {
        bail!("{kind} \"{name}\" does not exist; `casulo list {kind}` shows the ones you have");
    }
    let question = format!("delete {kind} \"{name}\"? [y/N] ");
    if !arguments.yes && !environment::ask(&mut std::io::stdin().lock(), &question)? {
        eprintln!("nothing deleted");
        return Ok(());
    }
    let removed = resources::delete(&home, kind, name)?;
    eprintln!("deleted {}", removed.display());
    Ok(())
}
