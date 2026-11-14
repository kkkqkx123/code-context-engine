//! Metadata backend facade.
//!
//! Owns the relation backend selection, local handle construction (project
//! databases and in-memory databases), contract dispatch, and diagnostics.
//! Orchestration layers depend on this facade instead of a concrete branch
//! so no SQL, connection, or transaction type leaks past assembly.

use std::sync::Arc;

use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileEpochEntry, FileRecord, GenerationOverride,
    NewProjectRecord, ProjectIndexManifest, ProjectRecord, ProjectUpdateRecord, RelationStorage,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_storage_metadb_sqlite::{SqliteClient, SqliteSnapshotStore};
use cce_types::{SnapshotDelta, StorageError};

/// Relation backend holding one concrete implementation.
///
/// The embedded SQLite branch (per-project database files) is the only
/// supported metadata backend. Path rules, cache eviction, capacity stats and
/// the project-delete two-step semantics stay inside the client.
#[derive(Clone)]
pub enum RelationStore {
    /// Embedded SQLite repositories.
    Local(Arc<SqliteClient>),
}

impl RelationStore {
    /// Wrap a local SQLite client.
    pub fn local(client: Arc<SqliteClient>) -> Self {
        Self::Local(client)
    }

    /// Build an in-memory store for tests and transient pipelines.
    pub fn in_memory() -> Result<Self, StorageError> {
        Ok(Self::Local(Arc::new(SqliteClient::in_memory()?)))
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Backend name for logging.
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
        }
    }

    /// Borrow the local client when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
        }
    }

    /// Unwrap into the local handle when this is the embedded branch.
    pub fn into_local(self) -> Option<Arc<SqliteClient>> {
        match self {
            Self::Local(client) => Some(client),
        }
    }

    /// Open the per-project scoped handle.
    ///
    /// The local branch opens the per-project database file.
    pub fn for_project(&self, project_id: i64) -> Result<Self, StorageError> {
        match self {
            Self::Local(client) => Ok(Self::Local(client.for_project(project_id)?)),
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// Only the local backend is supported; refusing a remote selection here
    /// keeps it from failing halfway through indexing or search.
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
                "remote relation backend is not wired yet: supported matrices are local/local/local and qdrant/local/local; set relation_backend = \"local\"",
            )),
        }
    }
}

impl std::fmt::Debug for RelationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "RelationStore::Local(..)"),
        }
    }
}

macro_rules! dispatch_relation {
    ($self:expr, $method:ident ($($arg:expr),*)) => {
        match $self {
            Self::Local(client) => RelationStorage::$method(client.as_ref(), $($arg),*).await,
        }
    };
}

