//! Typed HTTP client for the CCE server.
//!
//! Every server operation is reachable through exactly one method on
//! [`Client`], which binds the path constant, the wire parameters, the request
//! body and the response type together. Callers never assemble URLs and never
//! decode payloads themselves.
//!
//! Scope rules:
//!
//! - The client performs one request per call. Retries, circuit breaking,
//!   health caching and scheduling belong to the caller.
//! - Wire types come from `cce-api`; this crate defines no DTOs of its own, so
//!   the Rust client, the OpenAPI snapshot and the TypeScript projection stay
//!   projections of one contract.
//! - Local (in-process) execution does not go through this crate.

mod config;
mod dead_letters;
mod entity;
mod error;
mod graph;
mod health;
mod index;
mod ingest;
mod metrics;
mod project;
mod qdrant;
mod search;
mod storage;
mod tools;
mod transport;
mod watch;

pub use error::{ApiMethod, ClientError};
pub use transport::{Client, ClientConfig, INTERACTIVE_BUDGET, LONG_TASK_BUDGET, TOKEN_ENV};
