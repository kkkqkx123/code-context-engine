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
mod projection;

pub use checkpoint::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, FileCheckpointRecord,
    WorkUnitCheckpointRecord, WorkUnitStatus, validate_checkpoint,
};
pub use chunk::{ChunkContent, ChunkRecord, EntityDetailMapping};
pub use entity::{DbId, EntityRecord, EntityWithFile, FileEnsureEntry, FileHashEntry, FileRecord};
pub use projection::IndexStateProjection;
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

    /// Read an integer project metadata value, if the row exists.
    ///
    /// A missing row is the legitimate default (`None`); unparseable
    /// values and backend failures are errors.
    fn project_meta_get_int_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> impl Future<Output = Result<Option<i64>, StorageError>> + Send;

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

    /// Fetch the building manifest registered for an operation, if any.
    fn manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<Option<ProjectIndexManifest>, StorageError>> + Send;

    /// Highest building data epoch of one project, if any.
    fn manifest_building_max_epoch(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Option<i64>, StorageError>> + Send;

    /// Link (or unlink, with `None`) a building manifest to its parent.
    fn manifest_set_parent(
        &self,
        project_id: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Whether an in-progress checkpoint of the given operation type exists.
    fn manifest_has_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send;

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

    /// Upsert override dispositions of one generation atomically.
    fn overrides_upsert_batch(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, OverrideDisposition)],
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Clear the override registrations of one generation.
    fn overrides_clear(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

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

    /// Visible `(path, content_hash)` pairs of the active generation view.
    ///
    /// Own-generation rows win over inherited ones; overrides hide
    /// inherited rows but never rows the published generation owns itself.
    fn files_visible_hashes(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Vec<FileHashEntry>, StorageError>> + Send;

    /// Record content hashes at an epoch (idempotent per path).
    fn files_record_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, String)],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Drop hash rows of paths at an epoch.
    fn files_remove_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Ensure file rows of a batch without touching content hashes.
    fn files_ensure(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        entries: &[FileEnsureEntry],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete every file row of one path across all epochs.
    ///
    /// Entity and mapping rows follow through the foreign-key cascade.
    fn files_delete_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Latest file row of one path across all epochs, if any.
    fn file_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> impl Future<Output = Result<Option<FileRecord>, StorageError>> + Send;

    /// Idempotent batch write of entity rows (records carry project/epoch).
    fn entities_upsert(
        &self,
        entities: &[EntityRecord],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Batch insert of entity rows returning the assigned row ids.
    fn entities_insert_returning_ids(
        &self,
        entities: &[EntityRecord],
    ) -> impl Future<Output = Result<Vec<i64>, StorageError>> + Send;

    /// Entity rows of one generation joined with their file paths.
    fn entities_with_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<EntityWithFile>, StorageError>> + Send;

    /// Entity rows by row id scoped to one project.
    fn entities_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> impl Future<Output = Result<Vec<EntityRecord>, StorageError>> + Send;

    /// Entity rows of one file scoped to one project.
    fn entities_by_file(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> impl Future<Output = Result<Vec<EntityRecord>, StorageError>> + Send;

    /// Delete entity rows of one file at an epoch.
    fn entities_delete_by_file(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Repoint a child entity at its parent inside one generation.
    fn entities_set_parent(
        &self,
        project_id: i64,
        epoch: i64,
        child_id: i64,
        parent_id: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

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

    /// Full-text entity search scoped to one project and epoch.
    ///
    /// The local branch serves SQLite FTS5; the remote branch serves a
    /// portable substring match over name, signature and scoped name.
    /// Rows whose file path is in `excluded_files` are dropped inside the
    /// branch. An empty query after sanitization returns no rows.
    fn entities_search_fts(
        &self,
        project_id: i64,
        query: &str,
        limit: i64,
        epoch: i64,
        excluded_files: &[String],
    ) -> impl Future<Output = Result<Vec<EntityRecord>, StorageError>> + Send;

    /// List file rows of one project across the given epochs, ordered by path.
    ///
    /// Backs index-based file search on branches without live project files.
    fn files_list(
        &self,
        project_id: i64,
        epochs: &[i64],
    ) -> impl Future<Output = Result<Vec<FileRecord>, StorageError>> + Send;

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

    /// Chunk texts of one generation for BM25 document reconstruction.
    fn chunks_content_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<ChunkContent>, StorageError>> + Send;

    /// Largest entity id referenced by chunk rows of one generation.
    fn chunks_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Option<u64>, StorageError>> + Send;

    /// Delete every chunk row of one file across all epochs.
    fn chunks_delete_by_file(
        &self,
        project_id: i64,
        path: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Write chunk rows and entity detail mappings in one transaction.
    fn chunks_and_mappings_upsert(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> impl Future<Output = Result<(usize, usize), StorageError>> + Send;

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

    /// Detail mapping of one entity at an epoch, if present.
    fn mappings_by_entity(
        &self,
        project_id: i64,
        entity_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Option<EntityDetailMapping>, StorageError>> + Send;

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

    /// Fetch the active checkpoint of one project, if any.
    fn checkpoint_active(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Option<CheckpointRecord>, StorageError>> + Send;

    /// Mark an operation checkpoint active.
    fn checkpoint_set_active(
        &self,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Clear the active marker of an operation.
    fn checkpoint_clear_active(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Refresh the heartbeat of an active operation.
    fn checkpoint_heartbeat(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Clear active markers idle longer than `stale_threshold_secs`.
    fn checkpoints_cleanup_stale(
        &self,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// List in-progress operations of one project.
    fn checkpoints_unfinished(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Vec<CheckpointRecord>, StorageError>> + Send;

    /// Latest incomplete checkpoint of an operation type and root.
    fn checkpoint_latest_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> impl Future<Output = Result<Option<CheckpointRecord>, StorageError>> + Send;

    /// Update the batch cursor of an operation checkpoint.
    fn checkpoint_set_batch_index(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Create a batch-level checkpoint.
    fn checkpoint_batch_create(
        &self,
        project_id: i64,
        batch: &BatchCheckpointRecord,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Fetch a batch-level checkpoint.
    fn checkpoint_batch_get(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> impl Future<Output = Result<Option<BatchCheckpointRecord>, StorageError>> + Send;

    /// List file checkpoints of one batch.
    fn checkpoint_batch_files(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> impl Future<Output = Result<Vec<FileCheckpointRecord>, StorageError>> + Send;

    /// List failed files of one operation as `(path, error)` pairs.
    fn checkpoint_failed_files(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<Vec<(String, String)>, StorageError>> + Send;

    /// Delete batch checkpoints of one operation.
    fn checkpoint_batches_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete file checkpoints of one operation.
    fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Clear per-module progress markers of one operation.
    fn file_checkpoints_clear_module_progress(
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

    /// Batch insert of work-unit checkpoints returning row ids.
    fn work_units_insert_batch(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> impl Future<Output = Result<Vec<i64>, StorageError>> + Send;

    /// Delete work-unit checkpoints of one operation.
    fn work_units_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

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

    /// Fetch the snapshot manifest registered for an operation, if any.
    fn snapshot_manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<Option<RelationSnapshotManifest>, StorageError>> + Send;

    /// Activate a previously written relation epoch.
    fn snapshot_activate(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Reopen a failed relation epoch for another write attempt.
    fn snapshot_retry_failed(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Reclaim relation rows older than `stale_before`.
    fn snapshot_collect_garbage(
        &self,
        project_id: i64,
        stale_before: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Count canonical snapshot entities of one relation epoch.
    fn snapshot_entities_count(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> impl Future<Output = Result<i64, StorageError>> + Send;

    // -- Generation materialization --

    /// Copy one file's rows from a source generation into a target one.
    ///
    /// Parent references remap to the copied rows and target-owned rows
    /// are never shadowed; returns the copied entity row count.
    fn generation_copy_file(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Materialize an inherited generation into a full one (compaction).
    ///
    /// Copies every non-overridden source row into the target in one
    /// transaction, replacing crash residue of the same source paths.
    fn generation_materialize(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Compute the retention plan over stale generations.
    fn generation_gc_plan(
        &self,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> impl Future<Output = Result<GenerationGcPlan, StorageError>> + Send;

    /// Apply a retention plan, removing only unprotected generations.
    fn generation_gc_apply(
        &self,
        project_id: i64,
        plan: &GenerationGcPlan,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    // -- Hot-update candidate --

    /// Register a hot-update candidate inheriting from a parent generation.
    ///
    /// One transaction: begin the building manifest, link the parent, clear
    /// residual rows and overrides of the epoch number, mark it ready.
    fn candidate_begin(
        &self,
        project_id: i64,
        candidate_epoch: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Clear every candidate-generation row plus its override registrations.
    fn candidate_clear_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Clear one file set's candidate-generation rows (chunks, entities).
    fn candidate_delete_file_rows(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Prepare one file set for rewriting on a channel (`nl` or `emb`).
    ///
    /// Drops the channel's chunk rows of the candidate epoch and registers
    /// `replaced` overrides so the parent rows stay hidden.
    fn candidate_prepare_channel(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
        channel: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    // -- File-level state projection --

    /// Load the durable projection rows of one operation.
    fn projection_load(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<Vec<IndexStateProjection>, StorageError>> + Send;

    /// Upsert a projection row; an older version never overwrites a newer one.
    fn projection_upsert(
        &self,
        row: &IndexStateProjection,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Delete one file's projection row inside an operation.
    fn projection_delete(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete every projection row of one operation.
    fn projection_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> impl Future<Output = Result<usize, StorageError>> + Send;

    /// Delete every projection row of one project.
    fn projection_delete_by_project(
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
