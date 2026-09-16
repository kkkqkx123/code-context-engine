//! MCP (Model Context Protocol) server for the Code Context Engine.
//!
//! Independent compile target: depends on `cce-server-shared` (engine facade +
//! runtime + AppState) and `rmcp`, and never on the HTTP server crate. Tools
//! delegate to the shared engine so no business logic is duplicated.

pub mod handler;
pub mod server;

pub use handler::McpServerHandler;
pub use server::{build_state, run_http, run_http_from_config, run_stdio, run_stdio_from_config};
