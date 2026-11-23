//! Metadata storage contract (backend-neutral).
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
pub use entity::{
    DbId, EntityFileWrite, EntityRecord, EntityWithFile, FileEnsureEntry, FileEpochEntry,
    FileHashEntry, FileRecord,
};
pub use generation::{
    AdmissionAuditRecord, GenerationGcPlan, GenerationOverride, OverrideDisposition,
    ProjectIndexManifest, ProjectIndexManifestState,
};
pub use project::{NewProjectRecord, ProjectRecord, ProjectUpdateRecord};
pub use projection::IndexStateProjection;

mod backend;
#[macro_use]
mod contract;

pub use backend::{MetadataBackendStore, MetadataProjectScope};

metadata_storage_contract! {
    metadata_project: MetadataProjectStore {
        /// Ensure the project registry row exists (idempotent).
    fn ensure_project(
    &self,
    project_id: i64,
    root_path: &str,
    ) -> Result<(), StorageError>;
        /// Fetch the project registry row, if present.
    fn project_record(
    &self,
    project_id: i64,
    ) -> Result<Option<ProjectRecord>, StorageError>;
        /// List every registered project, newest first.
    fn projects_all(&self)
    -> Result<Vec<ProjectRecord>, StorageError>;
        /// Apply a partial project-row update.
    fn project_update(
    &self,
    project_id: i64,
    updates: &ProjectUpdateRecord,
    ) -> Result<(), StorageError>;
        /// Register a project row; returns the new id.
        ///
        /// Uniqueness violations on name or root path surface as errors whose
        /// message contains `UNIQUE constraint failed`.
    fn project_insert(
    &self,
    record: &NewProjectRecord,
    ) -> Result<i64, StorageError>;
        /// Whether a project row already uses `root_path`.
    fn project_path_exists(
    &self,
    root_path: &str,
    ) -> Result<bool, StorageError>;
        /// Delete one project's record together with its checkpoint rows.
    fn project_delete_with_cascade(
    &self,
    project_id: i64,
    ) -> Result<(), StorageError>;
        /// Remove one project's database files; returns the removed file count.
    fn project_delete_db(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Drop one project's index data while keeping the project row.
        ///
        /// Clears files, entities, chunks, entity mappings, relation snapshots,
        /// project metadata and checkpoints in one storage-side transaction.
    fn project_clear_index(
    &self,
    project_id: i64,
    ) -> Result<(), StorageError>;
        /// Read an integer project metadata value.
    fn project_meta_get_int(
    &self,
    project_id: i64,
    key: &str,
    ) -> Result<i64, StorageError>;
        /// Write an integer project metadata value.
    fn project_meta_set_int(
    &self,
    project_id: i64,
    key: &str,
    value: i64,
    ) -> Result<(), StorageError>;
        /// Read an integer project metadata value, if the row exists.
        ///
        /// A missing row is the legitimate default (`None`); unparseable
        /// values and backend failures are errors.
    fn project_meta_get_int_optional(
    &self,
    project_id: i64,
    key: &str,
    ) -> Result<Option<i64>, StorageError>;
        /// Read a string project metadata value, if the row exists.
    fn project_meta_get_string_optional(
    &self,
    project_id: i64,
    key: &str,
    ) -> Result<Option<String>, StorageError>;
        /// Write a string project metadata value.
    fn project_meta_set_string(
    &self,
    project_id: i64,
    key: &str,
    value: &str,
    ) -> Result<(), StorageError>;
    }
    metadata_manifest: MetadataManifestStore {
        /// Allocate (or reattach to) the building manifest for an operation.
    fn manifest_begin_building(
    &self,
    project_id: i64,
    data_epoch: i64,
    operation_id: &str,
    input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError>;
        /// Mark the inheritance registration of a candidate complete.
    fn manifest_mark_candidate_ready(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<(), StorageError>;
        /// Atomically activate a generation: the manifest row, the relation
        /// snapshot state, and the project metadata advance in one transaction.
    fn manifest_activate(
    &self,
    project_id: i64,
    data_epoch: i64,
    relation_epoch: i64,
    operation_id: &str,
    input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError>;
        /// Mark the building manifest of an operation failed.
    fn manifest_mark_failed(
    &self,
    project_id: i64,
    operation_id: &str,
    reason: &str,
    ) -> Result<(), StorageError>;
        /// Fetch the currently active manifest, if any.
    fn manifest_active(
    &self,
    project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError>;
        /// Fetch the building manifest registered for an operation, if any.
    fn manifest_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<Option<ProjectIndexManifest>, StorageError>;
        /// Highest building data epoch of one project, if any.
    fn manifest_building_max_epoch(
    &self,
    project_id: i64,
    ) -> Result<Option<i64>, StorageError>;
        /// Link (or unlink, with `None`) a building manifest to its parent.
    fn manifest_set_parent(
    &self,
    project_id: i64,
    operation_id: &str,
    parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError>;
        /// Parent data epoch of one data epoch, if the generation inherits.
    fn manifest_parent_epoch(
    &self,
    project_id: i64,
    data_epoch: i64,
    ) -> Result<Option<i64>, StorageError>;
        /// Whether an in-progress checkpoint of the given operation type exists.
    fn manifest_has_incomplete(
    &self,
    project_id: i64,
    operation_type: &str,
    ) -> Result<bool, StorageError>;
        /// Recycle one data epoch: content rows, overrides, manifest rows, and
        /// snapshot rows of the epoch are removed in one transaction.
    fn manifest_recycle_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
    }
    metadata_generation: MetadataGenerationStore {
        /// Replace the override set of one generation atomically.
    fn overrides_replace(
    &self,
    project_id: i64,
    epoch: i64,
    overrides: &[GenerationOverride],
    ) -> Result<(), StorageError>;
        /// List the overrides of one generation ordered by file path.
    fn overrides_for_generation(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError>;
        /// Upsert override dispositions of one generation atomically.
    fn overrides_upsert_batch(
    &self,
    project_id: i64,
    epoch: i64,
    entries: &[(String, OverrideDisposition)],
    ) -> Result<(), StorageError>;
        /// Clear the override registrations of one generation.
    fn overrides_clear(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
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
    ) -> Result<usize, StorageError>;
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
    ) -> Result<(), StorageError>;
        /// Compute the retention plan over stale generations.
    fn generation_gc_plan(
    &self,
    project_id: i64,
    keep_active_generations: usize,
    stale_before: i64,
    ) -> Result<GenerationGcPlan, StorageError>;
        /// Apply a retention plan, removing only unprotected generations.
    fn generation_gc_apply(
    &self,
    project_id: i64,
    plan: &GenerationGcPlan,
    ) -> Result<(), StorageError>;
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
    ) -> Result<(), StorageError>;
        /// Clear every candidate-generation row plus its override registrations.
    fn candidate_clear_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Clear one file set's candidate-generation rows (chunks, entities).
    fn candidate_delete_file_rows(
    &self,
    project_id: i64,
    epoch: i64,
    paths: &[String],
    ) -> Result<usize, StorageError>;
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
    ) -> Result<(), StorageError>;
    }
    metadata_content: MetadataContentStore {
        /// Idempotent batch write of file rows for one project and epoch.
    fn files_upsert(
    &self,
    project_id: i64,
    epoch: i64,
    files: &[FileRecord],
    ) -> Result<usize, StorageError>;
        /// Delete file rows of one project and epoch.
    fn files_delete_by_project_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Delete all file rows of one project.
    fn files_delete_by_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Visible `(path, content_hash)` pairs of the active generation view.
        ///
        /// Own-generation rows win over inherited ones; overrides hide
        /// inherited rows but never rows the published generation owns itself.
    fn files_visible_hashes(
    &self,
    project_id: i64,
    ) -> Result<Vec<FileHashEntry>, StorageError>;
        /// Record content hashes at an epoch (idempotent per path).
    fn files_record_hashes(
    &self,
    project_id: i64,
    epoch: i64,
    entries: &[(String, String)],
    ) -> Result<usize, StorageError>;
        /// Drop hash rows of paths at an epoch.
    fn files_remove_hashes(
    &self,
    project_id: i64,
    epoch: i64,
    paths: &[String],
    ) -> Result<usize, StorageError>;
        /// Ensure file rows of a batch without touching content hashes.
    fn files_ensure(
    &self,
    project_id: i64,
    epoch: i64,
    batch_id: i64,
    entries: &[FileEnsureEntry],
    ) -> Result<usize, StorageError>;
        /// Delete every file row of one path across all epochs.
        ///
        /// Entity and mapping rows follow through the foreign-key cascade.
    fn files_delete_by_path(
    &self,
    project_id: i64,
    path: &str,
    ) -> Result<usize, StorageError>;
        /// File rows by row id scoped to one project.
    fn files_by_ids(
    &self,
    project_id: i64,
    ids: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError>;
        /// Latest file row of one path across all epochs, if any.
    fn file_by_path(
    &self,
    project_id: i64,
    path: &str,
    ) -> Result<Option<FileRecord>, StorageError>;
        /// Idempotent batch write of entity rows (records carry project/epoch).
    fn entities_upsert(
    &self,
    entities: &[EntityRecord],
    ) -> Result<usize, StorageError>;
        /// Batch insert of entity rows returning the assigned row ids.
    fn entities_insert_returning_ids(
    &self,
    entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError>;
        /// Replace the entity rows of several files in one transaction.
        ///
        /// Each entry ensures its file row, deletes that file's rows at the
        /// epoch, inserts all entities and links parents by local index. The
        /// operation commits once and rolls back as a whole on failure.
    fn entities_replace_files(
    &self,
    project_id: i64,
    epoch: i64,
    batch_id: i64,
    files: &[EntityFileWrite],
    ) -> Result<(), StorageError>;
        /// Entity rows of one generation joined with their file paths.
    fn entities_with_files(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError>;
        /// Entity rows of one generation with row ids greater than `after_id`.
        ///
        /// Lets a long indexing run extend an in-memory id map instead of
        /// rescanning every entity after each batch. Backends without a native
        /// cursor keep the same semantics by filtering the full listing.
    fn entities_with_files_after(
    &self,
    project_id: i64,
    epoch: i64,
    after_id: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError>;
        /// Entity rows by row id scoped to one project.
    fn entities_by_ids(
    &self,
    project_id: i64,
    ids: &[i64],
    ) -> Result<Vec<EntityRecord>, StorageError>;
        /// Entity rows of one file scoped to one project.
    fn entities_by_file(
    &self,
    project_id: i64,
    file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError>;
        /// Delete one entity row by id; returns whether a row was removed.
    fn entities_delete_by_id(
    &self,
    project_id: i64,
    entity_id: i64,
    ) -> Result<bool, StorageError>;
        /// Delete entity rows of one file at an epoch.
    fn entities_delete_by_file(
    &self,
    file_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Repoint a child entity at its parent inside one generation.
    fn entities_set_parent(
    &self,
    project_id: i64,
    epoch: i64,
    child_id: i64,
    parent_id: i64,
    ) -> Result<(), StorageError>;
        /// Delete entity rows of one project and epoch.
    fn entities_delete_by_project_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Delete all entity rows of one project.
    fn entities_delete_by_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Entity row ids of one file at one epoch, ascending.
        ///
        /// Backs cross-epoch stability assertions without hydrating full rows.
    fn entity_ids_by_file_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    path: &str,
    ) -> Result<Vec<i64>, StorageError>;
        /// Count entity rows of one project and epoch.
    fn entities_count(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<i64, StorageError>;
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
    ) -> Result<Vec<EntityRecord>, StorageError>;
        /// List file rows of one project across the given epochs, ordered by path.
        ///
        /// Backs index-based file search on branches without live project files.
    fn files_list(
    &self,
    project_id: i64,
    epochs: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError>;
        /// Page chunk rows of one project and epoch with their file category.
    fn chunks_page_with_category(
    &self,
    project_id: i64,
    epoch: i64,
    limit: i64,
    offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError>;
        /// List `(path, batch_id, content_hash)` of one project and epoch ordered by path.
        ///
        /// Backs recovery classification without hydrating full file rows.
    fn files_epoch_entries(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Vec<FileEpochEntry>, StorageError>;
        /// Point one path's file row at `epoch` (recovery re-parse marking).
        ///
        /// Returns whether a row was updated.
    fn files_set_epoch(
    &self,
    project_id: i64,
    path: &str,
    epoch: i64,
    ) -> Result<bool, StorageError>;
        /// Latest cached parsed bytes for `(path, content_hash)` from an
        /// in-progress operation, if any.
    fn checkpoint_cached_parsed_data(
    &self,
    project_id: i64,
    file_path: &str,
    content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError>;
        /// Chunk rows of one file at one epoch, ordered by chunk id.
    fn chunks_by_file_epoch(
    &self,
    project_id: i64,
    file_path: &str,
    epoch: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError>;
        /// Chunk ids of one file at one epoch, ordered by chunk id.
    fn chunks_by_file(
    &self,
    project_id: i64,
    file_path: &str,
    epoch: i64,
    ) -> Result<Vec<String>, StorageError>;
        /// Count file rows of one project and epoch.
    fn files_count(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<i64, StorageError>;
        /// Whether a file row exists at one project, epoch and path.
    fn files_exists_at_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    path: &str,
    ) -> Result<bool, StorageError>;
        /// Page file rows of one project and epoch ordered by row id.
    fn files_page(
    &self,
    project_id: i64,
    epoch: i64,
    limit: i64,
    offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError>;
        /// Idempotent batch write of chunk rows (records carry project/epoch).
    fn chunks_upsert(
    &self,
    chunks: &[ChunkRecord],
    ) -> Result<usize, StorageError>;
        /// Read back chunk rows by id for enrichment. An empty epoch list
        /// disables epoch filtering.
    fn chunks_by_ids(
    &self,
    project_id: i64,
    chunk_ids: &[String],
    epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError>;
        /// Delete chunk rows of one project and epoch.
    fn chunks_delete_by_project_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Delete all chunk rows of one project.
    fn chunks_delete_by_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Count chunk rows of one file across all epochs.
    fn chunks_count_by_path(
    &self,
    project_id: i64,
    file_path: &str,
    ) -> Result<i64, StorageError>;
        /// Count chunk rows of one project and epoch.
    fn chunks_count(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<i64, StorageError>;
        /// Chunk texts of one generation for BM25 document reconstruction.
    fn chunks_content_by_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Vec<ChunkContent>, StorageError>;
        /// Chunk row and truncated-marker counts of one generation channel.
    fn chunks_stats(
    &self,
    project_id: i64,
    epoch: i64,
    channel: &str,
    ) -> Result<(i64, i64), StorageError>;
        /// Largest entity id referenced by chunk rows of one generation.
    fn chunks_max_entity_id(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Option<u64>, StorageError>;
        /// Delete every chunk row of one file across all epochs.
    fn chunks_delete_by_file(
    &self,
    project_id: i64,
    path: &str,
    ) -> Result<usize, StorageError>;
        /// Write chunk rows and entity detail mappings in one transaction.
    fn chunks_and_mappings_upsert(
    &self,
    chunks: &[ChunkRecord],
    mappings: &[EntityDetailMapping],
    ) -> Result<(usize, usize), StorageError>;
        /// Idempotent batch write of entity detail mappings.
    fn mappings_upsert(
    &self,
    mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError>;
        /// Delete mapping rows of one project and epoch.
    fn mappings_delete_by_project_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Delete all mapping rows of one project.
    fn mappings_delete_by_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Detail mapping of one entity at an epoch, if present.
    fn mappings_by_entity(
    &self,
    project_id: i64,
    entity_id: i64,
    epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError>;
    }
    metadata_checkpoint: MetadataCheckpointStore {
        /// Create an operation-level checkpoint.
    fn checkpoint_create(
    &self,
    project_id: i64,
    checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError>;
        /// Fetch an operation-level checkpoint.
    fn checkpoint_get(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError>;
        /// Update an operation-level checkpoint status.
    fn checkpoint_set_status(
    &self,
    project_id: i64,
    operation_id: &str,
    status: CheckpointStatus,
    ) -> Result<(), StorageError>;
        /// Replace an operation-level checkpoint.
    fn checkpoint_update(
    &self,
    project_id: i64,
    checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError>;
        /// Idempotent write of one file checkpoint.
    fn file_checkpoint_upsert(
    &self,
    project_id: i64,
    file: &FileCheckpointRecord,
    ) -> Result<(), StorageError>;
        /// Fetch one file checkpoint.
    fn file_checkpoint_get(
    &self,
    project_id: i64,
    operation_id: &str,
    file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError>;
        /// Fetch the active checkpoint of one project, if any.
    fn checkpoint_active(
    &self,
    project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError>;
        /// Mark an operation checkpoint active.
    fn checkpoint_set_active(
    &self,
    project_id: i64,
    operation_id: &str,
    priority: i32,
    ) -> Result<(), StorageError>;
        /// Clear the active marker of an operation.
    fn checkpoint_clear_active(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
        /// Refresh the heartbeat of an active operation.
    fn checkpoint_heartbeat(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<(), StorageError>;
        /// Clear active markers idle longer than `stale_threshold_secs`.
    fn checkpoints_cleanup_stale(
    &self,
    project_id: i64,
    stale_threshold_secs: i64,
    ) -> Result<usize, StorageError>;
        /// Delete terminal checkpoints older than `ttl_seconds`.
        ///
        /// Only `completed`/`failed` rows whose `updated_at` precedes the cutoff
        /// and that carry no active flag are removed; returns the deleted count.
    fn checkpoints_delete_expired(
    &self,
    project_id: i64,
    ttl_seconds: u64,
    ) -> Result<usize, StorageError>;
        /// List in-progress operations of one project.
    fn checkpoints_unfinished(
    &self,
    project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError>;
        /// Latest incomplete checkpoint of an operation type and root.
    fn checkpoint_latest_incomplete(
    &self,
    project_id: i64,
    operation_type: &str,
    root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError>;
        /// Update the batch cursor of an operation checkpoint.
    fn checkpoint_set_batch_index(
    &self,
    project_id: i64,
    operation_id: &str,
    batch_index: u32,
    ) -> Result<(), StorageError>;
        /// Create a batch-level checkpoint.
    fn checkpoint_batch_create(
    &self,
    project_id: i64,
    batch: &BatchCheckpointRecord,
    ) -> Result<(), StorageError>;
        /// Fetch a batch-level checkpoint.
    fn checkpoint_batch_get(
    &self,
    project_id: i64,
    operation_id: &str,
    batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError>;
        /// List file checkpoints of one batch.
    fn checkpoint_batch_files(
    &self,
    project_id: i64,
    operation_id: &str,
    batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError>;
        /// List failed files of one operation as `(path, error)` pairs.
    fn checkpoint_failed_files(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError>;
        /// Delete batch checkpoints of one operation.
    fn checkpoint_batches_delete_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
        /// Delete file checkpoints of one operation.
    fn checkpoint_files_delete_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
        /// Overwrite one file checkpoint's parsed envelope bytes.
        ///
        /// Returns whether a row was updated.
    fn checkpoint_file_set_parsed_data(
    &self,
    project_id: i64,
    operation_id: &str,
    file_path: &str,
    data: &[u8],
    ) -> Result<bool, StorageError>;
        /// `(entity id, Qdrant point ids JSON)` mappings of one file at one
        /// epoch, ordered by entity id.
    fn mappings_point_ids_by_file_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    path: &str,
    ) -> Result<Vec<(i64, String)>, StorageError>;
        /// Delete detail mappings of one file's entities.
    fn mappings_delete_by_file_id(
    &self,
    project_id: i64,
    file_id: i64,
    ) -> Result<usize, StorageError>;
        /// Delete detail mappings of one entity.
    fn mappings_delete_by_entity_id(
    &self,
    project_id: i64,
    entity_id: i64,
    ) -> Result<usize, StorageError>;
        /// Clear per-module progress markers of one operation.
    fn file_checkpoints_clear_module_progress(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
        /// Insert a work-unit checkpoint.
    fn work_unit_insert(
    &self,
    record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError>;
        /// Update a work-unit checkpoint status.
    fn work_unit_set_status(
    &self,
    project_id: i64,
    operation_id: &str,
    stage: &str,
    work_unit_hash: &str,
    status: WorkUnitStatus,
    ) -> Result<(), StorageError>;
        /// List work-unit checkpoints of one operation stage.
    fn work_units_list(
    &self,
    project_id: i64,
    operation_id: &str,
    stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError>;
        /// Fetch one work-unit checkpoint by hash.
    fn work_unit_by_hash(
    &self,
    project_id: i64,
    operation_id: &str,
    stage: &str,
    work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError>;
        /// Batch insert of work-unit checkpoints returning row ids.
    fn work_units_insert_batch(
    &self,
    records: &[WorkUnitCheckpointRecord],
    ) -> Result<Vec<i64>, StorageError>;
        /// Delete work-unit checkpoints of one operation.
    fn work_units_delete_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
    }
    metadata_snapshot: MetadataSnapshotStore {
        /// Allocate a building relation epoch for an operation.
    fn snapshot_allocate(
    &self,
    project_id: i64,
    operation_id: &str,
    config_fingerprint: &str,
    ) -> Result<i64, StorageError>;
        /// Persist a full snapshot and mark its epoch ready, atomically.
    fn snapshot_write_ready(
    &self,
    project_id: i64,
    epoch: i64,
    snapshot: &CanonicalRelationSnapshot,
    input_fingerprint: &str,
    snapshot_fingerprint: &str,
    ) -> Result<(), StorageError>;
        /// Read back the canonical snapshot of an epoch.
    fn snapshot_read(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<CanonicalRelationSnapshot, StorageError>;
        /// Fetch the snapshot manifest of an epoch, if present.
    fn snapshot_manifest(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError>;
        /// Persist one delta onto a building epoch allocated by
        /// [`MetadataStorage::snapshot_allocate`].
    fn snapshot_write_delta(
    &self,
    project_id: i64,
    epoch: i64,
    delta: &SnapshotDelta,
    ) -> Result<(), StorageError>;
        /// Read the incremental delta chain of an epoch range.
    fn snapshot_delta_chain(
    &self,
    project_id: i64,
    after_epoch: i64,
    up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError>;
        /// Resolve the base epoch a delta epoch was built from.
    fn snapshot_find_base(
    &self,
    project_id: i64,
    delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError>;
        /// Mark a relation epoch failed.
    fn snapshot_mark_failed(
    &self,
    project_id: i64,
    epoch: i64,
    reason: &str,
    ) -> Result<(), StorageError>;
        /// Delete snapshot manifests of every epoch but `keep_epoch`.
    fn snapshot_delete_manifests_except(
    &self,
    project_id: i64,
    keep_epoch: i64,
    ) -> Result<usize, StorageError>;
        /// `(chain length, cumulative delta bytes, base snapshot bytes)` of the
        /// delta chain ending at `active_epoch`; zeros when there is no chain.
    fn snapshot_delta_chain_info(
    &self,
    project_id: i64,
    active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError>;
        /// Delete every snapshot row of one epoch.
    fn snapshot_delete_epoch(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<usize, StorageError>;
        /// Delete every snapshot row of one project.
    fn snapshot_delete_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
        /// Fetch the snapshot manifest registered for an operation, if any.
    fn snapshot_manifest_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError>;
        /// Activate a previously written relation epoch.
    fn snapshot_activate(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<(), StorageError>;
        /// Reopen a failed relation epoch for another write attempt.
    fn snapshot_retry_failed(
    &self,
    project_id: i64,
    epoch: i64,
    ) -> Result<(), StorageError>;
        /// Reclaim relation rows older than `stale_before`.
    fn snapshot_collect_garbage(
    &self,
    project_id: i64,
    stale_before: i64,
    ) -> Result<usize, StorageError>;
        /// Count canonical snapshot entities of one relation epoch.
    fn snapshot_entities_count(
    &self,
    project_id: i64,
    relation_epoch: i64,
    ) -> Result<i64, StorageError>;
    }
    metadata_projection: MetadataProjectionStore {
        /// Load the durable projection rows of one operation.
    fn projection_load(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<Vec<IndexStateProjection>, StorageError>;
        /// Upsert a projection row; an older version never overwrites a newer one.
    fn projection_upsert(
    &self,
    row: &IndexStateProjection,
    ) -> Result<(), StorageError>;
        /// Upsert a batch of projection rows in one transaction.
    fn projection_upsert_batch(
    &self,
    rows: &[IndexStateProjection],
    ) -> Result<(), StorageError>;
        /// Delete one file's projection row inside an operation.
    fn projection_delete(
    &self,
    project_id: i64,
    operation_id: &str,
    file_path: &str,
    ) -> Result<usize, StorageError>;
        /// Delete every projection row of one operation.
    fn projection_delete_by_operation(
    &self,
    project_id: i64,
    operation_id: &str,
    ) -> Result<usize, StorageError>;
        /// Delete every projection row of one project.
    fn projection_delete_by_project(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
    }
    metadata_admission: MetadataAdmissionStore {
        /// Record one admitted ingest batch.
    fn admission_record_admitted(
    &self,
    fingerprint: &str,
    projects: &[i64],
    quota_bytes: Option<u64>,
    bytes: u64,
    ) -> Result<(), StorageError>;
        /// Record one rejection with its cause.
    fn admission_record_rejection(
    &self,
    fingerprint: &str,
    projects: &[i64],
    quota_bytes: Option<u64>,
    reason: &str,
    ) -> Result<(), StorageError>;
        /// Fetch one audit row by fingerprint.
    fn admission_get(
    &self,
    fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError>;
        /// List every audit row ordered by fingerprint.
    fn admission_list(
    &self,
    ) -> Result<Vec<AdmissionAuditRecord>, StorageError>;
    }
    metadata_capacity: MetadataCapacityStore {
        /// Aggregate on-disk size (local: main plus per-project files; remote:
        /// whole database size plus per-project estimate where available).
    fn db_size(&self) -> Result<u64, StorageError>;
        /// Remove a project: local deletes the per-project database files
        /// (evict handle, then main plus WAL/SHM sidecars); remote deletes the
        /// project rows across business tables in one transaction.
        /// Returns the number of removed business records; snapshot-only
        /// deletion stays on `snapshot_delete_project`.
    fn delete_project_db(
    &self,
    project_id: i64,
    ) -> Result<usize, StorageError>;
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_metadata_storage<T: MetadataStorage>() {}
