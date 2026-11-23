//! PostgreSQL remote branch for metadata storage.
//!
//! Implements the same [`MetadataStorage`] contract as the embedded SQLite
//! branch over `tokio-postgres` plus a `deadpool-postgres` pool. No
//! compile-time checked query framework is used: every statement is a plain
//! string executed at runtime, so the build never needs a live database.

mod admission;
mod checkpoint;
mod config;
mod error;
mod files;
mod generation;
mod json;
mod project;
mod projection;
mod rows;
mod schema;
mod snapshot;
mod sync_snapshot;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use cce_circuit_breaker::CircuitBreaker;
use cce_storage_common::metadb::{
    AdmissionAuditRecord, BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, ChunkContent,
    ChunkRecord, EntityDetailMapping, EntityFileWrite, EntityRecord, EntityWithFile,
    FileCheckpointRecord, FileEnsureEntry, FileEpochEntry, FileHashEntry, FileRecord,
    GenerationGcPlan, GenerationOverride, IndexStateProjection, MetadataStorage, NewProjectRecord,
    OverrideDisposition, ProjectIndexManifest, ProjectRecord, ProjectUpdateRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus,
};
use cce_types::error::common::ErrorClassify;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta, StorageError};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio::sync::Mutex;
use tokio_postgres::NoTls;

pub use config::PostgresConfig;

#[derive(Clone)]
pub struct PostgresClient {
    config: PostgresConfig,
    pool: Pool,
    breaker: Arc<Mutex<CircuitBreaker>>,
}

const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_TIMEOUT: Duration = Duration::from_secs(30);

/// Runtime that owns the connection pool's background tasks.
///
/// Snapshot reads are synchronous at the codegraph port. Keeping pool
/// maintenance on a dedicated multi-thread runtime lets those reads block a
/// caller on any runtime flavor without starving connection cleanup.
fn background_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("cce-metadata-pg")
            .enable_all()
            .build()
            .expect("postgres metadata runtime builds")
    })
}

pub(crate) fn runtime_handle() -> tokio::runtime::Handle {
    background_runtime().handle().clone()
}

