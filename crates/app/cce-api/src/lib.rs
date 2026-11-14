//! Shared API wire contract for CCE CLI and Server.
//!
//! This crate holds only transport facts: wire models, path templates and
//! error codes. It carries no HTTP client behavior, no timeout setup, and no
//! byte budget execution. Remote transmission lives in `cce-client`; local
//! execution never depends on network semantics.
//!
//! Schema derives stay here on purpose. This crate is the single source for
//! the server OpenAPI document and the generated TypeScript projection. Moving
//! schema derives into `cce-client` would force the server to depend on the
//! client transport for types and would duplicate the models the client
//! deliberately reuses from here.

pub mod models;
pub mod paths;
