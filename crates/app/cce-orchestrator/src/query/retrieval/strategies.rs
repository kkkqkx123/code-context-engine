//! Retrieval strategies module
//!
//! This module provides pluggable retrieval strategies using enum-based static dispatch.
//! Each strategy implements a specific recall algorithm (dense or BM25).

// Concrete strategy implementations
pub mod bm25;
pub mod dense;

// Main strategy enum and factory (must be last to reference above modules)
mod strategy_enum;

pub use strategy_enum::{RecallAlgorithm, RetrievalStrategy};
