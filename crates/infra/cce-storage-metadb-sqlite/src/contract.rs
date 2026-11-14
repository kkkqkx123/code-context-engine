//! Relation storage contract (backend-neutral).
//!
//! The `RelationStorage` trait is defined in `cce-storage-common` and shared
//! by the embedded SQLite branch and the remote PostgreSQL branch.

use cce_types::StorageError;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta};

use crate::SqliteClient;

pub use cce_storage_common::metadb::{
    AdmissionAuditRecord, BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, ChunkContent,
    ChunkRecord, EntityDetailMapping, EntityRecord, EntityWithFile, FileCheckpointRecord,
    FileEnsureEntry, FileEpochEntry, FileHashEntry, FileRecord, GenerationGcPlan,
    GenerationOverride, IndexStateProjection, NewProjectRecord, OverrideDisposition,
    ProjectIndexManifest, ProjectIndexManifestState, ProjectRecord, RelationStorage,
    WorkUnitCheckpointRecord, WorkUnitStatus, assert_relation_storage, validate_checkpoint,
};

pub use crate::repo::{
    AdmissionAuditRepository, CheckpointRepository, ChunkRepository, EntityDetailMappingRepository,
    EntityRepository, FileRepository, GenerationOverrideRepository, ProjectIndexManifestRepository,
    ProjectRepository, RelationSnapshotRepository,
};

pub use crate::types::{DbId, ProjectUpdateRecord};

impl RelationStorage for SqliteClient {
    async fn ensure_project(&self, project_id: i64, root_path: &str) -> Result<(), StorageError> {
        self.with_transaction(|tx| ProjectRepository::ensure(tx, project_id, root_path))
    }

    async fn project_record(&self, project_id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        let conn = self.read_connection()?;
        ProjectRepository::get_by_id(&conn, project_id)
    }

    async fn projects_all(&self) -> Result<Vec<ProjectRecord>, StorageError> {
        let conn = self.read_connection()?;
        ProjectRepository::get_all(&conn)
    }

