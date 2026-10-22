//! The command-line input layer: argument definitions, the environment,
//! one handler per command, and terminal output.

pub mod args;
pub mod commands;
mod environment;
mod output;

// Re-exported because `Cli` is what `main.rs` parses and what the help
// snapshots in tests/regression.rs walk.
pub use args::Cli;
