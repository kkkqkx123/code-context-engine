//! Relation storage contract (backend-neutral).
//!
//! Groups the business-data operations that the embedded SQLite branch
//! and the remote PostgreSQL branch serve: project directory, generation
//! manifests, per-generation overrides, content indexes (files, entities,
//! chunks, detail mappings, file summaries), progress checkpoints, relation
//! snapshots (including incremental reads), and admission audit.
//!
//! Transaction boundary: every named operation is atomic on its own scope —
//! it commits once on success and rolls back on any failure, never a
//! partial prefix. Batch writes are idempotent per unique key
//! (insert-or-update), so replaying a batch after a transient failure is
//! safe. Transient failures (serialization conflicts, deadlocks, connection
//! loss) surface as retryable errors; deterministic failures are permanent.
//!
//! The contract never exposes a concrete connection or transaction type:
//! the local branch opens its own transaction per operation, the remote
//! branch takes a pooled connection per operation. Operational data (metric
//! aggregation, in-process caches) stays local and is intentionally outside
//! this contract.

use cce_types::StorageError;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta};

mod checkpoint;
mod chunk;
mod entity;
mod generation;
mod project;

pub use checkpoint::{
    CheckpointRecord, CheckpointStatus, FileCheckpointRecord, WorkUnitCheckpointRecord,
    WorkUnitStatus,
};
pub use chunk::{ChunkRecord, EntityDetailMapping};
pub use entity::{DbId, EntityRecord, FileRecord};
pub use generation::{
    AdmissionAuditRecord, GenerationGcPlan, GenerationOverride, OverrideDisposition,
    ProjectIndexManifest, ProjectIndexManifestState,
};
pub use project::{NewProjectRecord, ProjectRecord, ProjectUpdateRecord};

/// Backend-neutral record aliases shared by the local and remote branches.
pub type RelationFile = FileRecord;
/// Backend-neutral record aliases shared by the local and remote branches.
pub type RelationEntity = EntityRecord;
/// Backend-neutral record aliases shared by the local and remote branches.
pub type RelationChunk = ChunkRecord;

/// Relation storage contract implemented by the local SQLite branch and the
/// remote PostgreSQL branch.
///
/// Callers hold the backend enum and call through this contract; no caller
/// touches a concrete connection or transaction.
pub trait RelationStorage: Clone + Send + Sync + 'static {
    // -- Project directory --

    /// Ensure the project registry row exists (idempotent).
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError>;

    /// Fetch the project registry row, if present.
    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError>;

    /// Read an integer project metadata value.
    async fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError>;

    /// Write an integer project metadata value.
    async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError>;

    // -- Generation manifest --

    /// Allocate (or reattach to) the building manifest for an operation.
    async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError>;

    /// Mark the inheritance registration of a candidate complete.
    async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError>;

    /// Atomically activate a generation: the manifest row, the relation
    /// snapshot state, and the project metadata advance in one transaction.
    async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError>;

    /// Mark the building manifest of an operation failed.
    async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError>;

    /// Fetch the currently active manifest, if any.
    async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError>;

    /// Recycle one data epoch: content rows, overrides, manifest rows, and
    /// snapshot rows of the epoch are removed in one transaction.
    async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    // -- Generation overrides --

    /// Replace the override set of one generation atomically.
    async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError>;

    /// List the overrides of one generation ordered by file path.
    async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError>;

    // -- Content index --

    /// Idempotent batch write of file rows for one project and epoch.
    async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError>;

    /// Delete file rows of one project and epoch.
    async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete all file rows of one project.
    async fn files_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Idempotent batch write of entity rows (records carry project/epoch).
    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError>;

    /// Delete entity rows of one project and epoch.
    async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete all entity rows of one project.
    async fn entities_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Count entity rows of one project and epoch.
    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError>;

    /// Idempotent batch write of chunk rows (records carry project/epoch).
    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError>;

    /// Read back chunk rows by id for enrichment. An empty epoch list
    /// disables epoch filtering.
    async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError>;

    /// Delete chunk rows of one file.
    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<usize, StorageError>;

    /// Delete chunk rows of one project and epoch.
    async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete all chunk rows of one project.
    async fn chunks_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Count chunk rows of one project and epoch.
    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError>;

    /// Idempotent batch write of entity detail mappings.
    async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError>;

    /// Delete mapping rows of one project and epoch.
    async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete all mapping rows of one project.
    async fn mappings_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Idempotent write of one file summary at an epoch.
    async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError>;

    /// Read one file summary at an epoch.
    async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError>;

    /// List summary payloads of one project and epoch.
    async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError>;

    /// Delete summary rows of one project and epoch.
    async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete all summary rows of one project.
    async fn summaries_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError>;

    // -- Progress checkpoints --

    /// Create an operation-level checkpoint.
    async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError>;

    /// Fetch an operation-level checkpoint.
    async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError>;

    /// Update an operation-level checkpoint status.
    async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError>;

    /// Replace an operation-level checkpoint.
    async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError>;

    /// Idempotent write of one file checkpoint.
    async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError>;

    /// Fetch one file checkpoint.
    async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError>;

    /// Delete file checkpoints of one operation.
    async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError>;

    /// Insert a work-unit checkpoint.
    async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError>;

    /// Update a work-unit checkpoint status.
    async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError>;

    /// List work-unit checkpoints of one operation stage.
    async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError>;

    /// Fetch one work-unit checkpoint by hash.
    async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError>;

    // -- Relation snapshots --

    /// Allocate a building relation epoch for an operation.
    async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError>;

    /// Persist a full snapshot and mark its epoch ready, atomically.
    async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError>;

    /// Read back the canonical snapshot of an epoch.
    async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<CanonicalRelationSnapshot, StorageError>;

    /// Fetch the snapshot manifest of an epoch, if present.
    async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError>;

    /// Read the incremental delta chain of an epoch range.
    async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError>;

    /// Resolve the base epoch a delta epoch was built from.
    async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError>;

    /// Mark a relation epoch failed.
    async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError>;

    /// Delete every snapshot row of one epoch.
    async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError>;

    /// Delete every snapshot row of one project.
    async fn snapshot_delete_project(&self, project_id: i64) -> Result<usize, StorageError>;

    // -- Admission audit --

    /// Record one admitted ingest batch.
    async fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError>;

    /// Record one rejection with its cause.
    async fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError>;

    /// Fetch one audit row by fingerprint.
    async fn admission_get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError>;

    /// List every audit row ordered by fingerprint.
    async fn admission_list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError>;

    // -- Capacity --

    /// Aggregate on-disk size (local: main plus per-project files; remote:
    /// whole database size plus per-project estimate where available).
    async fn db_size(&self) -> Result<u64, StorageError>;

    /// Remove a project: local deletes the per-project database files
    /// (evict handle, then main plus WAL/SHM sidecars); remote deletes the
    /// project rows across business tables in one transaction.
    /// Returns the number of removed business records; snapshot-only
    /// deletion stays on `snapshot_delete_project`.
    async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError>;

    /// Backend name for logging (`local` for the embedded branch).
    fn backend_name(&self) -> &'static str {
        "local"
    }

    /// Whether this branch keeps per-project database files.
    fn is_per_project_db(&self) -> bool {
        true
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_relation_storage<T: RelationStorage>() {}
