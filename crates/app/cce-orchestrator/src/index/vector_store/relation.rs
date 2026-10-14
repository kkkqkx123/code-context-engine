use std::sync::Arc;

use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileRecord, GenerationOverride, ProjectIndexManifest,
    ProjectRecord, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_storage_metadb_pg::PostgresClient;
use cce_storage_metadb_sqlite::SqliteClient;
use cce_types::StorageError;

/// Relation backend holding one concrete implementation.
///
/// Local is the embedded SQLite branch (per-project database files) and the
/// only supported branch; the remote PostgreSQL client is forward
/// scaffolding and assembly rejects it at startup until the remote paths
/// are wired. Path rules, cache eviction, capacity stats, and the
/// project-delete two-step semantics stay inside the local client.
#[derive(Clone)]
pub enum RelationStore {
    /// Embedded SQLite repositories.
    Local(Arc<SqliteClient>),
    /// External PostgreSQL service.
    Remote(Arc<PostgresClient>),
}

impl RelationStore {
    /// Wrap a local SQLite client.
    pub fn local(client: Arc<SqliteClient>) -> Self {
        Self::Local(client)
    }

    /// Wrap a remote PostgreSQL client.
    pub fn remote(client: Arc<PostgresClient>) -> Self {
        Self::Remote(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether this is the remote backend.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// Backend name for logging (`local` or `remote`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Remote(_) => "remote",
        }
    }

    /// Borrow the local client when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the remote client when this is the remote branch.
    pub fn as_remote(&self) -> Option<&Arc<PostgresClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the local handle when this is the embedded branch.
    pub fn into_local(self) -> Option<Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the remote handle when this is the remote branch.
    pub fn into_remote(self) -> Option<Arc<PostgresClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Open the per-project scoped handle.
    ///
    /// The local branch opens the per-project database file; the remote
    /// branch carries project filtering per query, so scoping is a clone.
    pub fn for_project(&self, project_id: i64) -> Result<Self, StorageError> {
        match self {
            Self::Local(client) => Ok(Self::Local(client.for_project(project_id)?)),
            Self::Remote(client) => Ok(Self::Remote(client.clone())),
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// The remote branch is rejected at startup: only the local backend is
    /// supported, and refusing here keeps a remote selection from failing
    /// halfway through indexing or search.
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.relation_backend {
            cce_config::modules::RelationBackend::Local => {
                let client = SqliteClient::new(database.sqlite.clone()).map_err(|e| {
                    StorageError::query(format!("failed to open relation store: {e}"))
                })?;
                Ok(Self::Local(Arc::new(client)))
            }
            cce_config::modules::RelationBackend::Remote => Err(StorageError::validation(
                "remote relation backend is not supported yet: the supported matrix is local-only; remote branches are forward scaffolding",
            )),
        }
    }
}

impl std::fmt::Debug for RelationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "RelationStore::Local(..)"),
            Self::Remote(_) => write!(f, "RelationStore::Remote(..)"),
        }
    }
}

macro_rules! dispatch_relation {
    ($self:expr, $method:ident ($($arg:expr),*)) => {
        match $self {
            Self::Local(client) => RelationStorage::$method(client.as_ref(), $($arg),*).await,
            Self::Remote(client) => RelationStorage::$method(client.as_ref(), $($arg),*).await,
        }
    };
}

