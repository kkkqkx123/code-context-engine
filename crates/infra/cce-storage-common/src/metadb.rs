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

use std::future::Future;

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
    fn ensure_project(
        &self,
        project_id: i64,
        root_path: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch the project registry row, if present.
    fn project_record(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Option<ProjectRecord>, StorageError>> + Send;

    /// Read an integer project metadata value.
    fn project_meta_get_int(
        &self,
        project_id: i64,
        key: &str,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Write an integer project metadata value.
    fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    // -- Generation manifest --

    /// Allocate (or reattach to) the building manifest for an operation.
    fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> impl Future<Output = Result<ProjectIndexManifest, StorageError>> + Send;

    /// Mark the inheritance registration of a candidate complete.
    fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Atomically activate a generation: the manifest row, the relation
    /// snapshot state, and the project metadata advance in one transaction.
    fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> impl Future<Output = Result<ProjectIndexManifest, StorageError>> + Send;

    /// Mark the building manifest of an operation failed.
    fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch the currently active manifest, if any.
    fn manifest_active(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Option<ProjectIndexManifest>, StorageError>> + Send;

    /// Recycle one data epoch: content rows, overrides, manifest rows, and
    /// snapshot rows of the epoch are removed in one transaction.
    fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    // -- Generation overrides --

    /// Replace the override set of one generation atomically.
    fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// List the overrides of one generation ordered by file path.
    fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<GenerationOverride>, StorageError>> + Send;

    // -- Content index --

    /// Idempotent batch write of file rows for one project and epoch.
    fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete file rows of one project and epoch.
    fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete all file rows of one project.
    fn files_delete_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Idempotent batch write of entity rows (records carry project/epoch).
    fn entities_upsert(
        &self,
        entities: &[EntityRecord],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete entity rows of one project and epoch.
    fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete all entity rows of one project.
    fn entities_delete_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Count entity rows of one project and epoch.
    fn entities_count(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Idempotent batch write of chunk rows (records carry project/epoch).
    fn chunks_upsert(
        &self,
        chunks: &[ChunkRecord],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Read back chunk rows by id for enrichment. An empty epoch list
    /// disables epoch filtering.
    fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> impl Future<Output = Result<Vec<ChunkRecord>, StorageError>> + Send;

    /// Delete chunk rows of one file.
    fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete chunk rows of one project and epoch.
    fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete all chunk rows of one project.
    fn chunks_delete_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Count chunk rows of one project and epoch.
    fn chunks_count(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Idempotent batch write of entity detail mappings.
    fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete mapping rows of one project and epoch.
    fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete all mapping rows of one project.
    fn mappings_delete_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Idempotent write of one file summary at an epoch.
    fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Read one file summary at an epoch.
    fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Option<String>, StorageError>> + Send;

    /// List summary payloads of one project and epoch.
    fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<(String, String, i64)>, StorageError>> + Send;

    /// Delete summary rows of one project and epoch.
    fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete all summary rows of one project.
    fn summaries_delete_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    // -- Progress checkpoints --

    /// Create an operation-level checkpoint.
    fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Fetch an operation-level checkpoint.
    fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<Option<CheckpointRecord>, StorageError>> + Send;

    /// Update an operation-level checkpoint status.
    fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Replace an operation-level checkpoint.
    fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Idempotent write of one file checkpoint.
    fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch one file checkpoint.
    fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> impl Future<Output = Result<Option<FileCheckpointRecord>, StorageError>> + Send;

    /// Delete file checkpoints of one operation.
    fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Insert a work-unit checkpoint.
    fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Update a work-unit checkpoint status.
    fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// List work-unit checkpoints of one operation stage.
    fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> impl Future<Output = Result<Vec<WorkUnitCheckpointRecord>, StorageError>> + Send;

    /// Fetch one work-unit checkpoint by hash.
    fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> impl Future<Output = Result<Option<WorkUnitCheckpointRecord>, StorageError>> + Send;

    // -- Relation snapshots --

    /// Allocate a building relation epoch for an operation.
    fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    /// Persist a full snapshot and mark its epoch ready, atomically.
    fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Read back the canonical snapshot of an epoch.
    fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<CanonicalRelationSnapshot, StorageError>> + Send;

    /// Fetch the snapshot manifest of an epoch, if present.
    fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Option<RelationSnapshotManifest>, StorageError>> + Send;

    /// Read the incremental delta chain of an epoch range.
    fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> impl Future<Output = Result<Vec<SnapshotDelta>, StorageError>> + Send;

    /// Resolve the base epoch a delta epoch was built from.
    fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> impl Future<Output = Result<Option<i64>, StorageError>> + Send;

    /// Mark a relation epoch failed.
    fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Delete every snapshot row of one epoch.
    fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete every snapshot row of one project.
    fn snapshot_delete_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    // -- Admission audit --

    /// Record one admitted ingest batch.
    fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Record one rejection with its cause.
    fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch one audit row by fingerprint.
    fn admission_get(
        &self,
        fingerprint: &str,
    ) -> impl Future<Output = Result<Option<AdmissionAuditRecord>, StorageError>> + Send;

    /// List every audit row ordered by fingerprint.
    fn admission_list(
        &self,
    ) -> impl Future<Output = Result<Vec<AdmissionAuditRecord>, StorageError>> + Send;

    // -- Capacity --

    /// Aggregate on-disk size (local: main plus per-project files; remote:
    /// whole database size plus per-project estimate where available).
    fn db_size(&self) -> impl Future<Output = Result<u64, StorageError>> + Send;

    /// Remove a project: local deletes the per-project database files
    /// (evict handle, then main plus WAL/SHM sidecars); remote deletes the
    /// project rows across business tables in one transaction.
    /// Returns the number of removed business records; snapshot-only
    /// deletion stays on `snapshot_delete_project`.
    fn delete_project_db(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

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
