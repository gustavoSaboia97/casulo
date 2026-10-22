//! casulo memory: a memory of a project's code that survives sessions and is
//! the same for every harness. See `docs/memory.md`.
//!
//! It depends only on `application/shared`, and reaches its SQLite store through
//! the `MemoryStore` port. It returns data; printing stays in casulo's command line.

pub mod domain;
pub mod ports;
pub mod services;
pub mod test_support;
