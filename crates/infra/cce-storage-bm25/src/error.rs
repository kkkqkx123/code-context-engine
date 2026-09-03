//! BM25 error types
//!
//! Re-exported from `cce_types`: the backend error type lives in `cce_types`
//! so that `StorageError::Bm25(Bm25Error)` can wrap it structurally.

pub use cce_types::error::Bm25Error;
