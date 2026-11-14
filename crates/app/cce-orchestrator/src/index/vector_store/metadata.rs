//! Metadata storage surface used by the orchestrator.
//!
//! The store is the embedded SQLite client directly; no backend enum
//! remains. This module keeps one import site for assembly and query paths.

use std::sync::Arc;

use cce_types::StorageError;

pub use cce_storage_metadb_sqlite::{MetadataDiagnostics, SqliteClient};

pub type MetadataStore = Arc<SqliteClient>;

pub fn in_memory_metadata_store() -> Result<MetadataStore, StorageError> {
    Ok(Arc::new(SqliteClient::in_memory()?))
}

pub fn open_metadata_store(
    sqlite: cce_config::global::SqliteConfig,
) -> Result<MetadataStore, StorageError> {
    Ok(Arc::new(SqliteClient::new(sqlite)?))
}
