//! Relation storage contract (backend-neutral).
//!
//! The `RelationStorage` trait is defined in `cce-storage-common` and shared
//! by the embedded SQLite branch and the remote PostgreSQL branch.

use cce_types::StorageError;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta};

use crate::SqliteClient;

pub use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileRecord, GenerationOverride, OverrideDisposition,
    ProjectIndexManifest, ProjectIndexManifestState, ProjectRecord, RelationStorage,
    WorkUnitCheckpointRecord, WorkUnitStatus, assert_relation_storage,
};

pub use crate::repo::{
    AdmissionAuditRepository, CheckpointRepository, EntityDetailMappingRepository,
    EntityRepository, FileRepository, FileSummaryRepository, GenerationOverrideRepository,
    ProjectIndexManifestRepository, ProjectRepository, RelationSnapshotRepository,
};

pub use crate::types::{BatchCheckpointRecord, DbId, NewProjectRecord, ProjectUpdateRecord};

impl RelationStorage for SqliteClient {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        self.with_transaction(|tx| ProjectRepository::ensure(tx, project_id, root_path))
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        let conn = self.read_connection()?;
        ProjectRepository::get_by_id(&conn, project_id)
    }

    async fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        SqliteClient::project_meta_get_int(self, project_id, key)
    }

    async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        SqliteClient::project_meta_set_int(self, project_id, key, value)
    }

    async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::begin_building(
                tx,
                project_id,
                data_epoch,
                operation_id,
                input_fingerprint,
            )
        })
    }

    async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::mark_candidate_ready(tx, project_id, operation_id)
        })
    }

    async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::activate(
                tx,
                project_id,
                data_epoch,
                relation_epoch,
                operation_id,
                input_fingerprint,
            )
        })
    }

    async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::mark_failed(tx, project_id, operation_id, reason)
        })
    }

    async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        let conn = self.read_connection()?;
        ProjectIndexManifestRepository::get_active(&conn, project_id)
    }

    async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            let mut removed = 0usize;
            removed += tx
                .execute(
                    "DELETE FROM files WHERE project_id = ?1 AND epoch = ?2",
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| StorageError::delete("files", error.to_string()))?;
            removed += tx
                .execute(
                    "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2",
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| StorageError::delete("entities", error.to_string()))?;
            removed += tx
                .execute(
                    "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2",
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| StorageError::delete("chunks", error.to_string()))?;
            removed += tx
                .execute(
                    "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2",
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| {
                    StorageError::delete("entity_detail_mappings", error.to_string())
                })?;
            removed += tx
                .execute(
                    "DELETE FROM file_summaries WHERE epoch = ?1 AND file_id IN \
                     (SELECT id FROM files WHERE project_id = ?2)",
                    rusqlite::params![epoch, project_id],
                )
                .map_err(|error| StorageError::delete("file_summaries", error.to_string()))?;
            GenerationOverrideRepository::clear_generation(tx, project_id, epoch)?;
            removed += Self::snapshot_delete_epoch_in(tx, project_id, epoch)?;
            removed += tx
                .execute(
                    "DELETE FROM project_index_manifests WHERE project_id = ?1 AND data_epoch = ?2",
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| {
                    StorageError::delete("project_index_manifests", error.to_string())
                })?;
            Ok(removed)
        })
    }

    async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            GenerationOverrideRepository::clear_generation(tx, project_id, epoch)?;
            for entry in overrides {
                GenerationOverrideRepository::upsert(
                    tx,
                    project_id,
                    epoch,
                    &entry.file_path,
                    entry.disposition,
                )?;
            }
            Ok(())
        })
    }

    async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        let conn = self.read_connection()?;
        GenerationOverrideRepository::list_for_generation(&conn, project_id, epoch)
    }

    async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError> {
        if files.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut count = 0usize;
            for file in files {
                tx.execute(
                    "INSERT INTO files (path, language, category, last_modified, created_at, \
                     project_id, content_hash, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
                     ON CONFLICT(project_id, epoch, path) DO UPDATE SET
                        language = excluded.language, category = excluded.category,
                        last_modified = excluded.last_modified,
                        content_hash = excluded.content_hash",
                    rusqlite::params![
                        file.path,
                        file.language,
                        file.category,
                        file.last_modified,
                        file.created_at,
                        project_id,
                        file.content_hash,
                        epoch,
                    ],
                )
                .map_err(|error| StorageError::insert("files", error.to_string()))?;
                count += 1;
            }
            Ok(count)
        })
    }

    async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM files WHERE project_id = ?1 AND epoch = ?2",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| StorageError::delete("files", error.to_string()))
        })
    }

    async fn files_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM files WHERE project_id = ?1",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("files", error.to_string()))
        })
    }

    async fn entities_upsert(&self, entities: &[EntityRecord]) -> Result<usize, StorageError> {
        if entities.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut count = 0usize;
            for entity in entities {
                tx.execute(
                    "INSERT INTO entities (name, kind, file_id, signature, span_start_row, \
                     span_end_row, span_start_column, span_end_column, span_start_byte, \
                     span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, \
                     return_type, doc_comment, modifiers_json, project_id, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                     ?16, ?17, ?18, ?19, ?20, ?21)
                     ON CONFLICT(project_id, epoch, file_id, scoped_name, kind) DO UPDATE SET
                        name = excluded.name, signature = excluded.signature,
                        span_start_row = excluded.span_start_row, span_end_row = excluded.span_end_row,
                        span_start_column = excluded.span_start_column,
                        span_end_column = excluded.span_end_column,
                        span_start_byte = excluded.span_start_byte, span_end_byte = excluded.span_end_byte,
                        depth = excluded.depth, parent_id = excluded.parent_id,
                        metadata = excluded.metadata, parameters_json = excluded.parameters_json,
                        return_type = excluded.return_type, doc_comment = excluded.doc_comment,
                        modifiers_json = excluded.modifiers_json, batch_id = excluded.batch_id",
                    rusqlite::params![
                        entity.name,
                        entity.kind,
                        entity.file_id,
                        entity.signature,
                        entity.span_start_row,
                        entity.span_end_row,
                        entity.span_start_column,
                        entity.span_end_column,
                        entity.span_start_byte,
                        entity.span_end_byte,
                        entity.scoped_name,
                        entity.depth,
                        entity.parent_id,
                        entity.metadata,
                        entity.parameters_json,
                        entity.return_type,
                        entity.doc_comment,
                        entity.modifiers_json,
                        entity.project_id,
                        entity.epoch,
                        entity.batch_id,
                    ],
                )
                .map_err(|error| StorageError::insert("entities", error.to_string()))?;
                count += 1;
            }
            Ok(count)
        })
    }

    async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| StorageError::delete("entities", error.to_string()))
        })
    }

    async fn entities_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM entities WHERE project_id = ?1",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("entities", error.to_string()))
        })
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        EntityRepository::count_by_project_and_epoch(&conn, project_id, epoch)
    }

    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError> {
        if chunks.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut count = 0usize;
            for chunk in chunks {
                tx.execute(
                    "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, \
                     entity_ids, entity_names, chunk_type, test_status, test_source, created_at, \
                     updated_at, project_id, epoch, batch_id, path, bm25_keywords, segment_id, truncated)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                     ?16, ?17, ?18, ?19)
                     ON CONFLICT(project_id, epoch, chunk_id) DO UPDATE SET
                        file_path = excluded.file_path, content = excluded.content,
                        start_line = excluded.start_line, end_line = excluded.end_line,
                        entity_ids = excluded.entity_ids, entity_names = excluded.entity_names,
                        chunk_type = excluded.chunk_type, test_status = excluded.test_status,
                        test_source = excluded.test_source, updated_at = excluded.updated_at,
                        batch_id = excluded.batch_id, path = excluded.path,
                        bm25_keywords = excluded.bm25_keywords, segment_id = excluded.segment_id,
                        truncated = excluded.truncated",
                    rusqlite::params![
                        chunk.chunk_id,
                        chunk.file_path,
                        chunk.content,
                        chunk.start_line,
                        chunk.end_line,
                        chunk.entity_ids,
                        chunk.entity_names,
                        chunk.chunk_type,
                        chunk.test_status,
                        chunk.test_source,
                        chunk.created_at,
                        chunk.updated_at,
                        chunk.project_id,
                        chunk.epoch,
                        chunk.batch_id,
                        chunk.path,
                        chunk.bm25_keywords,
                        chunk.segment_id,
                        chunk.truncated,
                    ],
                )
                .map_err(|error| StorageError::insert("chunks", error.to_string()))?;
                count += 1;
            }
            Ok(count)
        })
    }

    async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        use crate::repo::ChunkRepository;
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.read_connection()?;
        if epochs.is_empty() {
            return ChunkRepository::get_by_chunk_ids(&conn, chunk_ids, project_id, None);
        }
        let mut out = Vec::new();
        for epoch in epochs {
            out.extend(ChunkRepository::get_by_chunk_ids(
                &conn,
                chunk_ids,
                project_id,
                Some(*epoch),
            )?);
        }
        Ok(out)
    }

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            crate::repo::ChunkRepository::delete_by_file_path(tx, file_path, project_id)
        })
    }

    async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| StorageError::delete("chunks", error.to_string()))
        })
    }

    async fn chunks_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM chunks WHERE project_id = ?1",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("chunks", error.to_string()))
        })
    }

    async fn chunks_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project_id = ?1 AND epoch = ?2",
            rusqlite::params![project_id, epoch],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::query(error.to_string()))
    }

    async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError> {
        if mappings.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut count = 0usize;
            for mapping in mappings {
                EntityDetailMappingRepository::upsert(tx, mapping)?;
                count += 1;
            }
            Ok(count)
        })
    }

    async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| StorageError::delete("entity_detail_mappings", error.to_string()))
        })
    }

    async fn mappings_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM entity_detail_mappings WHERE project_id = ?1",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("entity_detail_mappings", error.to_string()))
        })
    }

    async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            FileSummaryRepository::upsert_with_epoch(tx, file_id, epoch, summary_json)?;
            Ok(())
        })
    }

    async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        let conn = self.read_connection()?;
        FileSummaryRepository::get_by_file_id_at_epoch(&conn, file_id, epoch)
    }

    async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        let conn = self.read_connection()?;
        FileSummaryRepository::list_json_by_epoch(&conn, project_id, epoch)
    }

    async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM file_summaries WHERE epoch = ?1 AND file_id IN \
                 (SELECT id FROM files WHERE project_id = ?2)",
                rusqlite::params![epoch, project_id],
            )
            .map_err(|error| StorageError::delete("file_summaries", error.to_string()))
        })
    }

    async fn summaries_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM file_summaries WHERE file_id IN \
                 (SELECT id FROM files WHERE project_id = ?1)",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("file_summaries", error.to_string()))
        })
    }

    async fn checkpoint_create(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<i64, StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::create_checkpoint(tx, project_id, checkpoint)
        })
    }

    async fn checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_checkpoint(&conn, project_id, operation_id)
    }

    async fn checkpoint_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        status: CheckpointStatus,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::update_checkpoint_status(tx, project_id, operation_id, status)
        })
    }

    async fn checkpoint_update(
        &self,
        project_id: i64,
        checkpoint: &CheckpointRecord,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::update_checkpoint(tx, project_id, checkpoint)
        })
    }

    async fn file_checkpoint_upsert(
        &self,
        project_id: i64,
        file: &FileCheckpointRecord,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::upsert_file_checkpoint(tx, project_id, file)
        })
    }

    async fn file_checkpoint_get(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<Option<FileCheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_file_checkpoint(&conn, project_id, operation_id, file_path)
    }

    async fn checkpoint_files_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::delete_checkpoint_files_by_operation_id(
            &conn,
            project_id,
            operation_id,
        )
    }

    async fn work_unit_insert(
        &self,
        record: &WorkUnitCheckpointRecord,
    ) -> Result<i64, StorageError> {
        self.with_transaction(|tx| CheckpointRepository::insert_work_unit(tx, record))
    }

    async fn work_unit_set_status(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
        status: WorkUnitStatus,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::update_work_unit_status(
                tx,
                project_id,
                operation_id,
                stage,
                work_unit_hash,
                status,
            )
        })
    }

    async fn work_units_list(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
    ) -> Result<Vec<WorkUnitCheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_work_units(&conn, project_id, operation_id, stage)
    }

    async fn work_unit_by_hash(
        &self,
        project_id: i64,
        operation_id: &str,
        stage: &str,
        work_unit_hash: &str,
    ) -> Result<Option<WorkUnitCheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_work_unit_by_hash(
            &conn,
            project_id,
            operation_id,
            stage,
            work_unit_hash,
        )
    }

    async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::allocate_building(
                tx,
                project_id,
                operation_id,
                config_fingerprint,
            )
        })
    }

    async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::write_snapshot_and_mark_ready(
                tx,
                project_id,
                epoch,
                snapshot,
                input_fingerprint,
                snapshot_fingerprint,
            )
        })
    }

    async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        let conn = self.read_connection()?;
        let manifest = RelationSnapshotRepository::get_manifest(&conn, project_id, epoch)?
            .ok_or_else(|| StorageError::not_found(format!("relation snapshot {epoch}")))?;
        RelationSnapshotRepository::read_snapshot(&conn, &manifest)
    }

    async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let conn = self.read_connection()?;
        RelationSnapshotRepository::get_manifest(&conn, project_id, epoch)
    }

    async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        let conn = self.read_connection()?;
        RelationSnapshotRepository::get_delta_chain(&conn, project_id, after_epoch, up_to_epoch)
    }

    async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        let conn = self.read_connection()?;
        RelationSnapshotRepository::find_base_epoch(&conn, project_id, delta_epoch)
    }

    async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::mark_failed(tx, project_id, epoch, reason)
        })
    }

    async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| Self::snapshot_delete_epoch_in(tx, project_id, epoch))
    }

    async fn snapshot_delete_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            let mut removed = 0usize;
            for table in [
                "relation_snapshot_deltas",
                "relation_snapshot_dependencies",
                "relation_snapshot_exports",
                "relation_snapshot_relations",
                "relation_snapshot_entities",
                "relation_snapshot_files",
            ] {
                removed += tx
                    .execute(
                        &format!("DELETE FROM {table} WHERE project_id = ?1"),
                        rusqlite::params![project_id],
                    )
                    .map_err(|error| StorageError::delete(table, error.to_string()))?;
            }
            removed += tx
                .execute(
                    "DELETE FROM relation_snapshot_manifest WHERE project_id = ?1",
                    rusqlite::params![project_id],
                )
                .map_err(|error| {
                    StorageError::delete("relation_snapshot_manifest", error.to_string())
                })?;
            Ok(removed)
        })
    }

    async fn admission_record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            AdmissionAuditRepository::record_admitted(tx, fingerprint, projects, quota_bytes, bytes)
        })
    }

    async fn admission_record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            AdmissionAuditRepository::record_rejection(
                tx,
                fingerprint,
                projects,
                quota_bytes,
                reason,
            )
        })
    }

    async fn admission_get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        let conn = self.read_connection()?;
        AdmissionAuditRepository::get(&conn, fingerprint)
    }

    async fn admission_list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        let conn = self.read_connection()?;
        AdmissionAuditRepository::list_all(&conn)
    }

    async fn db_size(&self) -> Result<u64, StorageError> {
        SqliteClient::db_size(self)
    }

    async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        SqliteClient::delete_project_db(self, project_id)
    }
}