    async fn project_update(
        &self,
        project_id: i64,
        updates: &ProjectUpdateRecord,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| ProjectRepository::update(tx, project_id, updates))
    }

    async fn project_insert(&self, record: &NewProjectRecord) -> Result<i64, StorageError> {
        self.with_transaction(|tx| ProjectRepository::insert(tx, record))
    }

    async fn project_path_exists(&self, root_path: &str) -> Result<bool, StorageError> {
        let conn = self.read_connection()?;
        ProjectRepository::path_exists(&conn, root_path)
    }

    async fn project_delete_with_cascade(&self, project_id: i64) -> Result<(), StorageError> {
        self.with_transaction(|tx| ProjectRepository::delete_with_cascade(tx, project_id))
    }

    async fn project_delete_db(&self, project_id: i64) -> Result<usize, StorageError> {
        self.delete_project_db(project_id)
    }

    async fn project_clear_index(&self, project_id: i64) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            use rusqlite::params;
            for table in [
                "files",
                "entities",
                "chunks",
                "entity_detail_mappings",
                "project_meta",
                "checkpoint",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE project_id = ?1"),
                    params![project_id],
                )
                .map_err(|error| StorageError::delete(table, error.to_string()))?;
            }
            Ok(())
        })?;
        self.snapshot_delete_project(project_id).await?;
        Ok(())
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
            let mut stmt = tx
                .prepare(
                    "INSERT INTO files (path, language, category, last_modified, created_at, \
                     project_id, content_hash, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
                     ON CONFLICT(project_id, epoch, path) DO UPDATE SET
                        language = excluded.language, category = excluded.category,
                        last_modified = excluded.last_modified,
                        content_hash = excluded.content_hash",
                )
                .map_err(|error| StorageError::insert("files", error.to_string()))?;
            let mut count = 0usize;
            for file in files {
                stmt.execute(rusqlite::params![
                    file.path,
                    file.language,
                    file.category,
                    file.last_modified,
                    file.created_at,
                    project_id,
                    file.content_hash,
                    epoch,
                ])
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
            let mut stmt = tx
                .prepare(
                    "INSERT INTO entities (name, kind, file_id, signature, span_start_row, \
                     span_end_row, span_start_column, span_end_column, span_start_byte, \
                     span_end_byte, scoped_name, overload_discriminator, depth, parent_id, metadata, parameters_json, \
                     return_type, doc_comment, modifiers_json, project_id, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                     ?16, ?17, ?18, ?19, ?20, ?21, ?22)
                     ON CONFLICT(project_id, epoch, file_id, scoped_name, kind, overload_discriminator) DO UPDATE SET
                        name = excluded.name, signature = excluded.signature,
                        span_start_row = excluded.span_start_row, span_end_row = excluded.span_end_row,
                        span_start_column = excluded.span_start_column,
                        span_end_column = excluded.span_end_column,
                        span_start_byte = excluded.span_start_byte, span_end_byte = excluded.span_end_byte,
                        depth = excluded.depth, parent_id = excluded.parent_id,
                        metadata = excluded.metadata, parameters_json = excluded.parameters_json,
                        return_type = excluded.return_type, doc_comment = excluded.doc_comment,
                        modifiers_json = excluded.modifiers_json, batch_id = excluded.batch_id",
                )
                .map_err(|error| StorageError::insert("entities", error.to_string()))?;
            let mut count = 0usize;
            for entity in entities {
                stmt.execute(
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
                        entity.overload_discriminator,
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

    async fn entity_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<i64>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT e.id FROM entities e JOIN files f ON e.file_id = f.id
                 WHERE e.project_id = ?1 AND e.epoch = ?2 AND f.path = ?3
                 ORDER BY e.id",
            )
            .map_err(|e| StorageError::query(format!("entity_ids_by_file_epoch prepare: {e}")))?;
        stmt.query_map(rusqlite::params![project_id, epoch, path], |row| row.get(0))
            .map_err(|e| StorageError::query(format!("entity_ids_by_file_epoch query: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("entity_ids_by_file_epoch collect: {e}")))
    }

    async fn entities_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        EntityRepository::count_by_project_and_epoch(&conn, project_id, epoch)
    }

    async fn entities_search_fts(
        &self,
        project_id: i64,
        query: &str,
        limit: i64,
        epoch: i64,
        excluded_files: &[String],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        let conn = self.read_connection()?;
        let entities =
            EntityRepository::search_fts_at_epoch(&conn, query, project_id, limit, epoch)?;
        if excluded_files.is_empty() {
            return Ok(entities);
        }
        let excluded: std::collections::HashSet<&str> =
            excluded_files.iter().map(String::as_str).collect();
        let mut kept = Vec::with_capacity(entities.len());
        for entity in entities {
            let visible = FileRepository::get_by_id(&conn, entity.file_id)?
                .is_some_and(|file| !excluded.contains(file.path.as_str()));
            if visible {
                kept.push(entity);
            }
        }
        Ok(kept)
    }

    async fn files_list(
        &self,
        project_id: i64,
        epochs: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        let conn = self.read_connection()?;
        let mut out = Vec::new();
        for epoch in epochs {
            let mut offset = 0i64;
            loop {
                let page = FileRepository::get_by_project_and_epoch_paged(
                    &conn, project_id, *epoch, 1000, offset,
                )?;
                if page.is_empty() {
                    break;
                }
                offset += page.len() as i64;
                out.extend(page);
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    async fn chunks_upsert(&self, chunks: &[ChunkRecord]) -> Result<usize, StorageError> {
        if chunks.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, \
                     entity_ids, entity_names, entity_kinds, group_title, chunk_type, test_status, test_source, created_at, \
                     updated_at, project_id, epoch, batch_id, path, bm25_keywords, segment_id, truncated)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                     ?16, ?17, ?18, ?19, ?20, ?21)
                     ON CONFLICT(project_id, epoch, file_path, chunk_id) DO UPDATE SET
                        file_path = excluded.file_path, content = excluded.content,
                        start_line = excluded.start_line, end_line = excluded.end_line,
                        entity_ids = excluded.entity_ids, entity_names = excluded.entity_names,
                        entity_kinds = excluded.entity_kinds, group_title = excluded.group_title,
                        chunk_type = excluded.chunk_type, test_status = excluded.test_status,
                        test_source = excluded.test_source, updated_at = excluded.updated_at,
                        batch_id = excluded.batch_id, path = excluded.path,
                        bm25_keywords = excluded.bm25_keywords, segment_id = excluded.segment_id,
                        truncated = excluded.truncated",
                )
                .map_err(|error| StorageError::insert("chunks", error.to_string()))?;
            let mut count = 0usize;
            for chunk in chunks {
                stmt.execute(
                    rusqlite::params![
                        chunk.chunk_id,
                        chunk.file_path,
                        chunk.content,
                        chunk.start_line,
                        chunk.end_line,
                        chunk.entity_ids,
                        chunk.entity_names,
                        chunk.entity_kinds,
                        chunk.group_title,
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

    async fn chunks_count_by_path(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project_id = ?1 AND file_path = ?2",
            rusqlite::params![project_id, file_path],
            |row| row.get(0),
        )
        .map_err(|e| StorageError::query(format!("chunks_count_by_path: {e}")))
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

    async fn snapshot_write_delta(
        &self,
        project_id: i64,
        epoch: i64,
        delta: &SnapshotDelta,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::write_delta(tx, project_id, epoch, delta)
        })
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

    async fn snapshot_delete_manifests_except(
        &self,
        project_id: i64,
        keep_epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::delete_manifests_except(tx, project_id, keep_epoch)
        })
    }

    async fn snapshot_delta_chain_info(
        &self,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError> {
        let conn = self.read_connection()?;
        RelationSnapshotRepository::get_delta_chain_info(&conn, project_id, active_epoch)
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

    async fn project_meta_get_int_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<i64>, StorageError> {
        let conn = self.read_connection()?;
        ProjectRepository::meta_get_int_optional(&conn, project_id, key)
    }

    async fn project_meta_get_string_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<String>, StorageError> {
        SqliteClient::project_meta_get_string_optional(self, project_id, key)
    }

    async fn project_meta_set_string(
        &self,
        project_id: i64,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        SqliteClient::project_meta_set_string(self, project_id, key, value)
    }

    async fn chunks_page_with_category(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError> {
        let conn = self.read_connection()?;
        ChunkRepository::get_by_project_and_epoch_with_category_paged(
            &conn, project_id, epoch, limit, offset,
        )
    }

    async fn files_epoch_entries(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FileEpochEntry>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT path, batch_id, content_hash FROM files
                 WHERE project_id = ?1 AND epoch = ?2 ORDER BY path",
            )
            .map_err(|e| StorageError::query(format!("files_epoch_entries prepare: {e}")))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, epoch], |row| {
                Ok(FileEpochEntry {
                    path: row.get(0)?,
                    batch_id: row.get(1)?,
                    content_hash: row.get(2)?,
                })
            })
            .map_err(|e| StorageError::query(format!("files_epoch_entries query: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("files_epoch_entries collect: {e}")))?;
        Ok(rows)
    }

    async fn files_set_epoch(
        &self,
        project_id: i64,
        path: &str,
        epoch: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.write_connection()?;
        let updated = conn
            .execute(
                "UPDATE files SET epoch = ?1 WHERE project_id = ?2 AND path = ?3",
                rusqlite::params![epoch, project_id, path],
            )
            .map_err(|e| StorageError::query(format!("files_set_epoch: {e}")))?;
        Ok(updated > 0)
    }

    async fn checkpoint_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let conn = self.read_connection()?;
        let result: Option<Vec<u8>> = conn
            .query_row(
                "SELECT cf.parsed_data
                 FROM checkpoint_file cf
                 JOIN checkpoint c ON cf.operation_id = c.operation_id
                   AND c.project_id = cf.project_id
                 WHERE cf.project_id = ?1
                   AND cf.file_path = ?2
                   AND cf.content_hash = ?3
                   AND cf.parsed_data IS NOT NULL
                   AND c.status = 'in_progress'
                 ORDER BY cf.id DESC
                 LIMIT 1",
                rusqlite::params![project_id, file_path, content_hash],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    async fn chunks_by_file_epoch(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT chunk_id, file_path, content,
                        start_line, end_line, entity_ids, entity_names, entity_kinds, group_title,
                        chunk_type, test_status, test_source,
                        created_at, updated_at, project_id, epoch, batch_id, path,
                        bm25_keywords, segment_id, truncated
                 FROM chunks
                 WHERE project_id = ?1 AND file_path = ?2 AND epoch = ?3
                 ORDER BY chunk_id",
            )
            .map_err(|e| StorageError::query(format!("chunks_by_file_epoch prepare: {e}")))?;
        stmt.query_map(rusqlite::params![project_id, file_path, epoch], |row| {
            Ok(ChunkRecord {
                chunk_id: row.get(0)?,
                file_path: row.get(1)?,
                content: row.get(2)?,
                start_line: row.get(3)?,
                end_line: row.get(4)?,
                entity_ids: row.get(5)?,
                entity_names: row.get(6)?,
                entity_kinds: row.get(7)?,
                group_title: row.get(8)?,
                chunk_type: row.get(9)?,
                test_status: row.get(10)?,
                test_source: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
                project_id: row.get(14)?,
                epoch: row.get(15)?,
                batch_id: row.get(16)?,
                path: row.get(17)?,
                bm25_keywords: row.get(18)?,
                segment_id: row.get(19)?,
                truncated: row.get(20)?,
            })
        })
        .map_err(|e| StorageError::query(format!("chunks_by_file_epoch query: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| StorageError::query(format!("chunks_by_file_epoch collect: {e}")))
    }

    async fn chunks_by_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT chunk_id FROM chunks
                 WHERE project_id = ?1 AND file_path = ?2 AND epoch = ?3
                 ORDER BY chunk_id",
            )
            .map_err(|e| StorageError::query(format!("chunks_by_file prepare: {e}")))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, file_path, epoch], |row| {
                row.get(0)
            })
            .map_err(|e| StorageError::query(format!("chunks_by_file query: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("chunks_by_file collect: {e}")))?;
        Ok(rows)
    }

    async fn manifest_parent_epoch(
        &self,
        project_id: i64,
        data_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        let conn = self.read_connection()?;
        ProjectIndexManifestRepository::parent_data_epoch_of(&conn, project_id, data_epoch)
    }

    async fn files_count(&self, project_id: i64, epoch: i64) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT COUNT(*) FROM files WHERE project_id = ?1 AND epoch = ?2",
            rusqlite::params![project_id, epoch],
            |row| row.get(0),
        )
        .map_err(|e| StorageError::query(format!("files_count: {e}")))
    }

    async fn files_exists_at_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<bool, StorageError> {
        let conn = self.read_connection()?;
        let exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM files WHERE project_id = ?1 AND epoch = ?2 AND path = ?3)",
                rusqlite::params![project_id, epoch, path],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::query(format!("files_exists_at_epoch: {e}")))?;
        Ok(exists != 0)
    }

    async fn files_page(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        let conn = self.read_connection()?;
        FileRepository::get_by_project_and_epoch_paged(&conn, project_id, epoch, limit, offset)
    }

    async fn files_visible_hashes(
        &self,
        project_id: i64,
    ) -> Result<Vec<FileHashEntry>, StorageError> {
        let conn = self.read_connection()?;
        let view = crate::cache::GenerationView::resolve(&conn, project_id)?;
        let mut stored: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        for epoch in view.ancestors.iter().rev() {
            Self::load_file_hashes(&conn, project_id, *epoch, &mut stored)?;
        }
        for path in &view.excluded_files {
            stored.remove(path);
        }
        Self::load_file_hashes(&conn, project_id, view.own_epoch, &mut stored)?;
        Ok(stored
            .into_iter()
            .map(|(path, content_hash)| FileHashEntry { path, content_hash })
            .collect())
    }

    async fn files_record_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, String)],
    ) -> Result<usize, StorageError> {
        if entries.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            for (path, hash) in entries {
                FileRepository::insert_hash_for_epoch(
                    tx,
                    std::path::Path::new(path),
                    hash,
                    project_id,
                    epoch,
                )?;
            }
            Ok(entries.len())
        })
    }

    async fn files_remove_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        if paths.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut removed = 0usize;
            for path in paths {
                removed += tx
                    .execute(
                        "DELETE FROM files WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
                        rusqlite::params![path, project_id, epoch],
                    )
                    .map_err(|error| StorageError::delete("files", error.to_string()))?;
            }
            Ok(removed)
        })
    }

    async fn files_ensure(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        entries: &[FileEnsureEntry],
    ) -> Result<usize, StorageError> {
        if entries.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut count = 0usize;
            for entry in entries {
                tx.execute(
                    "INSERT INTO files
                        (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?4, ?5, NULL, ?6, ?7)
                     ON CONFLICT(project_id, epoch, path) DO UPDATE SET
                        language = excluded.language,
                        category = excluded.category,
                        last_modified = excluded.last_modified,
                        batch_id = excluded.batch_id",
                    rusqlite::params![
                        entry.path,
                        entry.language,
                        entry.category,
                        entry.last_modified,
                        project_id,
                        epoch,
                        batch_id,
                    ],
                )
                .map_err(|error| {
                    StorageError::insert("files", format!("failed to ensure file record: {error}"))
                })?;
                count += 1;
            }
            Ok(count)
        })
    }

    async fn files_delete_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM files WHERE path = ?1 AND project_id = ?2",
                rusqlite::params![path, project_id],
            )
            .map_err(|error| StorageError::delete("files", error.to_string()))
        })
    }

    async fn files_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.read_connection()?;
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files WHERE project_id = ? AND id IN ({placeholders})"
            ))
            .map_err(|e| StorageError::query(format!("files_by_ids prepare: {e}")))?;
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
        params.push(&project_id);
        for id in ids {
            params.push(id);
        }
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    language: row.get(2)?,
                    category: row.get(3)?,
                    last_modified: row.get(4)?,
                    created_at: row.get(5)?,
                    project_id: row.get(6)?,
                    content_hash: row.get(7)?,
                })
            })
            .map_err(|e| StorageError::query(format!("files_by_ids query: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("files_by_ids collect: {e}")))
    }

    async fn file_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<Option<FileRecord>, StorageError> {
        let conn = self.read_connection()?;
        FileRepository::get_by_path_and_project(&conn, path, project_id)
    }

    async fn entities_insert_returning_ids(
        &self,
        entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError> {
        if entities.is_empty() {
            return Ok(Vec::new());
        }
        self.with_transaction(|tx| EntityRepository::insert_batch(tx, entities))
    }

    async fn entities_with_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError> {
        let conn = self.read_connection()?;
        let rows =
            EntityRepository::get_by_project_and_epoch_with_file_path(&conn, project_id, epoch)?;
        Ok(rows
            .into_iter()
            .map(|(entity, file_path)| EntityWithFile { entity, file_path })
            .collect())
    }

    async fn entities_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.read_connection()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entity) = EntityRepository::get_by_id(&conn, *id)?
                && entity.project_id == project_id
            {
                out.push(entity);
            }
        }
        Ok(out)
    }

    async fn entities_by_file(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        let conn = self.read_connection()?;
        EntityRepository::get_by_file_and_project(&conn, file_id, project_id)
    }

    async fn entities_delete_by_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<bool, StorageError> {
        let conn = self.write_connection()?;
        let removed = conn
            .execute(
                "DELETE FROM entities WHERE id = ?1 AND project_id = ?2",
                rusqlite::params![entity_id, project_id],
            )
            .map_err(|e| StorageError::delete("entities", e.to_string()))?;
        Ok(removed > 0)
    }

    async fn entities_delete_by_file(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            EntityRepository::delete_by_file_id_at_epoch(tx, file_id, epoch)?;
            Ok(tx.changes().try_into().unwrap_or(0usize))
        })
    }

    async fn entities_set_parent(
        &self,
        project_id: i64,
        epoch: i64,
        child_id: i64,
        parent_id: i64,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "UPDATE entities SET parent_id = ?1
                 WHERE id = ?2 AND project_id = ?3 AND epoch = ?4",
                rusqlite::params![parent_id, child_id, project_id, epoch],
            )
            .map_err(|error| StorageError::update("entities", error.to_string()))?;
            Ok(())
        })
    }

    async fn chunks_content_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<ChunkContent>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT chunk_id, content, bm25_keywords FROM chunks
                 WHERE project_id = ?1 AND epoch = ?2",
            )
            .map_err(|error| StorageError::query(error.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, epoch], |row| {
                Ok(ChunkContent {
                    chunk_id: row.get(0)?,
                    content: row.get(1)?,
                    bm25_keywords: row.get(2)?,
                })
            })
            .map_err(|error| StorageError::query(error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::query(error.to_string()))
    }

    async fn chunks_stats(
        &self,
        project_id: i64,
        epoch: i64,
        channel: &str,
    ) -> Result<(i64, i64), StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(truncated), 0) FROM chunks
             WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
            rusqlite::params![project_id, epoch, channel],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| StorageError::query(error.to_string()))
    }

    async fn chunks_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<u64>, StorageError> {
        let conn = self.read_connection()?;
        ChunkRepository::max_entity_id_for_epoch(&conn, project_id, epoch)
    }

    async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| ChunkRepository::delete_by_file_path(tx, path, project_id))
    }

    async fn chunks_and_mappings_upsert(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> Result<(usize, usize), StorageError> {
        if chunks.is_empty() && mappings.is_empty() {
            return Ok((0, 0));
        }
        self.with_transaction(|tx| {
            let mut chunk_count = 0usize;
            if !chunks.is_empty() {
                ChunkRepository::insert_batch(tx, chunks)?;
                chunk_count = chunks.len();
            }
            let mut mapping_count = 0usize;
            for mapping in mappings {
                EntityDetailMappingRepository::upsert(tx, mapping)?;
                mapping_count += 1;
            }
            Ok((chunk_count, mapping_count))
        })
    }

    async fn mappings_by_entity(
        &self,
        project_id: i64,
        entity_id: i64,
        epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        let conn = self.read_connection()?;
        EntityDetailMappingRepository::get_by_entity_id_at_epoch(
            &conn, entity_id, project_id, epoch,
        )
    }

    async fn manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        let conn = self.read_connection()?;
        ProjectIndexManifestRepository::get_building_by_operation(&conn, project_id, operation_id)
    }

    async fn manifest_building_max_epoch(
        &self,
        project_id: i64,
    ) -> Result<Option<i64>, StorageError> {
        let conn = self.read_connection()?;
        ProjectIndexManifestRepository::get_building_max_epoch(&conn, project_id)
    }

    async fn manifest_set_parent(
        &self,
        project_id: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::set_parent_data_epoch(
                tx,
                project_id,
                operation_id,
                parent_data_epoch,
            )
        })
    }

    async fn manifest_has_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
    ) -> Result<bool, StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM checkpoint
                 WHERE project_id = ?1 AND operation_type = ?2
                   AND status = 'in_progress'
             )",
            rusqlite::params![project_id, operation_type],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| StorageError::query(error.to_string()))
    }

    async fn overrides_upsert_batch(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, OverrideDisposition)],
    ) -> Result<(), StorageError> {
        if entries.is_empty() {
            return Ok(());
        }
        self.with_transaction(|tx| {
            for (path, disposition) in entries {
                GenerationOverrideRepository::upsert(tx, project_id, epoch, path, *disposition)?;
            }
            Ok(())
        })
    }

    async fn overrides_clear(&self, project_id: i64, epoch: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            GenerationOverrideRepository::clear_generation(tx, project_id, epoch)?;
            Ok(tx.changes().try_into().unwrap_or(0usize))
        })
    }

    async fn generation_copy_file(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            let target_exists: Option<i64> = tx
                .query_row(
                    "SELECT id FROM files
                     WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
                    rusqlite::params![project_id, target_epoch, path],
                    |row| row.get(0),
                )
                .ok();
            if target_exists.is_some() {
                return Ok(0);
            }
            let source_file = tx
                .query_row(
                    "SELECT language, category, last_modified, created_at, content_hash, batch_id
                     FROM files
                     WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
                    rusqlite::params![project_id, source_epoch, path],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u8>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .ok();
            let Some((language, category, last_modified, created_at, content_hash, batch_id)) =
                source_file
            else {
                return Ok(0);
            };
            tx.execute(
                "INSERT INTO files
                    (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    path,
                    language,
                    category,
                    last_modified,
                    created_at,
                    project_id,
                    content_hash,
                    target_epoch,
                    batch_id,
                ],
            )
            .map_err(|error| StorageError::insert("files", error.to_string()))?;
            let new_file_id = tx.last_insert_rowid();
            let entities = Self::load_entity_copies(tx, project_id, source_epoch, path)?;
            let entity_ids = Self::insert_entity_copies(tx, project_id, target_epoch, entities, |_| {
                Some(new_file_id)
            })?;
            Ok(entity_ids.len())
        })
    }

    async fn generation_materialize(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            Self::materialize_generation_in(
                tx,
                project_id,
                source_epoch,
                target_epoch,
                excluded_paths,
            )
        })
    }

    async fn generation_gc_plan(
        &self,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<GenerationGcPlan, StorageError> {
        let conn = self.read_connection()?;
        ProjectIndexManifestRepository::generation_gc_plan(
            &conn,
            project_id,
            keep_active_generations,
            stale_before,
        )
    }

    async fn generation_gc_apply(
        &self,
        project_id: i64,
        plan: &GenerationGcPlan,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::apply_generation_gc(tx, project_id, plan)
        })
    }

    async fn candidate_begin(
        &self,
        project_id: i64,
        candidate_epoch: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            ProjectIndexManifestRepository::begin_building(
                tx,
                project_id,
                candidate_epoch,
                operation_id,
                None,
            )
            .map(|_| ())?;
            ProjectIndexManifestRepository::set_parent_data_epoch(
                tx,
                project_id,
                operation_id,
                parent_data_epoch,
            )?;
            Self::clear_candidate_rows_in(tx, project_id, candidate_epoch)?;
            GenerationOverrideRepository::clear_generation(tx, project_id, candidate_epoch)?;
            ProjectIndexManifestRepository::mark_candidate_ready(tx, project_id, operation_id)
        })
    }

    async fn candidate_clear_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            let mut removed = Self::clear_candidate_rows_in(tx, project_id, epoch)?;
            GenerationOverrideRepository::clear_generation(tx, project_id, epoch)?;
            removed += usize::try_from(tx.changes()).unwrap_or(0);
            Ok(removed)
        })
    }

    async fn candidate_delete_file_rows(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        if paths.is_empty() {
            return Ok(0);
        }
        self.with_transaction(|tx| {
            let mut removed = 0usize;
            for path in paths {
                removed += tx
                    .execute(
                        "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3",
                        rusqlite::params![project_id, epoch, path],
                    )
                    .map_err(|error| StorageError::delete("chunks", error.to_string()))?;
                removed += tx
                    .execute(
                        "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                         (SELECT id FROM files WHERE path = ?3 AND project_id = ?1 AND epoch = ?2)",
                        rusqlite::params![project_id, epoch, path],
                    )
                    .map_err(|error| StorageError::delete("entities", error.to_string()))?;
            }
            Ok(removed)
        })
    }

    async fn candidate_prepare_channel(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
        channel: &str,
    ) -> Result<(), StorageError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.with_transaction(|tx| {
            for path in paths {
                tx.execute(
                    "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2 AND file_path = ?3 AND path = ?4",
                    rusqlite::params![project_id, epoch, path, channel],
                )
                .map_err(|error| StorageError::delete("chunks", error.to_string()))?;
                GenerationOverrideRepository::upsert(
                    tx,
                    project_id,
                    epoch,
                    path,
                    OverrideDisposition::Replaced,
                )?;
            }
            Ok(())
        })
    }

    async fn projection_load(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<IndexStateProjection>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT project_id, operation_id, file_path, version, state_json, updated_at
                 FROM index_state_projection
                 WHERE project_id = ?1 AND operation_id = ?2",
            )
            .map_err(|error| StorageError::query(error.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, operation_id], |row| {
                Ok(IndexStateProjection {
                    project_id: row.get(0)?,
                    operation_id: row.get(1)?,
                    file_path: row.get(2)?,
                    version: row.get(3)?,
                    state_json: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|error| StorageError::query(error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::query(error.to_string()))
    }

    async fn projection_upsert(&self, row: &IndexStateProjection) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "INSERT INTO index_state_projection
                    (project_id, operation_id, file_path, version, state_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(project_id, operation_id, file_path) DO UPDATE SET
                    version = excluded.version, state_json = excluded.state_json,
                    updated_at = excluded.updated_at
                 WHERE excluded.version >= index_state_projection.version",
                rusqlite::params![
                    row.project_id,
                    row.operation_id,
                    row.file_path,
                    row.version,
                    row.state_json,
                    row.updated_at,
                ],
            )
            .map_err(|error| StorageError::insert("index_state_projection", error.to_string()))?;
            Ok(())
        })
    }

    async fn projection_delete(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM index_state_projection
                 WHERE project_id = ?1 AND operation_id = ?2 AND file_path = ?3",
                rusqlite::params![project_id, operation_id, file_path],
            )
            .map_err(|error| StorageError::delete("index_state_projection", error.to_string()))
        })
    }

    async fn projection_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM index_state_projection
                 WHERE project_id = ?1 AND operation_id = ?2",
                rusqlite::params![project_id, operation_id],
            )
            .map_err(|error| StorageError::delete("index_state_projection", error.to_string()))
        })
    }

    async fn projection_delete_by_project(&self, project_id: i64) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM index_state_projection WHERE project_id = ?1",
                rusqlite::params![project_id],
            )
            .map_err(|error| StorageError::delete("index_state_projection", error.to_string()))
        })
    }

    async fn checkpoint_active(
        &self,
        project_id: i64,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_active_checkpoint(&conn, project_id)
    }

    async fn checkpoint_set_active(
        &self,
        project_id: i64,
        operation_id: &str,
        priority: i32,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::set_active_flag(tx, project_id, operation_id, priority)
        })
    }

    async fn checkpoint_clear_active(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::clear_active_flag(tx, project_id, operation_id)?;
            Ok(usize::try_from(tx.changes()).unwrap_or(0))
        })
    }

    async fn checkpoint_heartbeat(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::update_heartbeat(&conn, project_id, operation_id)
    }

    async fn checkpoints_cleanup_stale(
        &self,
        project_id: i64,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::cleanup_stale_active(&conn, project_id, stale_threshold_secs)
    }

    async fn checkpoints_delete_expired(
        &self,
        project_id: i64,
        ttl_seconds: u64,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::delete_expired_checkpoints(&conn, project_id, ttl_seconds)
    }

    async fn checkpoints_unfinished(
        &self,
        project_id: i64,
    ) -> Result<Vec<CheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_unfinished_operations(&conn, project_id)
    }

    async fn checkpoint_latest_incomplete(
        &self,
        project_id: i64,
        operation_type: &str,
        root_dir: &str,
    ) -> Result<Option<CheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_latest_incomplete_by_type(
            &conn,
            project_id,
            operation_type,
            root_dir,
        )
    }

    async fn checkpoint_set_batch_index(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::update_current_batch_index(
                tx,
                project_id,
                operation_id,
                batch_index,
            )
        })
    }

    async fn checkpoint_batch_create(
        &self,
        project_id: i64,
        batch: &BatchCheckpointRecord,
    ) -> Result<(), StorageError> {
        self.with_transaction(|tx| {
            CheckpointRepository::insert_batch_checkpoint(tx, project_id, batch)?;
            Ok(())
        })
    }

    async fn checkpoint_batch_get(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Option<BatchCheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_batch_checkpoint(&conn, project_id, operation_id, batch_index)
    }

    async fn checkpoint_batch_files(
        &self,
        project_id: i64,
        operation_id: &str,
        batch_index: u32,
    ) -> Result<Vec<FileCheckpointRecord>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_batch_files(&conn, project_id, operation_id, batch_index)
    }

    async fn checkpoint_failed_files(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.read_connection()?;
        CheckpointRepository::get_failed_files(&conn, project_id, operation_id)
    }

    async fn checkpoint_batches_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::delete_checkpoint_batches_by_operation_id(
            &conn,
            project_id,
            operation_id,
        )
    }

    async fn checkpoint_file_set_parsed_data(
        &self,
        project_id: i64,
        operation_id: &str,
        file_path: &str,
        data: &[u8],
    ) -> Result<bool, StorageError> {
        let conn = self.write_connection()?;
        let updated = conn
            .execute(
                "UPDATE checkpoint_file SET parsed_data = ?1
                 WHERE project_id = ?2 AND operation_id = ?3 AND file_path = ?4",
                rusqlite::params![data, project_id, operation_id, file_path],
            )
            .map_err(|e| StorageError::query(format!("checkpoint_file_set_parsed_data: {e}")))?;
        Ok(updated > 0)
    }

    async fn mappings_point_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<(i64, String)>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(
                "SELECT m.entity_id, m.qdrant_point_ids FROM entity_detail_mappings m
                 JOIN entities e ON e.id = m.entity_id
                 JOIN files f ON f.id = e.file_id
                 WHERE m.project_id = ?1 AND m.epoch = ?2 AND f.path = ?3
                 ORDER BY m.entity_id",
            )
            .map_err(|e| StorageError::query(format!("mappings_point_ids prepare: {e}")))?;
        stmt.query_map(rusqlite::params![project_id, epoch, path], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| StorageError::query(format!("mappings_point_ids query: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| StorageError::query(format!("mappings_point_ids collect: {e}")))
    }

    async fn mappings_delete_by_file_id(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        conn.execute(
            "DELETE FROM entity_detail_mappings
             WHERE project_id = ?1 AND entity_id IN (SELECT id FROM entities WHERE file_id = ?2)",
            rusqlite::params![project_id, file_id],
        )
        .map_err(|e| StorageError::delete("entity_detail_mappings", e.to_string()))
    }

    async fn mappings_delete_by_entity_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            EntityDetailMappingRepository::delete_by_entity_id(tx, entity_id, project_id)
        })
    }

    async fn file_checkpoints_clear_module_progress(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::clear_module_progress(&conn, project_id, operation_id)
    }

    async fn work_units_insert_batch(
        &self,
        records: &[WorkUnitCheckpointRecord],
    ) -> Result<Vec<i64>, StorageError> {
        if records.is_empty() {
            return Ok(Vec::new());
        }
        self.with_transaction(|tx| {
            let mut ids = Vec::with_capacity(records.len());
            for record in records {
                ids.push(CheckpointRepository::insert_work_unit(tx, record)?);
            }
            Ok(ids)
        })
    }

    async fn work_units_delete_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        CheckpointRepository::delete_work_units_by_operation_id(&conn, project_id, operation_id)
    }

    async fn snapshot_manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let conn = self.read_connection()?;
        RelationSnapshotRepository::get_manifest_by_operation(&conn, project_id, operation_id)
    }

    async fn snapshot_activate(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.with_transaction(|tx| RelationSnapshotRepository::activate(tx, project_id, epoch))
    }

    async fn snapshot_retry_failed(&self, project_id: i64, epoch: i64) -> Result<(), StorageError> {
        self.with_transaction(|tx| RelationSnapshotRepository::retry_failed(tx, project_id, epoch))
    }

    async fn snapshot_collect_garbage(
        &self,
        project_id: i64,
        stale_before: i64,
    ) -> Result<usize, StorageError> {
        self.with_transaction(|tx| {
            RelationSnapshotRepository::collect_garbage(tx, project_id, stale_before)
        })
    }

    async fn snapshot_entities_count(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<i64, StorageError> {
        let conn = self.read_connection()?;
        conn.query_row(
            "SELECT COUNT(*) FROM relation_snapshot_entities
             WHERE project_id = ?1 AND relation_epoch = ?2",
            rusqlite::params![project_id, relation_epoch],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::query(error.to_string()))
    }
}

