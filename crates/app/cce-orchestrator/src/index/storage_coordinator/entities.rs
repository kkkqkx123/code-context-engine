//! SQLite file and entity persistence.

use cce_scanner::FileEntry;
use cce_storage_sqlite::{EntityRecord, EntityRepository};

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    pub fn publish_file_hashes(&self, files: &[FileEntry]) -> Result<(), OrchestratorError> {
        let Some(metadata_store) = &self.metadata_store else {
            return Ok(());
        };
        let client = metadata_store.as_ref();

        client
            .with_transaction(|tx| {
                for file in files {
                    if let Some(hash) = file.content_hash.as_deref() {
                        cce_storage_sqlite::FileRepository::insert_hash_for_epoch(
                            tx,
                            &file.relative_path,
                            hash,
                            self.project_id,
                            self.epoch(),
                        )?;
                    }
                }
                Ok(())
            })
            .map_err(OrchestratorError::Storage)
    }

    /// Create candidate file rows without publishing change-detector hashes.
    /// Hashes are written only after the project manifest becomes active.
    pub fn ensure_file_records(&self, files: &[FileEntry]) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        let epoch = self.epoch();
        client
            .with_transaction(|tx| {
                for file in files {
                    let language = file
                        .language_info
                        .as_ref()
                        .map(|info| info.language.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    // Single-source category: reuse the scanner-detected
                    // routing info when present, otherwise recover it from
                    // the unified path detection chain.
                    let category = match &file.language_info {
                        Some(info) => info.file_category(),
                        None => cce_types::LanguageInfo::detect_from_path(
                            &file.relative_path.to_string_lossy(),
                        )
                        .file_category(),
                    }
                    .as_u8();
                    let now = chrono::Utc::now().timestamp();
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
                            file.relative_path.to_string_lossy(),
                            language,
                            category,
                            now,
                            self.project_id,
                            epoch,
                            self.batch_id(),
                        ],
                    )
                    .map_err(|error| {
                        cce_types::StorageError::insert(format!(
                            "failed to ensure file record: {error}"
                        ))
                    })?;
                }
                Ok(())
            })
            .map_err(OrchestratorError::Storage)
    }

    /// Persist parsed files into the ordinary entity generation.
    ///
    /// Relation snapshots and the ordinary entity tables are separate read
    /// models. Keeping this write in the same epoch as files, chunks and
    /// summaries makes the ordinary entity FTS path a real generation rather
    /// than an incidental side effect of relation indexing.
    pub fn store_parsed_files(
        &self,
        parsed_files: &[cce_types::ParsedFile],
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };

        let epoch = self.epoch();
        let batch_id = self.batch_id();
        client
            .with_transaction(|tx| {
                for parsed in parsed_files {
                    // Single-source category via the unified detection chain.
                    let category =
                        cce_types::LanguageInfo::detect_from_path(&parsed.path)
                            .file_category()
                            .as_u8();
                    tx.execute(
                        "INSERT INTO files
                            (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                         VALUES (?1, ?2, ?3, ?4, ?4, ?5, NULL, ?6, ?7)
                         ON CONFLICT(project_id, epoch, path) DO UPDATE SET
                            language = excluded.language,
                            category = excluded.category,
                            batch_id = excluded.batch_id",
                        rusqlite::params![
                            parsed.path,
                            parsed.language.to_string(),
                            category,
                            chrono::Utc::now().timestamp(),
                            self.project_id,
                            epoch,
                            batch_id,
                        ],
                    )
                    .map_err(|error| {
                        cce_types::StorageError::insert(format!(
                            "failed to ensure file record for {}: {error}",
                            parsed.path
                        ))
                    })?;

                    let file_id: i64 = tx
                        .query_row(
                            "SELECT id FROM files
                             WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
                            rusqlite::params![self.project_id, epoch, parsed.path],
                            |row| row.get(0),
                        )
                        .map_err(|error| {
                            cce_types::StorageError::query(format!(
                                "failed to resolve file record for {}: {error}",
                                parsed.path
                            ))
                        })?;

                    EntityRepository::delete_by_file_id_at_epoch(tx, file_id, epoch)?;

                    let scoped_names = parsed.resolve_all_scoped_names();
                    let mut inserted = Vec::with_capacity(parsed.entities.len());
                    for entity in &parsed.entities {
                        // resolve against the once-built map instead of
                        // rebuilding the id -> entity lookup per entity.
                        let scoped_name = scoped_names.get(&entity.id).cloned();
                        let mut metadata = entity.metadata.clone();
                        metadata.insert(
                            "__source_entity_id".to_string(),
                            entity.id.0.to_string(),
                        );
                        let record = EntityRecord {
                            id: 0,
                            name: entity.name.clone(),
                            kind: entity.kind.to_string(),
                            file_id,
                            signature: Some(entity.signature.clone()),
                            span_start_row: Some(entity.span.start_position.row as i64),
                            span_end_row: Some(entity.span.end_position.row as i64),
                            span_start_column: Some(entity.span.start_position.column as i64),
                            span_end_column: Some(entity.span.end_position.column as i64),
                            span_start_byte: Some(entity.span.start_byte as i64),
                            span_end_byte: Some(entity.span.end_byte as i64),
                            scoped_name,
                            depth: Some(entity.depth as i64),
                            parent_id: None,
                            metadata: Some(serde_json::to_string(&metadata).map_err(|error| {
                                cce_types::StorageError::insert(format!(
                                    "failed to serialize metadata for {}: {error}",
                                    entity.name
                                ))
                            })?),
                            parameters_json: Some(
                                serde_json::to_string(&entity.parameters).map_err(|error| {
                                    cce_types::StorageError::insert(format!(
                                        "failed to serialize parameters for {}: {error}",
                                        entity.name
                                    ))
                                })?,
                            ),
                            return_type: entity.return_type.clone(),
                            doc_comment: entity.doc_comment.clone(),
                            modifiers_json: Some(
                                serde_json::to_string(&entity.modifiers).map_err(|error| {
                                    cce_types::StorageError::insert(format!(
                                        "failed to serialize modifiers for {}: {error}",
                                        entity.name
                                    ))
                                })?,
                            ),
                            project_id: self.project_id,
                            epoch,
                            batch_id,
                        };
                        let db_id = EntityRepository::insert(tx, &record)?;
                        inserted.push((entity.id, db_id, entity.parent));
                    }

                    let source_to_db: std::collections::HashMap<_, _> = inserted
                        .iter()
                        .map(|(source_id, db_id, _)| (*source_id, *db_id))
                        .collect();
                    for (_source_id, db_id, parent_source_id) in inserted {
                        let Some(parent_source_id) = parent_source_id else {
                            continue;
                        };
                        let Some(parent_db_id) = source_to_db.get(&parent_source_id) else {
                            continue;
                        };
                        tx.execute(
                            "UPDATE entities SET parent_id = ?1
                             WHERE id = ?2 AND project_id = ?3 AND epoch = ?4",
                            rusqlite::params![
                                parent_db_id,
                                db_id,
                                self.project_id,
                                epoch,
                            ],
                        )
                        .map_err(|error| {
                            cce_types::StorageError::update(format!(
                                "failed to persist parent entity for {}: {error}",
                                parsed.path
                            ))
                        })?;
                    }
                }
                Ok(())
            })
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use super::super::StorageCoordinator;
    use cce_storage_sqlite::{EntityRepository, NewProjectRecord, ProjectRepository, SqliteClient};
    use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
    use cce_types::{Language, Span};
    use std::sync::Arc;

    #[test]
    fn stores_parsed_entities_in_the_target_epoch_with_parent_links() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())
            })
            .expect("project should be inserted");

        let mut parsed = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");
        let parent = Entity::new(
            EntityId(1),
            EntityKind::Struct,
            "Container".to_string(),
            Span::default(),
        );
        let child = Entity::new(
            EntityId(2),
            EntityKind::Method,
            "run".to_string(),
            Span::default(),
        )
        .with_parent(Some(EntityId(1)));
        parsed.add_entity(parent);
        parsed.add_entity(child);

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database)
            .with_epoch(7);
        storage
            .store_parsed_files(&[parsed])
            .expect("parsed entities should be stored");

        let conn = client.write_connection().expect("SQLite connection");
        let entities = EntityRepository::get_by_project_and_epoch_with_file_path(&conn, 1, 7)
            .expect("epoch entities should be queryable");
        assert_eq!(entities.len(), 2);
        let child = entities
            .iter()
            .find(|(entity, _)| entity.name == "run")
            .expect("child entity should exist");
        assert!(child.0.parent_id.is_some());
        let matches = EntityRepository::search_fts_at_epoch(&conn, "run", 1, 10, 7)
            .expect("epoch entity FTS should be queryable");
        assert_eq!(matches.len(), 1);
    }
}