impl RelationStorage for RelationStore {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        dispatch_relation!(self, ensure_project(project_id, root_path))
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        dispatch_relation!(self, project_record(project_id))
    }

    async fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        dispatch_relation!(self, project_meta_get_int(project_id, key))
    }

    async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, project_meta_set_int(project_id, key, value))
    }

    async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        dispatch_relation!(
            self,
            manifest_begin_building(project_id, data_epoch, operation_id, input_fingerprint)
        )
    }

    async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            manifest_mark_candidate_ready(project_id, operation_id)
        )
    }

    async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        dispatch_relation!(
            self,
            manifest_activate(
                project_id,
                data_epoch,
                relation_epoch,
                operation_id,
                input_fingerprint
            )
        )
    }

    async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, manifest_mark_failed(project_id, operation_id, reason))
    }

    async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        dispatch_relation!(self, manifest_active(project_id))
    }

    async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, manifest_recycle_epoch(project_id, epoch))
    }

    async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, overrides_replace(project_id, epoch, overrides))
    }

    async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        dispatch_relation!(self, overrides_for_generation(project_id, epoch))
    }

    async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_upsert(project_id, epoch, files))
    }

    async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_delete_by_project_epoch(project_id, epoch))
    }

    async fn files_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_delete_by_project(project_id))
    }

    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_upsert(entities))
    }

    async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_delete_by_project_epoch(project_id, epoch))
    }

    async fn entities_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_delete_by_project(project_id))
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, entities_count(project_id, epoch))
    }

    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_upsert(chunks))
    }

    async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        dispatch_relation!(self, chunks_by_ids(project_id, chunk_ids, epochs))
    }

    async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_project_epoch(project_id, epoch))
    }

    async fn chunks_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_project(project_id))
    }

    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, chunks_count(project_id, epoch))
    }

    async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_upsert(mappings))
    }

    async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_project_epoch(project_id, epoch))
    }

    async fn mappings_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_project(project_id))
    }

    async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, summary_upsert(file_id, epoch, summary_json))
    }

    async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        dispatch_relation!(self, summary_at_epoch(file_id, epoch))
    }

    async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        dispatch_relation!(self, summaries_by_epoch(project_id, epoch))
    }

    async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, summaries_delete_by_project_epoch(project_id, epoch))
    }

    async fn summaries_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, summaries_delete_by_project(project_id))
    }

    async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, checkpoint_create(project_id, checkpoint))
    }

    async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        dispatch_relation!(self, checkpoint_get(project_id, operation_id))
    }

    async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            checkpoint_set_status(project_id, operation_id, status)
        )
    }

    async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, checkpoint_update(project_id, checkpoint))
    }

    async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, file_checkpoint_upsert(project_id, file))
    }

    async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            file_checkpoint_get(project_id, operation_id, file_path)
        )
    }

    async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_files_delete_by_operation(project_id, operation_id)
        )
    }

    async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, work_unit_insert(record))
    }

    async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            work_unit_set_status(project_id, operation_id, stage, work_unit_hash, status)
        )
    }

    async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        dispatch_relation!(self, work_units_list(project_id, operation_id, stage))
    }

    async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            work_unit_by_hash(project_id, operation_id, stage, work_unit_hash)
        )
    }

    async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(
            self,
            snapshot_allocate(project_id, operation_id, config_fingerprint)
        )
    }

    async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &cce_types::CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            snapshot_write_ready(
                project_id,
                epoch,
                snapshot,
                input_fingerprint,
                snapshot_fingerprint
            )
        )
    }

    async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<cce_types::CanonicalRelationSnapshot, StorageError> {
        dispatch_relation!(self, snapshot_read(project_id, epoch))
    }

    async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<cce_types::RelationSnapshotManifest>, StorageError> {
        dispatch_relation!(self, snapshot_manifest(project_id, epoch))
    }

    async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<cce_types::SnapshotDelta>, StorageError> {
        dispatch_relation!(
            self,
            snapshot_delta_chain(project_id, after_epoch, up_to_epoch)
        )
    }

    async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        dispatch_relation!(self, snapshot_find_base(project_id, delta_epoch))
    }

    async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, snapshot_mark_failed(project_id, epoch, reason))
    }

    async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, snapshot_delete_epoch(project_id, epoch))
    }

    async fn snapshot_delete_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, snapshot_delete_project(project_id))
    }

    async fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            admission_record_admitted(fingerprint, projects, quota_bytes, bytes)
        )
    }

    async fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            admission_record_rejection(fingerprint, projects, quota_bytes, reason)
        )
    }

    async fn admission_get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        dispatch_relation!(self, admission_get(fingerprint))
    }

    async fn admission_list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        dispatch_relation!(self, admission_list())
    }

    async fn db_size(&self) -> Result<u64, StorageError> {
        dispatch_relation!(self, db_size())
    }

    async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, delete_project_db(project_id))
    }

    fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(client) => RelationStorage::backend_name(client.as_ref()),
            Self::Remote(client) => RelationStorage::backend_name(client.as_ref()),
        }
    }

    fn is_per_project_db(&self) -> bool {
        match self {
            Self::Local(client) => RelationStorage::is_per_project_db(client.as_ref()),
            Self::Remote(client) => RelationStorage::is_per_project_db(client.as_ref()),
        }
    }
}

/// Backend-neutral relation diagnostics snapshot.
///
/// Mirrors [`VectorDiagnostics`]: both branches probe their database size.
/// Probe failures fold into the snapshot so handlers stay branch-free.
#[derive(Debug, Clone)]
pub struct RelationDiagnostics {
    /// Whether the backend is reachable and serving.
    pub reachable: bool,
    /// Backend version, when the backend exposes one.
    pub version: Option<String>,
    /// Aggregate on-disk size in bytes, when the probe succeeds.
    pub size_bytes: u64,
    /// Human-readable failure detail, when unhealthy.
    pub error: Option<String>,
}

impl RelationStore {
    /// Backend-neutral diagnostics snapshot for status endpoints.
    ///
    /// Never fails: probe errors are folded into the snapshot so handlers
    /// stay branch-free.
    pub async fn diagnose_summary(&self) -> RelationDiagnostics {
        match RelationStorage::db_size(self).await {
            Ok(size) => RelationDiagnostics {
                reachable: true,
                version: None,
                size_bytes: size,
                error: None,
            },
            Err(e) => RelationDiagnostics {
                reachable: false,
                version: None,
                size_bytes: 0,
                error: Some(format!("Diagnostic failed: {e}")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_store_satisfies_relation_contract() {
        cce_storage_common::assert_relation_storage::<RelationStore>();
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        assert_eq!(RelationStorage::backend_name(&store), "local");
        assert!(RelationStorage::is_per_project_db(&store));
    }

    #[tokio::test]
    async fn relation_diagnose_reports_local_size() {
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        let diag = store.diagnose_summary().await;
        assert!(diag.reachable);
        assert!(diag.error.is_none());
    }

    #[test]
    fn relation_store_local_branch_dispatch() {
        let root = Arc::new(SqliteClient::in_memory().expect("in-memory client"));
        let store = RelationStore::local(root);
        assert!(store.is_local());
        assert_eq!(store.backend_name(), "local");
        let scoped = store.for_project(1).expect("project scoping must work");
        assert!(scoped.is_local());
    }

    #[test]
    fn relation_store_remote_branch_rejected_at_startup() {
        let mut database = cce_config::global::DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(RelationStore::from_database_config(&database).is_err());
        database.relation_remote.url = Some("postgres://localhost:5432/cce".to_string());
        let err = RelationStore::from_database_config(&database)
            .expect_err("remote branch must fail at startup");
        assert!(
            err.to_string().contains("not supported yet"),
            "unexpected error: {err}"
        );
    }
}
