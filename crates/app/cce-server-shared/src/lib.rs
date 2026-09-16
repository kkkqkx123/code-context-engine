//! Shared engine/runtime/state/maintenance for the Code Context Engine.
//!
//! This crate holds the engine facade (`CodeContextEngine`), the runtime
//! support modules, the HTTP `AppState`, and the index maintenance service.
//! Both the HTTP server (`cce-server`) and the MCP server (`cce-mcp`) depend
//! on this crate so that MCP is an independent compile target that does not
//! drag in the axum/HTTP layer.

pub mod engine;
pub mod maintenance;
pub mod runtime;
pub mod state;
