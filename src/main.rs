//! Entry point: parses the command line and runs the chosen command. The
//! handlers live in `casulo::cli::commands`.

use std::process::ExitCode;

use clap::Parser;

use casulo::cli::{Cli, commands};

fn main() -> ExitCode {
    match commands::run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            // `{:#}` prints the error with its context chain on one line.
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
