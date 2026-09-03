//! Domain error types
//!
//! This module provides domain-specific error types organized by functional area.
//! These errors are used across multiple modules and represent high-level business concepts.
//!
//! The error structure follows a layered approach:
//! - Common errors: Base error types shared across the codebase (e.g., IO, NotFound, Config)
//! - Domain errors: High-level business errors for specific functional areas
//! - Module-specific errors: Implementation-specific errors kept within modules
//!
//! Note: QueryError has been moved to orchestrator::query::QueryError as it is
//! specific to the query module and no longer needs to be shared across modules.

pub mod bm25;
pub mod common;
pub mod config;
pub mod parse;
pub mod parse_string;
pub mod qdrant;
pub mod storage;

// Re-export config errors (the primary ConfigError lives here now)
pub use config::{ConfigError, ConfigValidationError};

// Re-export common errors for convenience
pub use common::{HttpError, IoError, JsonError, NotFoundError, TimeoutError};

// Re-export domain errors for convenience
pub use bm25::Bm25Error;
pub use parse::ParseError;
pub use parse_string::{
    ParseDomainError, ParseGroupRoleError, ParseRelationLevelError, ParseRelationTypeError,
};
pub use qdrant::QdrantError;
pub use storage::StorageError;
