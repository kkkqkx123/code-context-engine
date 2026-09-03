//! Qdrant error types
//!
//! Re-exported from `cce_types`: the backend error type lives in `cce_types`
//! so that `StorageError::Qdrant(QdrantError)` can wrap it structurally.

pub use cce_types::error::QdrantError;
