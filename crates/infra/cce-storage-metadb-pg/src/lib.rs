//! PostgreSQL remote branch for relation storage.
//!
//! Implements the same [`RelationStorage`] contract as the embedded SQLite
//! branch over `tokio-postgres` plus a `deadpool-postgres` pool. No
//! compile-time checked query framework is used: every statement is a plain
//! string executed at runtime, so the build never needs a live database.

mod admission;
mod checkpoint;
mod config;
mod error;
mod files;
mod json;
mod project;
mod rows;
mod schema;
mod snapshot;

use std::sync::Arc;
use std::time::Duration;

use cce_circuit_breaker::CircuitBreaker;
use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileRecord, GenerationOverride, ProjectIndexManifest,
    ProjectRecord, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
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

impl PostgresClient {
    pub fn new(config: PostgresConfig) -> Result<Self, StorageError> {
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
        let config = PostgresConfig::from_remote(&database.relation_remote)?;
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
        self.pool
            .get()
            .await
            .map_err(|error| StorageError::connection(format!("pg pool acquire failed: {error}")))
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

impl std::fmt::Debug for PostgresClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresClient")
            .field("pool_size", &self.config.pool_size)
            .finish_non_exhaustive()
    }
}

impl RelationStorage for PostgresClient {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        self.ensure_project(project_id, root_path).await
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        self.project_record(project_id).await
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

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        self.chunks_delete_by_file(project_id, file_path).await
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

    async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError> {
        self.summary_upsert(file_id, epoch, summary_json).await
    }

    async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        self.summary_at_epoch(file_id, epoch).await
    }

    async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        self.summaries_by_epoch(project_id, epoch).await
    }

    async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.summaries_delete_by_project_epoch(project_id, epoch)
            .await
    }

    async fn summaries_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.summaries_delete_by_project(project_id).await
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

    async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        self.delete_project_db(project_id).await
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
    use cce_config::modules::RelationRemoteConfig;
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
        let remote = RelationRemoteConfig::default();
        assert!(PostgresConfig::from_remote(&remote).is_err());
        let remote = RelationRemoteConfig {
            url: Some("postgres://localhost:5432/cce".to_string()),
            ..RelationRemoteConfig::default()
        };
        let config = PostgresConfig::from_remote(&remote).expect("url suffices");
        assert!(config.username.is_none());
    }

    #[test]
    fn remote_config_rejects_bad_pool_size() {
        let remote = RelationRemoteConfig {
            url: Some("postgres://localhost:5432/cce".to_string()),
            pool_size: 0,
            ..RelationRemoteConfig::default()
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
            "file_summaries",
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
        cce_storage_common::metadb::assert_relation_storage::<PostgresClient>();
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