impl SqliteClient {
    /// Delete every snapshot row of one epoch inside an existing transaction.
    ///
    /// Shared by the contract delete and the epoch recycle so both stay in
    /// one atomic unit.
    fn snapshot_delete_epoch_in(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        let mut removed = 0usize;
        for table in [
            "relation_snapshot_dependencies",
            "relation_snapshot_exports",
            "relation_snapshot_relations",
            "relation_snapshot_entities",
            "relation_snapshot_files",
        ] {
            removed += tx
                .execute(
                    &format!("DELETE FROM {table} WHERE project_id = ?1 AND relation_epoch = ?2"),
                    rusqlite::params![project_id, epoch],
                )
                .map_err(|error| StorageError::delete(table, error.to_string()))?;
        }
        // Deltas keyed by base or delta epoch must go before the manifest row.
        removed += tx
            .execute(
                "DELETE FROM relation_snapshot_deltas WHERE project_id = ?1 AND \
                 (base_epoch = ?2 OR delta_epoch = ?2)",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| StorageError::delete("relation_snapshot_deltas", error.to_string()))?;
        removed += tx
            .execute(
                "DELETE FROM relation_snapshot_manifest WHERE project_id = ?1 AND relation_epoch = ?2",
                rusqlite::params![project_id, epoch],
            )
            .map_err(|error| {
                StorageError::delete("relation_snapshot_manifest", error.to_string())
            })?;
        Ok(removed)
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
        // A remote relation branch without parameters is rejected.
        let remote = DatabaseConfig {
            relation_backend: cce_config::modules::RelationBackend::Remote,
            ..DatabaseConfig::default()
        };
        assert!(remote.validate_backend_combination().is_err());
        // Remote branches are forward scaffolding, not an official preset:
        // even with endpoints configured, the full-remote combination needs
        // the advanced switch.
        let mut preset = DatabaseConfig::default();
        preset.vector_backend = cce_config::modules::VectorBackend::Qdrant;
        preset.relation_backend = cce_config::modules::RelationBackend::Remote;
        preset.fulltext_backend = cce_config::modules::FulltextBackend::Remote;
        preset.relation_remote.url = Some("postgres://localhost:5432/cce".to_string());
        preset.fulltext_remote.url = Some("http://localhost:9200".to_string());
        preset.fulltext_remote.index_name = Some("code_index".to_string());
        assert!(preset.validate_backend_combination().is_err());
        preset.allow_nonstandard_backends = true;
        assert!(preset.validate_backend_combination().is_ok());
        // A partial remote mix stays non-preset and needs the advanced switch.
        let mut partial = DatabaseConfig::default();
        partial.fulltext_backend = cce_config::modules::FulltextBackend::Remote;
        partial.fulltext_remote.url = Some("http://localhost:9200".to_string());
        partial.fulltext_remote.index_name = Some("code_index".to_string());
        assert!(partial.validate_backend_combination().is_err());
        partial.allow_nonstandard_backends = true;
        assert!(partial.validate_backend_combination().is_ok());
    }