/// One entity row staged for cross-generation copying.
struct EntityCopyRow {
    old_id: i64,
    old_file_id: i64,
    name: String,
    kind: String,
    signature: Option<String>,
    span_start_row: Option<i64>,
    span_end_row: Option<i64>,
    span_start_column: Option<i64>,
    span_end_column: Option<i64>,
    span_start_byte: Option<i64>,
    span_end_byte: Option<i64>,
    scoped_name: Option<String>,
    depth: Option<i64>,
    old_parent_id: Option<i64>,
    metadata: Option<String>,
    parameters_json: Option<String>,
    return_type: Option<String>,
    doc_comment: Option<String>,
    modifiers_json: Option<String>,
    batch_id: i64,
}

impl EntityCopyRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            old_id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            old_file_id: row.get(3)?,
            signature: row.get(4)?,
            span_start_row: row.get(5)?,
            span_end_row: row.get(6)?,
            span_start_column: row.get(7)?,
            span_end_column: row.get(8)?,
            span_start_byte: row.get(9)?,
            span_end_byte: row.get(10)?,
            scoped_name: row.get(11)?,
            depth: row.get(12)?,
            old_parent_id: row.get(13)?,
            metadata: row.get(14)?,
            parameters_json: row.get(15)?,
            return_type: row.get(16)?,
            doc_comment: row.get(17)?,
            modifiers_json: row.get(18)?,
            batch_id: row.get(19)?,
        })
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

    /// Load `(path, content_hash)` rows of one epoch into the view map.
    ///
    /// Rows without a stored hash record a `None` entry: the path stays
    /// visible while carrying no hash for comparison.
    fn load_file_hashes(
        conn: &rusqlite::Connection,
        project_id: i64,
        epoch: i64,
        stored: &mut std::collections::HashMap<String, Option<String>>,
    ) -> Result<(), StorageError> {
        let mut offset = 0i64;
        loop {
            let page = FileRepository::get_by_project_and_epoch_paged(
                conn, project_id, epoch, 1000, offset,
            )?;
            if page.is_empty() {
                break;
            }
            offset += page.len() as i64;
            for file in page {
                stored.insert(file.path, file.content_hash);
            }
        }
        Ok(())
    }

    /// Drop every candidate-generation row from the data tables.
    fn clear_candidate_rows_in(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        let mut removed = 0usize;
        for (table, sql) in [
            (
                "entity_detail_mappings",
                "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "chunks",
                "DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "entities",
                "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2",
            ),
            (
                "files",
                "DELETE FROM files WHERE project_id = ?1 AND epoch = ?2",
            ),
        ] {
            removed += tx
                .execute(sql, rusqlite::params![project_id, epoch])
                .map_err(|error| {
                    StorageError::delete(
                        table,
                        format!("failed to clear candidate generation: {error}"),
                    )
                })?;
        }
        Ok(removed)
    }

    /// One staged entity row for cross-generation copying.
    #[allow(clippy::too_many_arguments)]
    fn load_entity_copies(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        source_epoch: i64,
        path: &str,
    ) -> Result<Vec<EntityCopyRow>, StorageError> {
        let mut statement = tx
            .prepare(
                "SELECT e.id, e.name, e.kind, e.file_id, e.signature,
                        e.span_start_row, e.span_end_row, e.span_start_column,
                        e.span_end_column, e.span_start_byte, e.span_end_byte,
                        e.scoped_name, e.depth, e.parent_id, e.metadata,
                        e.parameters_json, e.return_type, e.doc_comment,
                        e.modifiers_json, e.batch_id
                 FROM entities e
                 JOIN files f ON f.id = e.file_id
                 WHERE e.project_id = ?1 AND e.epoch = ?2 AND f.path = ?3
                    AND f.epoch = ?2 AND f.project_id = e.project_id",
            )
            .map_err(|error| StorageError::query(error.to_string()))?;
        let rows = statement
            .query_map(rusqlite::params![project_id, source_epoch, path], |row| {
                EntityCopyRow::from_row(row)
            })
            .map_err(|error| StorageError::query(error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::query(error.to_string()))
    }

    /// Copy staged entity rows into `target_epoch`, remapping parents.
    fn insert_entity_copies(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        target_epoch: i64,
        entities: Vec<EntityCopyRow>,
        target_file_id: impl Fn(i64) -> Option<i64>,
    ) -> Result<std::collections::HashMap<i64, i64>, StorageError> {
        let mut entity_ids = std::collections::HashMap::new();
        let mut parent_updates = Vec::new();
        for entity in entities {
            let Some(new_file_id) = target_file_id(entity.old_file_id) else {
                continue;
            };
            tx.execute(
                "INSERT INTO entities
                    (name, kind, file_id, signature, span_start_row, span_end_row,
                     span_start_column, span_end_column, span_start_byte, span_end_byte,
                     scoped_name, depth, parent_id, metadata, parameters_json, return_type,
                     doc_comment, modifiers_json, project_id, epoch, batch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL,
                         ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                rusqlite::params![
                    entity.name,
                    entity.kind,
                    new_file_id,
                    entity.signature,
                    entity.span_start_row,
                    entity.span_end_row,
                    entity.span_start_column,
                    entity.span_end_column,
                    entity.span_start_byte,
                    entity.span_end_byte,
                    entity.scoped_name,
                    entity.depth,
                    entity.metadata,
                    entity.parameters_json,
                    entity.return_type,
                    entity.doc_comment,
                    entity.modifiers_json,
                    project_id,
                    target_epoch,
                    entity.batch_id,
                ],
            )
            .map_err(|error| StorageError::insert("entities", error.to_string()))?;
            let new_id = tx.last_insert_rowid();
            entity_ids.insert(entity.old_id, new_id);
            parent_updates.push((new_id, entity.old_parent_id));
        }
        for (new_id, old_parent_id) in parent_updates {
            if let Some(old_parent_id) = old_parent_id
                && let Some(parent_new_id) = entity_ids.get(&old_parent_id)
            {
                tx.execute(
                    "UPDATE entities SET parent_id = ?1 WHERE id = ?2",
                    rusqlite::params![parent_new_id, new_id],
                )
                .map_err(|error| StorageError::update("entities", error.to_string()))?;
            }
        }
        Ok(entity_ids)
    }

    /// Materialize an inherited generation into a full one (compaction).
    ///
    /// Copies every non-overridden source row into the target, merging with
    /// rows the target already owns. Crash-retry safe: target rows of the
    /// copied source paths are dropped first inside the same transaction.
    fn materialize_generation_in(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), StorageError> {
        let exclusion_sql = if excluded_paths.is_empty() {
            String::new()
        } else {
            let placeholders = excluded_paths
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(" AND path NOT IN ({placeholders})")
        };
        let source_paths: Vec<String> = {
            let sql = format!(
                "SELECT path FROM files
                 WHERE project_id = ?1 AND epoch = ?2{exclusion_sql}"
            );
            let mut statement = tx
                .prepare(&sql)
                .map_err(|error| StorageError::query(error.to_string()))?;
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &source_epoch];
            for path in excluded_paths {
                params.push(path);
            }
            let rows = statement
                .query_map(params.as_slice(), |row| row.get::<_, String>(0))
                .map_err(|error| StorageError::query(error.to_string()))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::query(error.to_string()))?
        };
        Self::clear_target_rows_for_paths_in(tx, project_id, target_epoch, &source_paths)?;
        let files_sql = format!(
            "SELECT id, path, language, category, last_modified, created_at, content_hash, batch_id
             FROM files WHERE project_id = ?1 AND epoch = ?2{exclusion_sql}"
        );
        let files = {
            let mut statement = tx
                .prepare(&files_sql)
                .map_err(|error| StorageError::query(error.to_string()))?;
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &source_epoch];
            for path in excluded_paths {
                params.push(path);
            }
            let rows = statement
                .query_map(params.as_slice(), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u8>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                })
                .map_err(|error| StorageError::query(error.to_string()))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::query(error.to_string()))?
        };
        let mut file_ids = std::collections::HashMap::new();
        for (old_id, path, language, category, last_modified, created_at, hash, batch_id) in files {
            tx.execute(
                "INSERT INTO files
                    (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    path,
                    language,
                    category,
                    last_modified,
                    created_at,
                    project_id,
                    hash,
                    target_epoch,
                    batch_id,
                ],
            )
            .map_err(|error| StorageError::insert("files", error.to_string()))?;
            file_ids.insert(old_id, tx.last_insert_rowid());
        }
        let entities = {
            let mut statement = tx
                .prepare(
                    "SELECT e.id, e.name, e.kind, e.file_id, e.signature,
                            e.span_start_row, e.span_end_row, e.span_start_column,
                            e.span_end_column, e.span_start_byte, e.span_end_byte,
                            e.scoped_name, e.depth, e.parent_id, e.metadata,
                            e.parameters_json, e.return_type, e.doc_comment,
                            e.modifiers_json, e.batch_id
                     FROM entities e
                     WHERE e.project_id = ?1 AND e.epoch = ?2",
                )
                .map_err(|error| StorageError::query(error.to_string()))?;
            let rows = statement
                .query_map(rusqlite::params![project_id, source_epoch], |row| {
                    EntityCopyRow::from_row(row)
                })
                .map_err(|error| StorageError::query(error.to_string()))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::query(error.to_string()))?
        };
        let entity_ids =
            Self::insert_entity_copies(tx, project_id, target_epoch, entities, |old_file_id| {
                file_ids.get(&old_file_id).copied()
            })?;
        let mappings = {
            let mut statement = tx
                .prepare(
                    "SELECT entity_id, qdrant_point_ids, bm25_doc_ids, chunk_count,
                            created_at, updated_at
                     FROM entity_detail_mappings
                     WHERE project_id = ?1 AND epoch = ?2",
                )
                .map_err(|error| StorageError::query(error.to_string()))?;
            let rows = statement
                .query_map(rusqlite::params![project_id, source_epoch], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })
                .map_err(|error| StorageError::query(error.to_string()))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::query(error.to_string()))?
        };
        for (old_entity_id, qdrant_point_ids, bm25_doc_ids, chunk_count, created_at, updated_at) in
            mappings
        {
            let Some(new_entity_id) = entity_ids.get(&old_entity_id) else {
                continue;
            };
            tx.execute(
                "INSERT INTO entity_detail_mappings
                    (entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids,
                     chunk_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    new_entity_id,
                    project_id,
                    target_epoch,
                    Self::replace_epoch_in_id_list(&qdrant_point_ids, source_epoch, target_epoch),
                    Self::replace_epoch_in_id_list(&bm25_doc_ids, source_epoch, target_epoch),
                    chunk_count,
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| StorageError::insert("entity_detail_mappings", error.to_string()))?;
        }
        let chunks_exclusion_sql = if excluded_paths.is_empty() {
            " AND epoch = ?3".to_string()
        } else {
            let placeholders = excluded_paths
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(" AND epoch = ?3 AND file_path NOT IN ({placeholders})")
        };
        let chunks_sql = format!(
            "INSERT INTO chunks
                (chunk_id, file_path, content, start_line, end_line,
                 entity_ids, entity_names, chunk_type, created_at, updated_at,
                 project_id, epoch, batch_id, truncated)
             SELECT chunk_id, file_path, content, start_line, end_line,
                    entity_ids, entity_names, chunk_type, created_at, updated_at,
                    project_id, ?2, batch_id, truncated
             FROM chunks WHERE project_id = ?1{chunks_exclusion_sql}"
        );
        let mut statement = tx
            .prepare(&chunks_sql)
            .map_err(|error| StorageError::insert("chunks", error.to_string()))?;
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &target_epoch, &source_epoch];
        for path in excluded_paths {
            params.push(path);
        }
        statement
            .execute(params.as_slice())
            .map_err(|error| StorageError::insert("chunks", error.to_string()))?;
        Ok(())
    }

    /// Delete the target-generation rows of an explicit path set.
    fn clear_target_rows_for_paths_in(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        target_epoch: i64,
        paths: &[String],
    ) -> Result<(), StorageError> {
        if paths.is_empty() {
            return Ok(());
        }
        let placeholders = paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let scope = format!(" AND path IN ({placeholders})");
        let chunk_scope = format!(" AND file_path IN ({placeholders})");
        for (table, sql) in [
            (
                "entity_detail_mappings",
                format!(
                    "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2 AND entity_id IN
                     (SELECT id FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                     (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2{scope}))"
                ),
            ),
            (
                "chunks",
                format!("DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2{chunk_scope}"),
            ),
            (
                "entities",
                format!(
                    "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                     (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2{scope})"
                ),
            ),
            (
                "files",
                format!("DELETE FROM files WHERE project_id = ?1 AND epoch = ?2{scope}"),
            ),
        ] {
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &target_epoch];
            for path in paths {
                params.push(path);
            }
            tx.execute(&sql, params.as_slice()).map_err(|error| {
                StorageError::delete(
                    table,
                    format!("failed to clear materialization residue: {error}"),
                )
            })?;
        }
        Ok(())
    }

    /// Replace the epoch field in a JSON array of `{prefix}::{epoch}::{suffix}` ids.
    fn replace_epoch_in_id_list(list: &str, old_epoch: i64, new_epoch: i64) -> String {
        cce_storage_common::ids::replace_epoch_in_id_list(list, old_epoch, new_epoch)
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
        // the advanced switch for its wired branches.
        let mut preset = DatabaseConfig {
            vector_backend: cce_config::modules::VectorBackend::Qdrant,
            relation_backend: cce_config::modules::RelationBackend::Remote,
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            relation_remote: cce_config::modules::RelationRemoteConfig {
                url: Some("postgres://localhost:5432/cce".to_string()),
                ..Default::default()
            },
            fulltext_remote: cce_config::modules::FulltextRemoteConfig {
                url: Some("http://localhost:9200".to_string()),
                index_name: Some("code_index".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(preset.validate_backend_combination().is_err());
        preset.allow_nonstandard_backends = true;
        // The remote relation write path is not wired, so a combination
        // containing it stays rejected even with the advanced switch; only
        // wired branches (remote fulltext below) are flag-allowed.
        assert!(preset.validate_backend_combination().is_err());
        // Remote fulltext is a wired official preset, so the partial mix
        // validates without the advanced switch.
        let partial = DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            fulltext_remote: cce_config::modules::FulltextRemoteConfig {
                url: Some("http://localhost:9200".to_string()),
                index_name: Some("code_index".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
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
            entity_kinds: "[]".to_string(),
            group_title: String::new(),
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

    /// The prepared-statement batch upserts must behave exactly like the
    /// row-by-row form they replaced: empty input writes nothing, a batch
    /// writes every row once, and a conflicting replay updates the row in
    /// place instead of duplicating it.
    #[tokio::test]
    async fn batch_upserts_handle_empty_input_and_conflicts() {
        use crate::repo::{EntityRepository, FileRepository};

        let client = SqliteClient::in_memory().expect("in-memory client");
        RelationStorage::ensure_project(&client, 1, "/repo")
            .await
            .expect("ensure project");

        // Empty input: zero count, no rows, no error.
        assert_eq!(
            RelationStorage::files_upsert(&client, 1, 3, &[])
                .await
                .expect("empty files batch"),
            0
        );
        assert_eq!(
            RelationStorage::entities_upsert(&client, &[])
                .await
                .expect("empty entities batch"),
            0
        );
        assert_eq!(
            RelationStorage::chunks_upsert(&client, &[])
                .await
                .expect("empty chunks batch"),
            0
        );

        let file_v1 = FileRecord {
            id: 0,
            path: "a.rs".to_string(),
            language: "rust".to_string(),
            category: 4,
            last_modified: 1,
            created_at: 1,
            project_id: 1,
            content_hash: Some("hash-1".to_string()),
        };
        assert_eq!(
            RelationStorage::files_upsert(&client, 1, 3, std::slice::from_ref(&file_v1))
                .await
                .expect("insert file"),
            1
        );
        // Conflict replay with changed payload: one row, updated content.
        let file_v2 = FileRecord {
            language: "rust-2024".to_string(),
            content_hash: Some("hash-2".to_string()),
            ..file_v1.clone()
        };
        assert_eq!(
            RelationStorage::files_upsert(&client, 1, 3, &[file_v2])
                .await
                .expect("update file"),
            1
        );

        // Read connections are scoped per read: the in-memory client cannot
        // serve a write while a read connection is held.
        let stored_file = {
            let conn = client.read_connection().expect("read connection");
            let stored = FileRepository::get_by_path_and_project_at_epoch(&conn, "a.rs", 1, 3)
                .expect("file read")
                .expect("file row present");
            assert_eq!(stored.language, "rust-2024");
            assert_eq!(stored.content_hash.as_deref(), Some("hash-2"));
            stored
        };

        let entity_v1 = EntityRecord {
            id: 0,
            name: "fn_a".to_string(),
            kind: "function".to_string(),
            file_id: stored_file.id,
            signature: Some("fn a() {}".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(1),
            span_start_column: Some(0),
            span_end_column: Some(0),
            span_start_byte: Some(0),
            span_end_byte: Some(1),
            scoped_name: Some("fn_a".to_string()),
            overload_discriminator: String::new(),
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 3,
            batch_id: 0,
            rank: 0.0,
        };
        assert_eq!(
            RelationStorage::entities_upsert(&client, std::slice::from_ref(&entity_v1))
                .await
                .expect("insert entity"),
            1
        );
        // Same natural key (project, epoch, file, scoped_name, kind,
        // discriminator) with a changed payload updates in place.
        let entity_v2 = EntityRecord {
            signature: Some("fn a(x: u32) {}".to_string()),
            ..entity_v1.clone()
        };
        assert_eq!(
            RelationStorage::entities_upsert(&client, &[entity_v2])
                .await
                .expect("update entity"),
            1
        );
        assert_eq!(
            RelationStorage::entities_count(&client, 1, 3)
                .await
                .expect("entity count"),
            1,
            "a conflicting replay must not duplicate the row"
        );
        let stored_entities = {
            let conn = client.read_connection().expect("read connection");
            EntityRepository::get_by_project_and_epoch_with_file_path(&conn, 1, 3)
                .expect("entity read")
        };
        assert_eq!(stored_entities.len(), 1);
        assert_eq!(stored_entities[0].1, "a.rs");
        assert_eq!(
            stored_entities[0].0.signature.as_deref(),
            Some("fn a(x: u32) {}")
        );

        let chunk_v1 = ChunkRecord {
            chunk_id: "c1".to_string(),
            file_path: "a.rs".to_string(),
            content: "fn a() {}".to_string(),
            start_line: 1,
            end_line: 1,
            entity_ids: "[]".to_string(),
            entity_names: "[]".to_string(),
            entity_kinds: "[]".to_string(),
            group_title: String::new(),
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
            RelationStorage::chunks_upsert(&client, std::slice::from_ref(&chunk_v1))
                .await
                .expect("insert chunk"),
            1
        );
        let chunk_v2 = ChunkRecord {
            content: "fn a(x: u32) {}".to_string(),
            ..chunk_v1.clone()
        };
        assert_eq!(
            RelationStorage::chunks_upsert(&client, &[chunk_v2])
                .await
                .expect("update chunk"),
            1
        );
        assert_eq!(
            RelationStorage::chunks_count(&client, 1, 3)
                .await
                .expect("chunk count"),
            1,
            "a conflicting replay must not duplicate the row"
        );
        let readback = RelationStorage::chunks_by_ids(&client, 1, &["c1".to_string()], &[3])
            .await
            .expect("readback");
        assert_eq!(readback.len(), 1);
        assert_eq!(readback[0].content, "fn a(x: u32) {}");
    }
}
