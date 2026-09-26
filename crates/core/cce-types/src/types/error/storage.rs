//! Storage domain error types
//!
//! This module defines error types related to storage operations across the codebase.

use super::bm25::Bm25Error;
use super::common::{IoError, NotFoundError};
use super::qdrant::QdrantError;
use thiserror::Error;

/// Storage error type for domain-specific storage operations
#[derive(Error, Debug)]
pub enum StorageError {
    /// Connection error
    #[error("Storage connection failed: {0}")]
    Connection(String),

    /// Query error
    #[error("Query failed: {0}")]
    Query(String),

    /// Insert error
    #[error("Insert into {table} failed: {reason}")]
    Insert { table: String, reason: String },

    /// Transaction error
    #[error("Transaction failed: {0}")]
    Transaction(String),

    /// Table error
    #[error("Table operation failed: {0}")]
    Table(String),

    /// Delete error
    #[error("Delete from {table} failed: {reason}")]
    Delete { table: String, reason: String },

    /// Update error
    #[error("Update {table} failed: {reason}")]
    Update { table: String, reason: String },

    /// Validation error
    #[error("Validation failed: {0}")]
    Validation(String),

    /// Not found - uses common NotFoundError
    #[error("{0}")]
    NotFound(#[from] NotFoundError),

    /// IO error - uses common IoError
    #[error("{0}")]
    Io(#[from] IoError),

    /// SQLite error
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// Storage space exhausted. Retrying cannot succeed until an operator
    /// frees disk space, so this is permanent and must alert.
    #[error("Storage full: {0}")]
    StorageFull(String),

    /// Epoch conflict — CAS check failed during publish.
    #[error("Epoch conflict: active_epoch={active} != base_epoch={base}")]
    EpochConflict {
        /// The active epoch at CAS check time
        active: i64,
        /// The base epoch the snapshot was built from
        base: i64,
    },

    /// Qdrant backend error (structured, classification delegated to `QdrantError`)
    #[error(transparent)]
    Qdrant(#[from] QdrantError),

    /// BM25 backend error (structured, classification delegated to `Bm25Error`)
    #[error(transparent)]
    Bm25(#[from] Bm25Error),
}

impl StorageError {
    /// Create a connection error
    pub fn connection(reason: impl Into<String>) -> Self {
        Self::Connection(reason.into())
    }

    /// Create a query error
    pub fn query(reason: impl Into<String>) -> Self {
        Self::Query(reason.into())
    }

    /// Create an insert error
    pub fn insert(table: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Insert {
            table: table.into(),
            reason: reason.into(),
        }
    }

    /// Create a transaction error
    pub fn transaction(reason: impl Into<String>) -> Self {
        Self::Transaction(reason.into())
    }

    /// Create a table error
    pub fn table(reason: impl Into<String>) -> Self {
        Self::Table(reason.into())
    }

    /// Create a delete error
    pub fn delete(table: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Delete {
            table: table.into(),
            reason: reason.into(),
        }
    }

    /// Create an update error
    pub fn update(table: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Update {
            table: table.into(),
            reason: reason.into(),
        }
    }

    /// Create a validation error
    pub fn validation(reason: impl Into<String>) -> Self {
        Self::Validation(reason.into())
    }

    /// Create a sqlite error
    pub fn sqlite(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        if is_storage_full_message(&reason) {
            return Self::StorageFull(reason);
        }
        Self::Sqlite(reason)
    }

    /// Create a storage-full error
    pub fn storage_full(reason: impl Into<String>) -> Self {
        Self::StorageFull(reason.into())
    }

    /// Whether the failure is caused by exhausted storage space.
    pub fn is_storage_full(&self) -> bool {
        match self {
            Self::StorageFull(_) => true,
            Self::Io(err) => err.is_storage_full(),
            Self::Qdrant(err) => err.is_storage_full(),
            Self::Bm25(err) => err.is_storage_full(),
            _ => false,
        }
    }

    /// Create an epoch conflict error
    pub fn epoch_conflict(active: i64, base: i64) -> Self {
        Self::EpochConflict { active, base }
    }

    /// Create a not found error
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound(NotFoundError::new(resource))
    }

    /// Get error code for programmatic error handling
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Connection(_) => "STORAGE_CONNECTION_ERROR",
            Self::Query(_) => "STORAGE_QUERY_ERROR",
            Self::Insert { .. } => "STORAGE_INSERT_ERROR",
            Self::Transaction(_) => "STORAGE_TRANSACTION_ERROR",
            Self::Table(_) => "STORAGE_TABLE_ERROR",
            Self::Delete { .. } => "STORAGE_DELETE_ERROR",
            Self::Update { .. } => "STORAGE_UPDATE_ERROR",
            Self::Validation(_) => "STORAGE_VALIDATION_ERROR",
            Self::NotFound(_) => "STORAGE_NOT_FOUND_ERROR",
            Self::Io(_) => "STORAGE_IO_ERROR",
            Self::Sqlite(_) => "STORAGE_SQLITE_ERROR",
            Self::StorageFull(_) => "STORAGE_STORAGE_FULL_ERROR",
            Self::EpochConflict { .. } => "STORAGE_EPOCH_CONFLICT",
            Self::Qdrant(_) => "STORAGE_QDRANT_ERROR",
            Self::Bm25(_) => "STORAGE_BM25_ERROR",
        }
    }
}

// Implement From<std::io::Error> for StorageError via IoError
impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        let wrapped = IoError::from(err);
        if wrapped.is_storage_full() {
            return Self::StorageFull(wrapped.to_string());
        }
        Self::Io(wrapped)
    }
}

fn is_storage_full_message(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("no space")
        || lowered.contains("storage full")
        || lowered.contains("enospc")
        || lowered.contains("disk full")
}

impl super::common::ErrorClassify for StorageError {
    fn is_retryable(&self) -> bool {
        if self.is_storage_full() {
            return false;
        }
        // Backend errors delegate to their own classification; the remaining
        // variants are retryable when they are connection failures, query-time
        // failures, sqlite runtime faults (busy/locked), or write conflicts
        // that can succeed on a subsequent attempt (epoch conflicts resolve
        // once the caller retries with a newer base snapshot).
        match self {
            Self::Qdrant(err) => err.is_retryable(),
            Self::Bm25(err) => err.is_retryable(),
            Self::Io(err) => err.is_retryable(),
            Self::StorageFull(_) => false,
            _ => matches!(
                self,
                Self::Connection(_)
                    | Self::Query(_)
                    | Self::Insert { .. }
                    | Self::Update { .. }
                    | Self::Transaction(_)
                    | Self::Sqlite(_)
                    | Self::EpochConflict { .. }
            ),
        }
    }

    fn is_transient(&self) -> bool {
        if self.is_storage_full() {
            return false;
        }
        match self {
            Self::Qdrant(err) => err.is_transient(),
            Self::Bm25(err) => err.is_transient(),
            Self::Io(err) => err.is_transient(),
            _ => self.is_retryable(),
        }
    }

    fn is_permanent(&self) -> bool {
        if self.is_storage_full() {
            return true;
        }
        // Sqlite runtime failures (busy/locked/IO) are excluded: they can
        // succeed once the database frees up.
        match self {
            Self::Qdrant(err) => err.is_permanent(),
            Self::Bm25(err) => err.is_permanent(),
            _ => matches!(
                self,
                Self::NotFound(_)
                    | Self::Table(_)
                    | Self::Delete { .. }
                    | Self::Validation(_)
                    | Self::StorageFull(_)
            ),
        }
    }
}
