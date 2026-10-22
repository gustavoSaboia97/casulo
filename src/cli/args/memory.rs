//! Arguments of the internal `casulo memory` commands, which `casulo run`
//! starts in the memory container.

use std::path::PathBuf;

use clap::Subcommand;

/// Internal subcommands of `casulo memory`.
#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// Serve the memory to agents over MCP
    Serve(ServeArgs),
}

/// Arguments of `casulo memory serve`.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// The memory directory, holding memory.sqlite and notes/
    #[arg(long)]
    pub dir: PathBuf,
    /// The repository to index
    #[arg(long)]
    pub workspace: PathBuf,
    /// Address to listen on; the memory container uses 0.0.0.0 on its internal network
    #[arg(long, default_value = "127.0.0.1")]
    pub host: std::net::IpAddr,
    /// Port to listen on
    #[arg(long, default_value_t = 7373)]
    pub port: u16,
}
