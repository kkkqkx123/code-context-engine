//! Relation-store file and entity persistence.

use cce_scanner::FileEntry;
use cce_storage_common::RelationStorage;
use cce_storage_common::metadb::{EntityRecord, FileEnsureEntry};

use crate::error::OrchestratorError;

use super::StorageCoordinator;

impl StorageCoordinator {
    pub async fn publish_file_hashes(&self, files: &[FileEntry]) -> Result<(), OrchestratorError> {
        let Some(store) = &self.metadata_store else {
            return Ok(());
        };
        let entries: Vec<(String, String)> = files
            .iter()
            .filter_map(|file| {
                file.content_hash.as_deref().map(|hash| {
                    (
                        file.relative_path.to_string_lossy().into_owned(),
                        hash.to_string(),
                    )
                })
            })
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        store
            .files_record_hashes(self.project_id, self.epoch(), &entries)
            .await
            .map(|_| ())
            .map_err(OrchestratorError::Storage)
    }

    /// Create candidate file rows without publishing change-detector hashes.
    /// Hashes are written only after the project manifest becomes active.
    pub async fn ensure_file_records(&self, files: &[FileEntry]) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };
        let epoch = self.epoch();
        let now = chrono::Utc::now().timestamp();
        let entries: Vec<FileEnsureEntry> = files
            .iter()
            .map(|file| {
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
                FileEnsureEntry {
                    path: file.relative_path.to_string_lossy().into_owned(),
                    language,
                    category,
                    last_modified: now,
                }
            })
            .collect();
        store
            .files_ensure(self.project_id, epoch, self.batch_id(), &entries)
            .await
            .map(|_| ())
            .map_err(OrchestratorError::Storage)
    }

    /// Persist parsed files into the ordinary entity generation.
    ///
    /// Relation snapshots and the ordinary entity tables are separate read
    /// models. Keeping this write in the same epoch as files, chunks and
    /// summaries makes the ordinary entity FTS path a real generation rather
    /// than an incidental side effect of relation indexing.
    pub async fn store_parsed_files(
        &self,
        parsed_files: &[cce_types::ParsedFile],
    ) -> Result<(), OrchestratorError> {
        let Some(store) = self.metadata_store.as_ref() else {
            return Ok(());
        };

        let epoch = self.epoch();
        let batch_id = self.batch_id();
        for parsed in parsed_files {
            // Single-source category via the unified detection chain.
            let category = cce_types::LanguageInfo::detect_from_path(&parsed.path)
                .file_category()
                .as_u8();
            store
                .files_ensure(
                    self.project_id,
                    epoch,
                    batch_id,
                    &[FileEnsureEntry {
                        path: parsed.path.clone(),
                        language: parsed.language.to_string(),
                        category,
                        last_modified: chrono::Utc::now().timestamp(),
                    }],
                )
                .await
                .map_err(OrchestratorError::Storage)?;

            let file = store
                .file_by_path(self.project_id, &parsed.path)
                .await
                .map_err(OrchestratorError::Storage)?
                .ok_or_else(|| {
                    OrchestratorError::Storage(cce_types::StorageError::query(format!(
                        "failed to resolve file record for {}",
                        parsed.path
                    )))
                })?;
            let file_id = file.id;
            store
                .entities_delete_by_file(file_id, epoch)
                .await
                .map_err(OrchestratorError::Storage)?;

            let scoped_names = parsed.resolve_all_scoped_names();
            // The symbol key aligns with the relation side:
            // (project, epoch, file, scoped_name, kind,
            // overload_discriminator). Overloads and cfg-gated
            // duplicates share the name but carry distinct
            // discriminators, so no suffix mangling is needed. The
            // source entity link stays exact via `__source_entity_id`.
            let mut records = Vec::with_capacity(parsed.entities.len());
            let mut sources = Vec::with_capacity(parsed.entities.len());
            for entity in &parsed.entities {
                // resolve against the once-built map instead of
                // rebuilding the id -> entity map per entity.
                let scoped_name = scoped_names.get(&entity.id).cloned();
                let overload_discriminator =
                    scoped_name.as_ref().map_or_else(String::new, |name| {
                        cce_types::StableSymbolKey::for_entity(&parsed.path, name, entity)
                            .overload_discriminator
                    });
                let mut metadata = entity.metadata.clone();
                metadata.insert("__source_entity_id".to_string(), entity.id.0.to_string());
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
                    overload_discriminator,
                    depth: Some(entity.depth as i64),
                    parent_id: None,
                    metadata: Some(serde_json::to_string(&metadata).map_err(|error| {
                        cce_types::StorageError::insert(
                            "entities",
                            format!("failed to serialize metadata for {}: {error}", entity.name),
                        )
                    })?),
                    parameters_json: Some(serde_json::to_string(&entity.parameters).map_err(
                        |error| {
                            cce_types::StorageError::insert(
                                "entities",
                                format!(
                                    "failed to serialize parameters for {}: {error}",
                                    entity.name
                                ),
                            )
                        },
                    )?),
                    return_type: entity.return_type.clone(),
                    doc_comment: entity.doc_comment.clone(),
                    modifiers_json: Some(serde_json::to_string(&entity.modifiers).map_err(
                        |error| {
                            cce_types::StorageError::insert(
                                "entities",
                                format!(
                                    "failed to serialize modifiers for {}: {error}",
                                    entity.name
                                ),
                            )
                        },
                    )?),
                    project_id: self.project_id,
                    epoch,
                    batch_id,
                    rank: 0.0,
                };
                sources.push((entity.id, entity.parent));
                records.push(record);
            }
            let db_ids = store
                .entities_insert_returning_ids(&records)
                .await
                .map_err(OrchestratorError::Storage)?;
            let source_to_db: std::collections::HashMap<_, _> = sources
                .iter()
                .copied()
                .zip(db_ids.iter().copied())
                .map(|((source_id, _), db_id)| (source_id, db_id))
                .collect();
            for ((_source_id, parent_source_id), db_id) in sources.into_iter().zip(db_ids) {
                let Some(parent_source_id) = parent_source_id else {
                    continue;
                };
                let Some(parent_db_id) = source_to_db.get(&parent_source_id) else {
                    continue;
                };
                store
                    .entities_set_parent(self.project_id, epoch, db_id, *parent_db_id)
                    .await
                    .map_err(OrchestratorError::Storage)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::StorageCoordinator;
    use cce_storage_common::RelationStorage;
    use cce_storage_metadb::RelationStore;
    use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
    use cce_types::{Language, Span};

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[test]
    fn stores_parsed_entities_in_the_target_epoch_with_parent_links() {
        let database = RelationStore::in_memory().expect("in-memory store");
        block_on(database.ensure_project(1, "/tmp/test")).expect("project should be ensured");

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
            .with_metadata_store(database.clone())
            .with_epoch(7);
        block_on(storage.store_parsed_files(&[parsed])).expect("parsed entities should be stored");

        let entities = block_on(database.entities_with_files(1, 7))
            .expect("epoch entities should be queryable");
        assert_eq!(entities.len(), 2);
        let child = entities
            .iter()
            .find(|entry| entry.entity.name == "run")
            .expect("child entity should exist");
        assert!(child.entity.parent_id.is_some());
        let matches = block_on(database.entities_search_fts(1, "run", 10, 7, &[]))
            .expect("epoch entity FTS should be queryable");
        assert_eq!(matches.len(), 1);
    }
}