impl PostgresClient {
    pub fn new(config: PostgresConfig) -> Result<Self, StorageError> {
        let _guard = background_runtime().enter();
        let pg_config = config.pg_config()?;
        let manager = Manager::from_config(
            pg_config,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );
        let pool = Pool::builder(manager)
            .max_size(config.pool_size.max(1) as usize)
            .wait_timeout(Some(config.acquire_timeout))
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(|error| {
                StorageError::connection(format!("failed to build pg pool: {error}"))
            })?;
        Ok(Self {
            config,
            pool,
            breaker: Arc::new(Mutex::new(CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_TIMEOUT,
            ))),
        })
    }

    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        let config = PostgresConfig::from_remote(&database.metadata_remote)?;
        Self::new(config)
    }

    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }

    pub fn is_enabled(&self) -> bool {
        !self.config.url.is_empty()
    }

    pub fn backend_name(&self) -> &'static str {
        "remote"
    }

    pub async fn ensure_migrated(&self) -> Result<(), StorageError> {
        let mut client = self.pooled().await?;
        client
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY,
                    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )",
            )
            .await
            .map_err(error::classify_pg)?;
        let applied: i64 = client
            .query_one(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                &[],
            )
            .await
            .map_err(error::classify_pg)?
            .get(0);
        if applied < 1 {
            let tx = client.transaction().await.map_err(error::classify_pg)?;
            tx.batch_execute(schema::V1_DDL)
                .await
                .map_err(error::classify_pg)?;
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES ($1)",
                &[&1i64],
            )
            .await
            .map_err(error::classify_pg)?;
            tx.commit().await.map_err(error::classify_pg)?;
        }
        if applied < schema::POSTGRES_SCHEMA_VERSION {
            let tx = client.transaction().await.map_err(error::classify_pg)?;
            if applied < 2 {
                tx.batch_execute(schema::V2_DDL)
                    .await
                    .map_err(error::classify_pg)?;
            }
            if applied < 3 {
                tx.batch_execute(schema::V3_DDL)
                    .await
                    .map_err(error::classify_pg)?;
            }
            if applied < 4 {
                tx.batch_execute(schema::V4_DDL)
                    .await
                    .map_err(error::classify_pg)?;
            }
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES ($1)",
                &[&schema::POSTGRES_SCHEMA_VERSION],
            )
            .await
            .map_err(error::classify_pg)?;
            tx.commit().await.map_err(error::classify_pg)?;
            tracing::info!(
                version = schema::POSTGRES_SCHEMA_VERSION,
                "PostgreSQL schema migrated"
            );
        } else {
            tracing::debug!(version = applied, "PostgreSQL schema already current");
        }
        Ok(())
    }

    pub async fn health(&self) -> Result<bool, StorageError> {
        self.check_breaker().await?;
        let client = self.pooled().await?;
        let result = client
            .query_one("SELECT 1", &[])
            .await
            .map(|_| ())
            .map_err(error::classify_pg);
        match &result {
            Ok(()) => self.record_success().await,
            Err(e) => self.record_failure(e).await,
        }
        result.map(|()| true)
    }

    async fn pooled(&self) -> Result<deadpool_postgres::Client, StorageError> {
        // Connection drivers must live on the metadata runtime so synchronous
        // snapshot reads can wait without parking the caller's runtime.
        let pool = self.pool.clone();
        runtime_handle()
            .spawn(async move {
                pool.get().await.map_err(|error| {
                    StorageError::connection(format!("pg pool acquire failed: {error}"))
                })
            })
            .await
            .map_err(|error| StorageError::connection(format!("pg pool task failed: {error}")))?
    }

    async fn check_breaker(&self) -> Result<(), StorageError> {
        if self.breaker.lock().await.is_open() {
            return Err(StorageError::connection(
                "circuit breaker is open, rejecting relation request",
            ));
        }
        Ok(())
    }

    async fn record_success(&self) {
        self.breaker.lock().await.record_success();
    }

    async fn record_failure(&self, error: &StorageError) {
        if error.is_transient() {
            tracing::warn!(error = %error, "Relation remote failure recorded by circuit breaker");
            self.breaker.lock().await.record_failure();
        }
    }

    async fn run<T>(
        &self,
        run: impl std::future::Future<Output = Result<T, StorageError>> + Send,
    ) -> Result<T, StorageError> {
        self.check_breaker().await?;
        let result = run.await;
        match &result {
            Ok(_) => self.record_success().await,
            Err(e) => self.record_failure(e).await,
        }
        result
    }
}

impl cce_storage_common::metadb::MetadataProjectScope for PostgresClient {
    fn scope_to_project(&self, _project_id: i64) -> Result<Self, StorageError> {
        Ok(self.clone())
    }
}

impl std::fmt::Debug for PostgresClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresClient")
            .field("pool_size", &self.config.pool_size)
            .finish_non_exhaustive()
    }
}

