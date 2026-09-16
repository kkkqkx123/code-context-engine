//! cce_server crate - HTTP server and engine facade
//!
//! The engine facade, runtime, AppState and maintenance service now live in
//! `cce-server-shared` so that the MCP server (`cce-mcp`) can reuse them as an
//! independent compile target. We re-export them here under the same paths so
//! the HTTP layer keeps compiling without changes.

pub use cce_server_shared::{engine, maintenance, runtime};

pub mod api;
pub mod logger;
