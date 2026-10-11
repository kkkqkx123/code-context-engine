//! Application services shared by the HTTP and MCP surfaces.
//!
//! Transports keep wire validation and response rendering; every shared
//! behavior (query assembly, execution, conversion) lives in the modules
//! below exactly once.

pub mod search;
