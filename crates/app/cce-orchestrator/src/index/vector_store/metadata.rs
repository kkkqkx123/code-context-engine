//! Metadata persistence handle used by the orchestrator.
//!
//! The handle selects one concrete backend at assembly time and shares the
//! same instance across indexing, query, hot update, checkpoint, export and
//! project registry paths.

use std::sync::Arc;

use cce_config::global::DatabaseConfig;
use cce_storage_common::metadb::MetadataBackendStore;
use cce_types::StorageError;

pub use cce_storage_metadb_sqlite::{MetadataDiagnostics, SqliteClient};

/// Deterministic metadata backend dispatch; never a trait object.
pub type MetadataStore =
    MetadataBackendStore<Arc<SqliteClient>, Arc<cce_storage_metadb_pg::PostgresClient>>;

pub fn in_memory_metadata_store() -> Result<MetadataStore, StorageError> {
    Ok(MetadataBackendStore::Local(Arc::new(
        SqliteClient::in_memory()?,
    )))
}

pub async fn open_metadata_store(database: &DatabaseConfig) -> Result<MetadataStore, StorageError> {
    match database.metadata_backend {
        cce_config::modules::MetadataBackend::Local => Ok(MetadataBackendStore::Local(Arc::new(
            SqliteClient::new(database.sqlite.clone())?,
        ))),
        cce_config::modules::MetadataBackend::Remote => {
            let client = Arc::new(cce_storage_metadb_pg::PostgresClient::from_database_config(
                database,
            )?);
            client.ensure_migrated().await?;
            Ok(MetadataBackendStore::Remote(client))
        }
    }
}