impl RelationStorage for RelationStore {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        dispatch_relation!(self, ensure_project(project_id, root_path))
    }

    async fn projects_all(&self) -> Result<Vec<ProjectRecord>, StorageError> {
        dispatch_relation!(self, projects_all())
    }

    async fn project_update(
        &self,
        project_id: i64,
        updates: &ProjectUpdateRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, project_update(project_id, updates))
    }

    async fn project_insert(&self, record: &NewProjectRecord) -> Result<i64, StorageError> {
        dispatch_relation!(self, project_insert(record))
    }

    async fn project_path_exists(&self, root_path: &str) -> Result<bool, StorageError> {
        dispatch_relation!(self, project_path_exists(root_path))
    }

    async fn project_delete_with_cascade(&self, project_id: i64) -> Result<(), StorageError> {
        dispatch_relation!(self, project_delete_with_cascade(project_id))
    }

    async fn project_delete_db(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, project_delete_db(project_id))
    }

    async fn project_clear_index(&self, project_id: i64) -> Result<(), StorageError> {
        dispatch_relation!(self, project_clear_index(project_id))
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

    async fn project_meta_get_int_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<i64>, StorageError> {
        dispatch_relation!(self, project_meta_get_int_optional(project_id, key))
    }

    async fn project_meta_get_string_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<String>, StorageError> {
        dispatch_relation!(self, project_meta_get_string_optional(project_id, key))
    }

    async fn project_meta_set_string(
        &self,
        project_id: i64,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, project_meta_set_string(project_id, key, value))
    }

    async fn chunks_page_with_category(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError> {
        dispatch_relation!(
            self,
            chunks_page_with_category(project_id, epoch, limit, offset)
        )
    }

    async fn files_epoch_entries(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FileEpochEntry>, StorageError> {
        dispatch_relation!(self, files_epoch_entries(project_id, epoch))
    }

    async fn files_set_epoch(
        &self,
        project_id: i64,
        path: &str,
        epoch: i64,
    ) -> Result<bool, StorageError> {
        dispatch_relation!(self, files_set_epoch(project_id, path, epoch))
    }

    async fn checkpoint_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_cached_parsed_data(project_id, file_path, content_hash)
        )
    }

    async fn chunks_by_file_epoch(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        dispatch_relation!(self, chunks_by_file_epoch(project_id, file_path, epoch))
    }

    async fn chunks_by_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        dispatch_relation!(self, chunks_by_file(project_id, file_path, epoch))
    }

    async fn manifest_parent_epoch(
        &self,
        project_id: i64,
        data_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        dispatch_relation!(self, manifest_parent_epoch(project_id, data_epoch))
    }

    async fn files_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, files_count(project_id, epoch))
    }

    async fn files_exists_at_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<bool, StorageError> {
        dispatch_relation!(self, files_exists_at_epoch(project_id, epoch, path))
    }

    async fn files_page(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        dispatch_relation!(self, files_page(project_id, epoch, limit, offset))
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

    async fn manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        dispatch_relation!(self, manifest_by_operation(project_id, operation_id))
    }

    async fn manifest_building_max_epoch(
        &self,
        project_id: i64,
    ) -> Result<Option<i64>, StorageError> {
        dispatch_relation!(self, manifest_building_max_epoch(project_id))
    }

    async fn manifest_set_parent(
        &self,
        project_id: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            manifest_set_parent(project_id, operation_id, parent_data_epoch)
        )
    }

    async fn manifest_has_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
    ) -> Result<bool, StorageError> {
        dispatch_relation!(self, manifest_has_incomplete(project_id, operation_type))
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

    async fn overrides_upsert_batch(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, cce_storage_common::metadb::OverrideDisposition)],
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, overrides_upsert_batch(project_id, epoch, entries))
    }

    async fn overrides_clear(&self, project_id: i64, epoch: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, overrides_clear(project_id, epoch))
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

    async fn files_visible_hashes(
        &self,
        project_id: i64,
    ) -> Result<Vec<cce_storage_common::metadb::FileHashEntry>, StorageError> {
        dispatch_relation!(self, files_visible_hashes(project_id))
    }

    async fn files_record_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, String)],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_record_hashes(project_id, epoch, entries))
    }

    async fn files_remove_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_remove_hashes(project_id, epoch, paths))
    }

    async fn files_ensure(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        entries: &[cce_storage_common::metadb::FileEnsureEntry],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_ensure(project_id, epoch, batch_id, entries))
    }

    async fn files_delete_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, files_delete_by_path(project_id, path))
    }

    async fn files_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        dispatch_relation!(self, files_by_ids(project_id, ids))
    }

    async fn file_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<Option<FileRecord>, StorageError> {
        dispatch_relation!(self, file_by_path(project_id, path))
    }

    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_upsert(entities))
    }

    async fn entities_insert_returning_ids(
        &self,
        entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError> {
        dispatch_relation!(self, entities_insert_returning_ids(entities))
    }

    async fn entities_with_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<cce_storage_common::metadb::EntityWithFile>, StorageError> {
        dispatch_relation!(self, entities_with_files(project_id, epoch))
    }

    async fn entities_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        dispatch_relation!(self, entities_by_ids(project_id, ids))
    }

    async fn entities_by_file(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        dispatch_relation!(self, entities_by_file(project_id, file_id))
    }

    async fn entities_delete_by_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<bool, StorageError> {
        dispatch_relation!(self, entities_delete_by_id(project_id, entity_id))
    }

    async fn entities_delete_by_file(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, entities_delete_by_file(file_id, epoch))
    }

    async fn entities_set_parent(
        &self,
        project_id: i64,
        epoch: i64,
        child_id: i64,
        parent_id: i64,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            entities_set_parent(project_id, epoch, child_id, parent_id)
        )
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

    async fn entity_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<i64>, StorageError> {
        dispatch_relation!(self, entity_ids_by_file_epoch(project_id, epoch, path))
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, entities_count(project_id, epoch))
    }

    async fn entities_search_fts(
        &self,
        project_id: i64,
        query: &str,
        limit: i64,
        epoch: i64,
        excluded_files: &[String],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        dispatch_relation!(
            self,
            entities_search_fts(project_id, query, limit, epoch, excluded_files)
        )
    }

    async fn files_list(
        &self,
        project_id: i64,
        epochs: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        dispatch_relation!(self, files_list(project_id, epochs))
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

    async fn chunks_count_by_path(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, chunks_count_by_path(project_id, file_path))
    }

    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        dispatch_relation!(self, chunks_count(project_id, epoch))
    }

    async fn chunks_content_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<cce_storage_common::metadb::ChunkContent>, StorageError> {
        dispatch_relation!(self, chunks_content_by_epoch(project_id, epoch))
    }

    async fn chunks_stats(
        &self,
        project_id: i64,
        epoch: i64,
        channel: &str,
    ) -> Result<(i64, i64), StorageError> {
        dispatch_relation!(self, chunks_stats(project_id, epoch, channel))
    }

    async fn chunks_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<u64>, StorageError> {
        dispatch_relation!(self, chunks_max_entity_id(project_id, epoch))
    }

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, chunks_delete_by_file(project_id, path))
    }

    async fn chunks_and_mappings_upsert(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> Result<(usize, usize), StorageError> {
        dispatch_relation!(self, chunks_and_mappings_upsert(chunks, mappings))
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

    async fn mappings_by_entity(
        &self,
        project_id: i64,
        entity_id: i64,
        epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        dispatch_relation!(self, mappings_by_entity(project_id, entity_id, epoch))
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

    async fn checkpoint_active(
        &self,
        project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        dispatch_relation!(self, checkpoint_active(project_id))
    }

    async fn checkpoint_set_active(
        &self,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            checkpoint_set_active(project_id, operation_id, priority)
        )
    }

    async fn checkpoint_clear_active(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, checkpoint_clear_active(project_id, operation_id))
    }

    async fn checkpoint_heartbeat(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, checkpoint_heartbeat(project_id, operation_id))
    }

    async fn checkpoints_cleanup_stale(
        &self,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            checkpoints_cleanup_stale(project_id, stale_threshold_secs)
        )
    }

    async fn checkpoints_delete_expired(
        &self,
        project_id: i64,
        ttl_seconds: u64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, checkpoints_delete_expired(project_id, ttl_seconds))
    }

    async fn checkpoints_unfinished(
        &self,
        project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError> {
        dispatch_relation!(self, checkpoints_unfinished(project_id))
    }

    async fn checkpoint_latest_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_latest_incomplete(project_id, operation_type, root_dir)
        )
    }

    async fn checkpoint_set_batch_index(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            checkpoint_set_batch_index(project_id, operation_id, batch_index)
        )
    }

    async fn checkpoint_batch_create(
        &self,
        project_id: i64,
        batch: &cce_storage_common::metadb::BatchCheckpointRecord,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, checkpoint_batch_create(project_id, batch))
    }

    async fn checkpoint_batch_get(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<cce_storage_common::metadb::BatchCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_batch_get(project_id, operation_id, batch_index)
        )
    }

    async fn checkpoint_batch_files(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_batch_files(project_id, operation_id, batch_index)
        )
    }

    async fn checkpoint_failed_files(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        dispatch_relation!(self, checkpoint_failed_files(project_id, operation_id))
    }

    async fn checkpoint_batches_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_batches_delete_by_operation(project_id, operation_id)
        )
    }

    async fn checkpoint_file_set_parsed_data(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
        data: &[u8],
    ) -> Result<bool, StorageError> {
        dispatch_relation!(
            self,
            checkpoint_file_set_parsed_data(project_id, operation_id, file_path, data)
        )
    }

    async fn mappings_point_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<(i64, String)>, StorageError> {
        dispatch_relation!(
            self,
            mappings_point_ids_by_file_epoch(project_id, epoch, path)
        )
    }

    async fn mappings_delete_by_file_id(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_file_id(project_id, file_id))
    }

    async fn mappings_delete_by_entity_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, mappings_delete_by_entity_id(project_id, entity_id))
    }

    async fn file_checkpoints_clear_module_progress(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            file_checkpoints_clear_module_progress(project_id, operation_id)
        )
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

    async fn work_units_insert_batch(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> Result<Vec<i64>, StorageError> {
        dispatch_relation!(self, work_units_insert_batch(records))
    }

    async fn work_units_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            work_units_delete_by_operation(project_id, operation_id)
        )
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

    async fn snapshot_manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<cce_types::RelationSnapshotManifest>, StorageError> {
        dispatch_relation!(
            self,
            snapshot_manifest_by_operation(project_id, operation_id)
        )
    }

    async fn snapshot_activate(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        dispatch_relation!(self, snapshot_activate(project_id, epoch))
    }

    async fn snapshot_retry_failed(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        dispatch_relation!(self, snapshot_retry_failed(project_id, epoch))
    }

    async fn snapshot_collect_garbage(
        &self,
        project_id: i64,
        stale_before: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, snapshot_collect_garbage(project_id, stale_before))
    }

    async fn snapshot_entities_count(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<i64, StorageError> {
        dispatch_relation!(self, snapshot_entities_count(project_id, relation_epoch))
    }

    async fn snapshot_write_delta(
        &self,
        project_id: i64,
        epoch: i64,
        delta: &SnapshotDelta,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, snapshot_write_delta(project_id, epoch, delta))
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

    async fn snapshot_delete_manifests_except(
        &self,
        project_id: i64,
        keep_epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            snapshot_delete_manifests_except(project_id, keep_epoch)
        )
    }

    async fn snapshot_delta_chain_info(
        &self,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError> {
        dispatch_relation!(self, snapshot_delta_chain_info(project_id, active_epoch))
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

    async fn generation_copy_file(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            generation_copy_file(project_id, source_epoch, target_epoch, path)
        )
    }

    async fn generation_materialize(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            generation_materialize(project_id, source_epoch, target_epoch, excluded_paths)
        )
    }

    async fn generation_gc_plan(
        &self,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<cce_storage_common::metadb::GenerationGcPlan, StorageError> {
        dispatch_relation!(
            self,
            generation_gc_plan(project_id, keep_active_generations, stale_before)
        )
    }

    async fn generation_gc_apply(
        &self,
        project_id: i64,
        plan: &cce_storage_common::metadb::GenerationGcPlan,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, generation_gc_apply(project_id, plan))
    }

    async fn candidate_begin(
        &self,
        project_id: i64,
        candidate_epoch: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            candidate_begin(project_id, candidate_epoch, operation_id, parent_data_epoch)
        )
    }

    async fn candidate_clear_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, candidate_clear_epoch(project_id, epoch))
    }

    async fn candidate_delete_file_rows(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, candidate_delete_file_rows(project_id, epoch, paths))
    }

    async fn candidate_prepare_channel(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
        channel: &str,
    ) -> Result<(), StorageError> {
        dispatch_relation!(
            self,
            candidate_prepare_channel(project_id, epoch, paths, channel)
        )
    }

    async fn projection_load(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<cce_storage_common::metadb::IndexStateProjection>, StorageError> {
        dispatch_relation!(self, projection_load(project_id, operation_id))
    }

    async fn projection_upsert(
        &self,
        row: &cce_storage_common::metadb::IndexStateProjection,
    ) -> Result<(), StorageError> {
        dispatch_relation!(self, projection_upsert(row))
    }

    async fn projection_delete(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(self, projection_delete(project_id, operation_id, file_path))
    }

    async fn projection_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        dispatch_relation!(
            self,
            projection_delete_by_operation(project_id, operation_id)
        )
    }

    async fn projection_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        dispatch_relation!(self, projection_delete_by_project(project_id))
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
        }
    }

    fn is_per_project_db(&self) -> bool {
        match self {
            Self::Local(client) => RelationStorage::is_per_project_db(client.as_ref()),
        }
    }
}

