//! Shared engine/runtime/context/maintenance for the Code Context Engine.
//!
//! This crate holds the engine facade (`CodeContextEngine`), the runtime
//! support modules, the shared application context (`SharedContext`), and the
//! index maintenance service. Both the HTTP server (`cce-server`) and the MCP
//! server (`cce-mcp`) depend on this crate so that MCP is an independent
//! compile target that does not drag in the axum/HTTP layer.
//!
//! Public items are reachable only through their module paths; no top-level
//! re-exports are provided.

pub mod context;
pub mod engine;
pub mod maintenance;
pub mod runtime;
pub mod services;
