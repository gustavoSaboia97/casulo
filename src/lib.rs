//! casulo: run any AI coding agent in a hardened, disposable container.
//!
//! The code lives in this library so `main.rs` stays a small composition root
//! and every module can be tested on its own.

pub mod api;
pub mod application;
pub mod cli;
pub mod infrastructure;