impl cce_types::relation::RelationSnapshotStore for RelationStore {
    fn get_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<cce_types::RelationSnapshotManifest>, StorageError> {
        match self {
            Self::Local(client) => {
                SqliteSnapshotStore::new(client.as_ref().clone()).get_manifest(project_id, epoch)
            }
        }
    }

    fn read_snapshot(
        &self,
        manifest: &cce_types::RelationSnapshotManifest,
    ) -> Result<cce_types::CanonicalRelationSnapshot, StorageError> {
        match self {
            Self::Local(client) => {
                SqliteSnapshotStore::new(client.as_ref().clone()).read_snapshot(manifest)
            }
        }
    }

    fn find_base_epoch(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        match self {
            Self::Local(client) => SqliteSnapshotStore::new(client.as_ref().clone())
                .find_base_epoch(project_id, delta_epoch),
        }
    }

    fn get_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<cce_types::SnapshotDelta>, StorageError> {
        match self {
            Self::Local(client) => SqliteSnapshotStore::new(client.as_ref().clone())
                .get_delta_chain(project_id, after_epoch, up_to_epoch),
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
        let store = RelationStore::in_memory().expect("in-memory store");
        assert_eq!(RelationStorage::backend_name(&store), "local");
        assert!(RelationStorage::is_per_project_db(&store));
    }

    #[tokio::test]
    async fn relation_diagnose_reports_local_size() {
        let store = RelationStore::in_memory().expect("in-memory store");
        let diag = store.diagnose_summary().await;
        assert!(diag.reachable);
        assert!(diag.error.is_none());
    }

    #[test]
    fn relation_store_local_branch_dispatch() {
        let store = RelationStore::in_memory().expect("in-memory store");
        assert!(store.is_local());
        assert_eq!(store.backend_name(), "local");
        let scoped = store.for_project(1).expect("project scoping must work");
        assert!(scoped.is_local());
    }

    #[test]
    fn relation_store_rejects_remote_backend_selection() {
        let database = cce_config::global::DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        let err = RelationStore::from_database_config(&database)
            .expect_err("remote branch must fail at startup");
        assert!(
            err.to_string().contains("not wired yet"),
            "unexpected error: {err}"
        );
    }
}