impl MetadataStorage for PostgresClient {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        self.ensure_project(project_id, root_path).await
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        self.project_record(project_id).await
    }

    async fn projects_all(&self) -> Result<Vec<ProjectRecord>, StorageError> {
        self.projects_all().await
    }

    async fn project_insert(&self, record: &NewProjectRecord) -> Result<i64, StorageError> {
        self.project_insert(record).await
    }

    async fn project_path_exists(&self, root_path: &str) -> Result<bool, StorageError> {
        self.project_path_exists(root_path).await
    }

    async fn project_clear_index(&self, project_id: i64) -> Result<(), StorageError> {
        self.project_clear_index(project_id).await
    }

    async fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        self.project_meta_get_int(project_id, key).await
    }

    async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        self.project_meta_set_int(project_id, key, value).await
    }

    async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        self.manifest_begin_building(project_id, data_epoch, operation_id, input_fingerprint)
            .await
    }

    async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.manifest_mark_candidate_ready(project_id, operation_id)
            .await
    }

    async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        self.manifest_activate(
            project_id,
            data_epoch,
            relation_epoch,
            operation_id,
            input_fingerprint,
        )
        .await
    }

    async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.manifest_mark_failed(project_id, operation_id, reason)
            .await
    }

    async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        self.manifest_active(project_id).await
    }

    async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.manifest_recycle_epoch(project_id, epoch).await
    }

    async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError> {
        self.overrides_replace(project_id, epoch, overrides).await
    }

    async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        self.overrides_for_generation(project_id, epoch).await
    }

    async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError> {
        self.files_upsert(project_id, epoch, files).await
    }

    async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.files_delete_by_project_epoch(project_id, epoch).await
    }

    async fn files_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.files_delete_by_project(project_id).await
    }

    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError> {
        self.entities_upsert(entities).await
    }

    async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.entities_delete_by_project_epoch(project_id, epoch)
            .await
    }

    async fn entities_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.entities_delete_by_project(project_id).await
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        self.entities_count(project_id, epoch).await
    }

    async fn entities_search_fts(
        &self,
        project_id: i64,
        query: &str,
        limit: i64,
        epoch: i64,
        excluded_files: &[String],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        self.entities_search_fts(project_id, query, limit, epoch, excluded_files)
            .await
    }

    async fn files_list(
        &self,
        project_id: i64,
        epochs: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        self.files_list(project_id, epochs).await
    }

    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError> {
        self.chunks_upsert(chunks).await
    }

    async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        self.chunks_by_ids(project_id, chunk_ids, epochs).await
    }

    async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.chunks_delete_by_project_epoch(project_id, epoch).await
    }

    async fn chunks_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.chunks_delete_by_project(project_id).await
    }

    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        self.chunks_count(project_id, epoch).await
    }

    async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError> {
        self.mappings_upsert(mappings).await
    }

    async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.mappings_delete_by_project_epoch(project_id, epoch)
            .await
    }

    async fn mappings_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.mappings_delete_by_project(project_id).await
    }

    async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        self.checkpoint_create(project_id, checkpoint).await
    }

    async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        self.checkpoint_get(project_id, operation_id).await
    }

    async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        self.checkpoint_set_status(project_id, operation_id, status)
            .await
    }

    async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        self.checkpoint_update(project_id, checkpoint).await
    }

    async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        self.file_checkpoint_upsert(project_id, file).await
    }

    async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        self.file_checkpoint_get(project_id, operation_id, file_path)
            .await
    }

    async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.checkpoint_files_delete_by_operation(project_id, operation_id)
            .await
    }

    async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        self.work_unit_insert(record).await
    }

    async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        self.work_unit_set_status(project_id, operation_id, stage, work_unit_hash, status)
            .await
    }

    async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        self.work_units_list(project_id, operation_id, stage).await
    }

    async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        self.work_unit_by_hash(project_id, operation_id, stage, work_unit_hash)
            .await
    }

    async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        self.snapshot_allocate(project_id, operation_id, config_fingerprint)
            .await
    }

    async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        self.snapshot_write_ready(
            project_id,
            epoch,
            snapshot,
            input_fingerprint,
            snapshot_fingerprint,
        )
        .await
    }

    async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        self.snapshot_read(project_id, epoch).await
    }

    async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        self.snapshot_manifest(project_id, epoch).await
    }

    async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        self.snapshot_delta_chain(project_id, after_epoch, up_to_epoch)
            .await
    }

    async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        self.snapshot_find_base(project_id, delta_epoch).await
    }

    async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.snapshot_mark_failed(project_id, epoch, reason).await
    }

    async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.snapshot_delete_epoch(project_id, epoch).await
    }

    async fn snapshot_delete_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.snapshot_delete_project(project_id).await
    }

    async fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        self.record_admitted(fingerprint, projects, quota_bytes, bytes)
            .await
    }

    async fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.record_rejection(fingerprint, projects, quota_bytes, reason)
            .await
    }

    async fn admission_get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        self.get(fingerprint).await
    }

    async fn admission_list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        self.list().await
    }

    async fn db_size(&self) -> Result<u64, StorageError> {
        self.db_size().await
    }

    async fn project_meta_get_int_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<i64>, StorageError> {
        self.project_meta_get_int_optional(project_id, key).await
    }

    async fn project_meta_get_string_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<String>, StorageError> {
        self.project_meta_get_string_optional(project_id, key).await
    }

    async fn project_meta_set_string(
        &self,
        project_id: i64,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        self.project_meta_set_string(project_id, key, value).await
    }

    async fn chunks_page_with_category(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError> {
        self.chunks_page_with_category(project_id, epoch, limit, offset)
            .await
    }

    async fn files_page(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        self.files_page(project_id, epoch, limit, offset).await
    }

    async fn files_visible_hashes(
        &self,
        project_id: i64,
    ) -> Result<Vec<FileHashEntry>, StorageError> {
        self.files_visible_hashes(project_id).await
    }

    async fn files_record_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, String)],
    ) -> Result<usize, StorageError> {
        self.files_record_hashes(project_id, epoch, entries).await
    }

    async fn files_remove_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        self.files_remove_hashes(project_id, epoch, paths).await
    }

    async fn files_ensure(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        entries: &[FileEnsureEntry],
    ) -> Result<usize, StorageError> {
        self.files_ensure(project_id, epoch, batch_id, entries)
            .await
    }

    async fn files_delete_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.files_delete_by_path(project_id, path).await
    }

    async fn file_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<Option<FileRecord>, StorageError> {
        self.file_by_path(project_id, path).await
    }

    async fn entities_insert_returning_ids(
        &self,
        entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError> {
        self.entities_insert_returning_ids(entities).await
    }

    async fn entities_replace_files(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        files: &[EntityFileWrite],
    ) -> Result<(), StorageError> {
        self.entities_replace_files(project_id, epoch, batch_id, files)
            .await
    }

    async fn entities_with_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError> {
        self.entities_with_files(project_id, epoch).await
    }

    async fn entities_with_files_after(
        &self,
        project_id: i64,
        epoch: i64,
        after_id: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError> {
        Ok(self
            .entities_with_files(project_id, epoch)
            .await?
            .into_iter()
            .filter(|entry| entry.entity.id > after_id)
            .collect())
    }

    async fn entities_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        self.entities_by_ids(project_id, ids).await
    }

    async fn entities_by_file(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        self.entities_by_file(project_id, file_id).await
    }

    async fn entities_delete_by_file(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.entities_delete_by_file(file_id, epoch).await
    }

    async fn entities_set_parent(
        &self,
        project_id: i64,
        epoch: i64,
        child_id: i64,
        parent_id: i64,
    ) -> Result<(), StorageError> {
        self.entities_set_parent(project_id, epoch, child_id, parent_id)
            .await
    }

    async fn chunks_content_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<ChunkContent>, StorageError> {
        self.chunks_content_by_epoch(project_id, epoch).await
    }

    async fn chunks_stats(
        &self,
        project_id: i64,
        epoch: i64,
        channel: &str,
    ) -> Result<(i64, i64), StorageError> {
        self.chunks_stats(project_id, epoch, channel).await
    }

    async fn chunks_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<u64>, StorageError> {
        self.chunks_max_entity_id(project_id, epoch).await
    }

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.chunks_delete_by_file(project_id, path).await
    }

    async fn chunks_and_mappings_upsert(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> Result<(usize, usize), StorageError> {
        self.chunks_and_mappings_upsert(chunks, mappings).await
    }

    async fn mappings_by_entity(
        &self,
        project_id: i64,
        entity_id: i64,
        epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        self.mappings_by_entity(project_id, entity_id, epoch).await
    }

    async fn manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        self.manifest_by_operation(project_id, operation_id).await
    }

    async fn manifest_building_max_epoch(
        &self,
        project_id: i64,
    ) -> Result<Option<i64>, StorageError> {
        self.manifest_building_max_epoch(project_id).await
    }

    async fn manifest_set_parent(
        &self,
        project_id: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        self.manifest_set_parent(project_id, operation_id, parent_data_epoch)
            .await
    }

    async fn manifest_has_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
    ) -> Result<bool, StorageError> {
        self.manifest_has_incomplete(project_id, operation_type)
            .await
    }

    async fn overrides_upsert_batch(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, OverrideDisposition)],
    ) -> Result<(), StorageError> {
        self.overrides_upsert_batch(project_id, epoch, entries)
            .await
    }

    async fn overrides_clear(&self, project_id: i64, epoch: i64) -> Result<usize, StorageError> {
        self.overrides_clear(project_id, epoch).await
    }

    async fn generation_copy_file(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.generation_copy_file(project_id, source_epoch, target_epoch, path)
            .await
    }

    async fn generation_materialize(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), StorageError> {
        self.generation_materialize(project_id, source_epoch, target_epoch, excluded_paths)
            .await
    }

    async fn generation_gc_plan(
        &self,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<GenerationGcPlan, StorageError> {
        self.generation_gc_plan(project_id, keep_active_generations, stale_before)
            .await
    }

    async fn generation_gc_apply(
        &self,
        project_id: i64,
        plan: &GenerationGcPlan,
    ) -> Result<(), StorageError> {
        self.generation_gc_apply(project_id, plan).await
    }

    async fn candidate_begin(
        &self,
        project_id: i64,
        candidate_epoch: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        self.candidate_begin(project_id, candidate_epoch, operation_id, parent_data_epoch)
            .await
    }

    async fn candidate_clear_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.candidate_clear_epoch(project_id, epoch).await
    }

    async fn candidate_delete_file_rows(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        self.candidate_delete_file_rows(project_id, epoch, paths)
            .await
    }

    async fn candidate_prepare_channel(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
        channel: &str,
    ) -> Result<(), StorageError> {
        self.candidate_prepare_channel(project_id, epoch, paths, channel)
            .await
    }

    async fn projection_load(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<IndexStateProjection>, StorageError> {
        self.projection_load(project_id, operation_id).await
    }

    async fn projection_upsert_batch(
        &self,
        rows: &[IndexStateProjection],
    ) -> Result<(), StorageError> {
        self.projection_upsert_batch(rows).await
    }

    async fn projection_delete(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        self.projection_delete(project_id, operation_id, file_path)
            .await
    }

    async fn projection_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.projection_delete_by_operation(project_id, operation_id)
            .await
    }

    async fn projection_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.projection_delete_by_project(project_id).await
    }

    async fn checkpoint_active(
        &self,
        project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        self.checkpoint_active(project_id).await
    }

    async fn checkpoint_set_active(
        &self,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> Result<(), StorageError> {
        self.checkpoint_set_active(project_id, operation_id, priority)
            .await
    }

    async fn checkpoint_clear_active(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.checkpoint_clear_active(project_id, operation_id).await
    }

    async fn checkpoint_heartbeat(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.checkpoint_heartbeat(project_id, operation_id).await
    }

    async fn checkpoints_cleanup_stale(
        &self,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        self.checkpoints_cleanup_stale(project_id, stale_threshold_secs)
            .await
    }

    async fn checkpoints_delete_expired(
        &self,
        project_id: i64,
        ttl_seconds: u64,
    ) -> Result<usize, StorageError> {
        self.checkpoints_delete_expired(project_id, ttl_seconds)
            .await
    }

    async fn files_epoch_entries(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FileEpochEntry>, StorageError> {
        self.files_epoch_entries(project_id, epoch).await
    }

    async fn files_set_epoch(
        &self,
        project_id: i64,
        path: &str,
        epoch: i64,
    ) -> Result<bool, StorageError> {
        self.files_set_epoch(project_id, path, epoch).await
    }

    async fn checkpoint_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        self.checkpoint_cached_parsed_data(project_id, file_path, content_hash)
            .await
    }

    async fn chunks_by_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        self.chunks_by_file(project_id, file_path, epoch).await
    }

    async fn snapshot_delete_manifests_except(
        &self,
        project_id: i64,
        keep_epoch: i64,
    ) -> Result<usize, StorageError> {
        self.snapshot_delete_manifests_except(project_id, keep_epoch)
            .await
    }

    async fn snapshot_delta_chain_info(
        &self,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError> {
        self.snapshot_delta_chain_info(project_id, active_epoch)
            .await
    }

    async fn snapshot_write_delta(
        &self,
        project_id: i64,
        epoch: i64,
        delta: &SnapshotDelta,
    ) -> Result<(), StorageError> {
        self.snapshot_write_delta(project_id, epoch, delta).await
    }

    async fn manifest_parent_epoch(
        &self,
        project_id: i64,
        data_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        self.manifest_parent_epoch(project_id, data_epoch).await
    }

    async fn files_exists_at_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<bool, StorageError> {
        self.files_exists_at_epoch(project_id, epoch, path).await
    }

    async fn entity_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<i64>, StorageError> {
        self.entity_ids_by_file_epoch(project_id, epoch, path).await
    }

    async fn chunks_by_file_epoch(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        self.chunks_by_file_epoch(project_id, file_path, epoch)
            .await
    }

    async fn checkpoint_file_set_parsed_data(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
        data: &[u8],
    ) -> Result<bool, StorageError> {
        self.checkpoint_file_set_parsed_data(project_id, operation_id, file_path, data)
            .await
    }

    async fn mappings_point_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<(i64, String)>, StorageError> {
        self.mappings_point_ids_by_file_epoch(project_id, epoch, path)
            .await
    }

    async fn chunks_count_by_path(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<i64, StorageError> {
        self.chunks_count_by_path(project_id, file_path).await
    }

    async fn entities_delete_by_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<bool, StorageError> {
        self.entities_delete_by_id(project_id, entity_id).await
    }

    async fn mappings_delete_by_file_id(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<usize, StorageError> {
        self.mappings_delete_by_file_id(project_id, file_id).await
    }

    async fn mappings_delete_by_entity_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<usize, StorageError> {
        self.mappings_delete_by_entity_id(project_id, entity_id)
            .await
    }

    async fn files_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        self.files_by_ids(project_id, ids).await
    }

    async fn project_update(
        &self,
        project_id: i64,
        updates: &ProjectUpdateRecord,
    ) -> Result<(), StorageError> {
        self.project_update(project_id, updates).await
    }

    async fn checkpoints_unfinished(
        &self,
        project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError> {
        self.checkpoints_unfinished(project_id).await
    }

    async fn checkpoint_latest_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        self.checkpoint_latest_incomplete(project_id, operation_type, root_dir)
            .await
    }

    async fn checkpoint_set_batch_index(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        self.checkpoint_set_batch_index(project_id, operation_id, batch_index)
            .await
    }

    async fn checkpoint_batch_create(
        &self,
        project_id: i64,
        batch: &BatchCheckpointRecord,
    ) -> Result<(), StorageError> {
        self.checkpoint_batch_create(project_id, batch).await
    }

    async fn checkpoint_batch_get(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError> {
        self.checkpoint_batch_get(project_id, operation_id, batch_index)
            .await
    }

    async fn checkpoint_batch_files(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        self.checkpoint_batch_files(project_id, operation_id, batch_index)
            .await
    }

    async fn checkpoint_failed_files(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        self.checkpoint_failed_files(project_id, operation_id).await
    }

    async fn checkpoint_batches_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.checkpoint_batches_delete_by_operation(project_id, operation_id)
            .await
    }

    async fn file_checkpoints_clear_module_progress(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.file_checkpoints_clear_module_progress(project_id, operation_id)
            .await
    }

    async fn work_units_insert_batch(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> Result<Vec<i64>, StorageError> {
        self.work_units_insert_batch(records).await
    }

    async fn work_units_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.work_units_delete_by_operation(project_id, operation_id)
            .await
    }

    async fn snapshot_manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        self.snapshot_manifest_by_operation(project_id, operation_id)
            .await
    }

    async fn snapshot_activate(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.snapshot_activate(project_id, epoch).await
    }

    async fn snapshot_retry_failed(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.snapshot_retry_failed(project_id, epoch).await
    }

    async fn snapshot_collect_garbage(
        &self,
        project_id: i64,
        stale_before: i64,
    ) -> Result<usize, StorageError> {
        self.snapshot_collect_garbage(project_id, stale_before)
            .await
    }

    async fn snapshot_entities_count(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<i64, StorageError> {
        self.snapshot_entities_count(project_id, relation_epoch)
            .await
    }

    async fn files_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        self.files_count(project_id, epoch).await
    }

    async fn project_delete_with_cascade(&self, project_id: i64) -> Result<(), StorageError> {
        self.project_delete_with_cascade(project_id).await
    }

    async fn project_delete_db(&self, project_id: i64) -> Result<usize, StorageError> {
        self.delete_project_db(project_id).await
    }

    async fn projection_upsert(&self, row: &IndexStateProjection) -> Result<(), StorageError> {
        self.projection_upsert(row).await
    }

    fn backend_name(&self) -> &'static str {
        "remote"
    }

    fn is_per_project_db(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::modules::MetadataRemoteConfig;
    use cce_types::error::common::ErrorClassify;

    fn test_config() -> PostgresConfig {
        PostgresConfig {
            url: "postgres://localhost:5432/cce".to_string(),
            username: Some("cce".to_string()),
            password: None,
            pool_size: 4,
            connect_timeout: Duration::from_millis(1000),
            acquire_timeout: Duration::from_millis(1000),
            statement_timeout: Duration::from_millis(5000),
        }
    }

    #[test]
    fn remote_config_requires_url() {
        let remote = MetadataRemoteConfig::default();
        assert!(PostgresConfig::from_remote(&remote).is_err());
        let remote = MetadataRemoteConfig {
            url: Some("postgres://localhost:5432/cce".to_string()),
            ..MetadataRemoteConfig::default()
        };
        let config = PostgresConfig::from_remote(&remote).expect("url suffices");
        assert!(config.username.is_none());
    }

    #[test]
    fn remote_config_rejects_bad_pool_size() {
        let remote = MetadataRemoteConfig {
            url: Some("postgres://localhost:5432/cce".to_string()),
            pool_size: 0,
            ..MetadataRemoteConfig::default()
        };
        assert!(PostgresConfig::from_remote(&remote).is_err());
    }

    #[test]
    fn migration_ddl_covers_business_tables() {
        for table in [
            "projects",
            "project_meta",
            "project_index_manifests",
            "generation_overrides",
            "admission_audit",
            "files",
            "entities",
            "entity_detail_mappings",
            "chunks",
            "checkpoint",
            "checkpoint_batch",
            "checkpoint_file",
            "work_unit_checkpoint",
            "index_state_projection",
            "relation_snapshot_manifest",
            "relation_snapshot_files",
            "relation_snapshot_entities",
            "relation_snapshot_relations",
            "relation_snapshot_exports",
            "relation_snapshot_dependencies",
            "relation_snapshot_deltas",
            "schema_migrations",
        ] {
            assert!(
                schema::V1_DDL.contains(table) || table == "schema_migrations",
                "{table} must be migrated"
            );
        }
        assert!(schema::V1_DDL.contains("ON DELETE CASCADE"));
        assert!(schema::V1_DDL.contains("UNIQUE (project_id, epoch, path)"));
        assert!(schema::V1_DDL.contains("BYTEA"));
    }

    #[test]
    fn pool_builds_without_io() {
        let client = PostgresClient::new(test_config()).expect("pool builds without I/O");
        assert_eq!(client.backend_name(), "remote");
        assert!(client.is_enabled());
        cce_storage_common::metadb::assert_metadata_storage::<PostgresClient>();
    }

    #[test]
    fn transient_states_classify_retryable() {
        use tokio_postgres::error::SqlState;
        assert!(StorageError::connection("pg connection closed").is_retryable());
        assert!(StorageError::transaction("pg transient failure").is_retryable());
        assert_eq!(SqlState::T_R_SERIALIZATION_FAILURE.code(), "40001");
        assert_eq!(SqlState::T_R_DEADLOCK_DETECTED.code(), "40P01");
        assert_eq!(SqlState::LOCK_NOT_AVAILABLE.code(), "55P03");
    }
}
