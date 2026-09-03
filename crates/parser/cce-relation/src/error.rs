//! Error types for relation module
//!
//! Provides detailed error types for different failure scenarios.

use cce_types::EntityId;
use thiserror::Error;

/// Relation module error
#[derive(Error, Debug)]
pub enum RelationError {
    /// Index error
    #[error("Index error: {0}")]
    Index(#[from] IndexError),

    /// Query error
    #[error("Query error: {0}")]
    Query(#[from] RelationQueryError),

    /// Resolution error
    #[error("Resolution error: {0}")]
    Resolution(#[from] ResolutionError),

    /// Persistence error
    #[error("Persistence error: {0}")]
    Persistence(#[from] PersistenceError),
}

/// Index-related errors
#[derive(Error, Debug)]
pub enum IndexError {
    /// Entity not found
    #[error("Entity not found: {0:?}")]
    EntityNotFound(EntityId),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Inconsistent state
    #[error("Inconsistent state: {0}")]
    InconsistentState(String),

    /// Duplicate entity
    #[error("Duplicate entity: {0:?}")]
    DuplicateEntity(EntityId),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

impl IndexError {
    /// Create entity not found error
    pub fn entity_not_found(entity_id: EntityId) -> Self {
        Self::EntityNotFound(entity_id)
    }

    /// Create file not found error
    pub fn file_not_found(file_id: &str) -> Self {
        Self::FileNotFound(file_id.to_string())
    }

    /// Create inconsistent state error
    pub fn inconsistent_state(message: impl Into<String>) -> Self {
        Self::InconsistentState(message.into())
    }
}

/// Query-related errors (RelationQueryError to avoid conflict with orchestrator::query::error::QueryError)
#[derive(Error, Debug)]
pub enum RelationQueryError {
    /// Entity not found
    #[error("Entity not found: {0}")]
    NotFound(String),

    /// Invalid query parameters
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Traversal error
    #[error("Traversal error: {0}")]
    Traversal(String),

    /// Path not found
    #[error("Path not found from {from} to {to} (max depth: {max_depth})")]
    PathNotFound {
        from: String,
        to: String,
        max_depth: usize,
    },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Index not available
    #[error("Index not available: {0}")]
    IndexNotAvailable(String),
}

impl RelationQueryError {
    /// Create not found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// Create invalid query error
    pub fn invalid_query(message: impl Into<String>) -> Self {
        Self::InvalidQuery(message.into())
    }

    /// Create traversal error
    pub fn traversal(message: impl Into<String>) -> Self {
        Self::Traversal(message.into())
    }

    /// Create path not found error
    pub fn path_not_found(
        from: impl Into<String>,
        to: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        Self::PathNotFound {
            from: from.into(),
            to: to.into(),
            max_depth,
        }
    }

    /// Create internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Create index not available error
    pub fn index_not_available(index: impl Into<String>) -> Self {
        Self::IndexNotAvailable(index.into())
    }

    /// Create config error (alias for invalid_query)
    pub fn config(message: impl Into<String>) -> Self {
        Self::InvalidQuery(message.into())
    }

    /// Create invalid error (alias for invalid_query)
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidQuery(message.into())
    }
}

/// Resolution-related errors
#[derive(Error, Debug)]
pub enum ResolutionError {
    /// Symbol not found
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    /// Ambiguous symbol
    #[error("Ambiguous symbol: {symbol} (found {count} matches)")]
    AmbiguousSymbol { symbol: String, count: usize },

    /// Invalid import
    #[error("Invalid import: {0}")]
    InvalidImport(String),

    /// Circular dependency
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),
}

/// Persistence-related errors
#[derive(Error, Debug)]
pub enum PersistenceError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),
}
