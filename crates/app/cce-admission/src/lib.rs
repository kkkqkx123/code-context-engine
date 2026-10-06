//! Optional admission layer for remote hosting.
//!
//! This crate concentrates every request admission concern that only exists
//! in the remote hosting shape: bearer token verification, token to project
//! scope binding, per-token rate limiting, request body bounds, and public
//! path bypass. The local single-host shape never depends on this crate, so
//! none of these symbols exist in the default build.
//!
//! The store never logs raw tokens. Hashes are compared in constant time and
//! only short non-sensitive fingerprints appear in logs.

pub mod config;
pub mod context;
pub mod metrics;
pub mod middleware;
pub mod token;

pub use config::{AdmissionConfig, TokenEntry};
pub use context::{AdmissionContext, is_loopback_host, is_public_path, requires_admission};
pub use metrics::{AdmissionMetrics, AdmissionStats};
pub use middleware::{AdmissionGate, admission_middleware};
pub use token::TokenStore;