    #[tokio::test]
    async fn named_operations_round_trip_on_local_branch() {
        let client = SqliteClient::in_memory().expect("in-memory client");
        RelationStorage::ensure_project(&client, 1, "/repo")
            .await
            .expect("ensure project");
        assert!(
            RelationStorage::project_record(&client, 1)
                .await
                .expect("read project")
                .is_some()
        );
        RelationStorage::project_meta_set_int(&client, 1, "active_epoch", 3)
            .await
            .expect("set meta");
        assert_eq!(
            RelationStorage::project_meta_get_int(&client, 1, "active_epoch")
                .await
                .expect("get meta"),
            3
        );

        let manifest = RelationStorage::manifest_begin_building(&client, 1, 3, "op-1", None)
            .await
            .expect("begin building");
        assert_eq!(manifest.data_epoch, 3);
        RelationStorage::manifest_mark_candidate_ready(&client, 1, "op-1")
            .await
            .expect("mark ready");
        let active = RelationStorage::manifest_activate(&client, 1, 3, 0, "op-1", None)
            .await
            .expect("activate");
        assert_eq!(active.state, ProjectIndexManifestState::Active);
        assert!(
            RelationStorage::manifest_active(&client, 1)
                .await
                .expect("active")
                .is_some()
        );

        RelationStorage::overrides_replace(
            &client,
            1,
            3,
            &[GenerationOverride {
                file_path: "a.rs".to_string(),
                disposition: OverrideDisposition::Replaced,
            }],
        )
        .await
        .expect("replace overrides");
        let overrides = RelationStorage::overrides_for_generation(&client, 1, 3)
            .await
            .expect("list overrides");
        assert_eq!(overrides.len(), 1);

        let file = FileRecord {
            id: 0,
            path: "a.rs".to_string(),
            language: "rust".to_string(),
            category: 4,
            last_modified: 1,
            created_at: 1,
            project_id: 1,
            content_hash: Some("hash".to_string()),
        };
        assert_eq!(
            RelationStorage::files_upsert(&client, 1, 3, &[file])
                .await
                .expect("upsert files"),
            1
        );
        assert_eq!(
            RelationStorage::files_upsert(
                &client,
                1,
                3,
                &[FileRecord {
                    id: 0,
                    path: "a.rs".to_string(),
                    language: "rust".to_string(),
                    category: 4,
                    last_modified: 2,
                    created_at: 1,
                    project_id: 1,
                    content_hash: Some("hash2".to_string()),
                }]
            )
            .await
            .expect("replay is idempotent"),
            1
        );

        let chunk = ChunkRecord {
            chunk_id: "c1".to_string(),
            file_path: "a.rs".to_string(),
            content: "fn a() {}".to_string(),
            start_line: 1,
            end_line: 1,
            entity_ids: "[]".to_string(),
            entity_names: "[]".to_string(),
            chunk_type: "code".to_string(),
            test_status: 0,
            test_source: 0,
            created_at: 1,
            updated_at: 1,
            project_id: Some(1),
            epoch: 3,
            batch_id: 0,
            path: "emb".to_string(),
            bm25_keywords: String::new(),
            segment_id: String::new(),
            truncated: 0,
        };
        assert_eq!(
            RelationStorage::chunks_upsert(&client, &[chunk])
                .await
                .expect("upsert chunks"),
            1
        );
        assert_eq!(
            RelationStorage::chunks_count(&client, 1, 3)
                .await
                .expect("count"),
            1
        );
        let readback = RelationStorage::chunks_by_ids(&client, 1, &["c1".to_string()], &[3])
            .await
            .expect("readback");
        assert_eq!(readback.len(), 1);
        assert_eq!(readback[0].content, "fn a() {}");

        RelationStorage::summary_upsert(&client, 1, 3, r#"{"summary_text":"hi"}"#)
            .await
            .expect("upsert summary");
        assert!(
            RelationStorage::summary_at_epoch(&client, 1, 3)
                .await
                .expect("read summary")
                .is_some()
        );

        RelationStorage::admission_record_admitted(&client, "fp", &[1], None, 10)
            .await
            .expect("admit");
        assert!(
            RelationStorage::admission_get(&client, "fp")
                .await
                .expect("get audit")
                .is_some()
        );

        let removed = RelationStorage::manifest_recycle_epoch(&client, 1, 3)
            .await
            .expect("recycle");
        assert!(removed > 0);
        assert_eq!(
            RelationStorage::chunks_count(&client, 1, 3)
                .await
                .expect("count"),
            0
        );
    }
}
