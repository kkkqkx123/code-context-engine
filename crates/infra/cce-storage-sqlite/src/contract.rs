//! Relation storage contract (phase 1 skeleton).
//!
//! Groups the business data operations that today live on the embedded
//! SQLite repositories: files, entities, chunks, detail mappings, file
//! summaries, checkpoints, project registry, index manifests, generation
//! overrides, and relation snapshots (including incremental reads).
//!
//! Transaction boundary: multi-table writes stay atomic through
//! `with_transaction`; batch document writes are idempotent per key
//! (insert-or-replace), so replaying a batch after a transient failure is
//! safe. Operational data (metric aggregation, in-process caches) stays
//! local and is intentionally outside this contract.

use std::sync::Arc;

use cce_types::StorageError;

use crate::SqliteClient;

/// Relation storage contract implemented by the local SQLite branch.
///
/// Phase 1 keeps the existing repositories as the internal implementation;
/// callers switch to this contract plus the backend enum in phase 2, and a
/// future relational branch implements the same surface in phase 3.
pub trait RelationStorage: Clone + Send + Sync + 'static {
    /// Open the per-project scoped database handle.
    fn for_project(&self, project_id: i64) -> Result<Arc<SqliteClient>, StorageError>;

    /// Run a multi-table atomic write; commit on success, rollback on error.
    fn with_transaction<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<R, StorageError>;

    /// Read an integer project metadata value.
    fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError>;

    /// Write an integer project metadata value.
    fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError>;

    /// Aggregate on-disk size across the main and per-project databases.
    fn db_size(&self) -> Result<u64, StorageError>;

    /// Remove a per-project database file (two-step semantics: evict the
    /// cached handle, then delete main plus WAL/SHM sidecars).
    fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Backend name for logging (`local` for the embedded branch).
    fn backend_name(&self) -> &'static str {
        "local"
    }

    /// Whether this branch keeps per-project database files.
    fn is_per_project_db(&self) -> bool {
        true
    }
}

impl RelationStorage for SqliteClient {
    fn for_project(&self, project_id: i64) -> Result<Arc<SqliteClient>, StorageError> {
        SqliteClient::for_project(self, project_id)
    }

    fn with_transaction<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<R, StorageError>,
    {
        SqliteClient::with_transaction(self, f)
    }

    fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        SqliteClient::project_meta_get_int(self, project_id, key)
    }

    fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        SqliteClient::project_meta_set_int(self, project_id, key, value)
    }

    fn db_size(&self) -> Result<u64, StorageError> {
        SqliteClient::db_size(self)
    }

    fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        SqliteClient::delete_project_db(self, project_id)
    }
}

/// File-hash cache view port.
///
/// Exposes the generation-scoped hash view without leaking the underlying
/// three-table join used by the local implementation.
pub trait FileHashCachePort: Send + Sync + 'static {
    /// Resolve the active epoch for a project.
    fn active_epoch(&self) -> Result<Option<i64>, StorageError>;
}

impl FileHashCachePort for crate::cache::FileHashCache {
    fn active_epoch(&self) -> Result<Option<i64>, StorageError> {
        crate::cache::FileHashCache::active_epoch(self)
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_relation_storage<T: RelationStorage>() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_client_satisfies_relation_contract() {
        assert_relation_storage::<SqliteClient>();
        let client = SqliteClient::in_memory().expect("in-memory client");
        assert_eq!(RelationStorage::backend_name(&client), "local");
        assert!(RelationStorage::is_per_project_db(&client));
    }

    #[test]
    fn backend_combination_presets_hold() {
        use cce_config::global::DatabaseConfig;
        let local = DatabaseConfig::default();
        assert!(local.validate_backend_combination().is_ok());
        assert!(local.is_local_relation());
        assert!(local.is_local_fulltext());
        let remote = DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..DatabaseConfig::default()
        };
        assert!(remote.validate_backend_combination().is_err());
        let remote_ft = DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            ..DatabaseConfig::default()
        };
        assert!(remote_ft.validate_backend_combination().is_err());
    }
}
