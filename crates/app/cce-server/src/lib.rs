//! cce_server crate - HTTP server layer
//!
//! The engine facade, runtime, shared context (`SharedContext`) and
//! maintenance service live in `cce-server-shared` and are used directly
//! from there; this crate only provides the axum/HTTP transport.

pub mod api;
pub use cce_logging as logger;
