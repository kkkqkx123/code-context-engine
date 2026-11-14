//! Export error types
//!
//! This module provides error types for the export functionality.

use std::path::PathBuf;
use thiserror::Error;

/// Export error type
#[derive(Debug, Error)]
pub enum ExportError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid source path
    #[error("Invalid source path: {0}")]
    InvalidSourcePath(PathBuf),

    /// No chunks to export
    #[error("No chunks to export")]
    NoChunks,

    /// Formatter error
    #[error("Formatter error: {0}")]
    Formatter(String),

    /// Aggregation error
    #[error("Aggregation error: {0}")]
    Aggregation(String),

    /// Path computation error
    #[error("Path computation error: {0}")]
    PathComputation(String),

    /// Relation enhancement error
    #[error("Relation enhancement error: {0}")]
    RelationEnhancement(String),
}
