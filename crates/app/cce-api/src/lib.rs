//! Shared API wire contract for CCE CLI and Server.
//!
//! This crate holds only transport facts: wire models, path templates,
//! validation rules, and version data. It carries no HTTP client behavior,
//! no timeout setup, and no byte budget execution. Remote transmission lives
//! in `cce-transport`; local execution never depends on network semantics.

pub mod models;
pub mod paths;
