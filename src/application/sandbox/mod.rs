//! The container an agent runs in: `casulo run`, `shell`, `build` and
//! `compose`, with its mounts, masks, push rules, image and memory
//! container.

pub mod domain;
pub mod ports;
pub mod services;
pub mod test_support;
