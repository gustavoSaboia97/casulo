//! Use cases of the sandbox group. They hold the logic and reach the
//! outside world only through ports.

pub mod build;
pub mod compose;
pub mod inject;
pub mod mask;
pub mod memory_container;
pub mod push;
pub mod replace_files;
pub mod run;
pub mod run_plan;
