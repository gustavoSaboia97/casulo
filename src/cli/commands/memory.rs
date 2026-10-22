//! `casulo memory serve`: the memory server, run inside the memory
//! container.

use anyhow::{Context, Result};

use crate::cli::args::ServeArgs;

/// Runs the memory server until it is stopped. Called inside the memory
/// container, where `--dir` and `--workspace` are its mounts.
pub fn serve(arguments: ServeArgs) -> Result<()> {
    // `Arc` shares one value between threads; the server and the indexer
    // thread both hold the same git and clock.
    use std::sync::Arc;

    std::fs::create_dir_all(&arguments.dir)
        .with_context(|| format!("cannot create {}", arguments.dir.display()))?;
    let database_file = arguments.dir.join("memory.sqlite");
    let database = crate::infrastructure::sqlite::Database::open(&database_file)?;
    let server = Arc::new(crate::api::memory_mcp::Server::new(
        database,
        Some(database_file),
        arguments.workspace,
        arguments.dir.join("notes"),
        Arc::new(crate::infrastructure::git::GitCli),
        Arc::new(crate::infrastructure::clock::SystemClock),
    ));
    let address = std::net::SocketAddr::new(arguments.host, arguments.port);
    let listener = std::net::TcpListener::bind(address)
        .with_context(|| format!("cannot listen on {address}"))?;
    eprintln!("memory: listening on {address}");
    crate::api::memory_mcp::start_indexing(
        Arc::clone(&server),
        Arc::new(crate::infrastructure::tree_sitter_rust::TreeSitterRust),
    )?;
    crate::api::memory_mcp::serve(server, listener)
}
