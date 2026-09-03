use std::collections::{BTreeMap, HashMap};

use cce_types::{
    CanonicalEntity, CanonicalRelationSnapshot, CanonicalRelationTarget, Span, StableSymbolKey,
    StorageError,
};
use rusqlite::{ToSql, Transaction, params};

use crate::helpers::execute_insert_batch;

use super::{RelationSnapshotRepository, RelationSnapshotState};
use super::{optional_json, query_error, require_state, required, to_json};

impl RelationSnapshotRepository {
    /// Write only final query/recovery data. No Canonical DTO blob is retained.
    ///
    /// All inserts reuse a single prepared statement per table instead
    /// of re-parsing SQL per row; the caller's transaction preserves the
    /// all-or-nothing snapshot contract (the manifest stays `building` until
    /// every row is written).
    pub fn write_snapshot_and_mark_ready(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        require_state(tx, project_id, epoch, RelationSnapshotState::Building)?;

        // === 1. Files ===
        let file_sql = "INSERT INTO relation_snapshot_files
            (project_id, relation_epoch, path, language, input_hash, file_size, imports_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
        let file_ids: HashMap<String, i64> = {
            let imports_json: Vec<String> = snapshot
                .files
                .iter()
                .map(|file| to_json(&file.imports))
                .collect::<Result<_, _>>()?;
            let file_sizes: Vec<i64> = snapshot
                .files
                .iter()
                .map(|file| file.file_size as i64)
                .collect();
            let mut rows: Vec<Vec<&dyn ToSql>> = Vec::with_capacity(snapshot.files.len());
            for (i, file) in snapshot.files.iter().enumerate() {
                rows.push(vec![
                    &project_id,
                    &epoch,
                    &file.path,
                    &file.language,
                    &file.input_hash,
                    &file_sizes[i],
                    &imports_json[i],
                ]);
            }
            let row_ids = execute_insert_batch(tx, file_sql, &rows, "relation_snapshot_files")?;
            snapshot
                .files
                .iter()
                .zip(row_ids)
                .map(|(file, row_id)| (file.path.clone(), row_id))
                .collect()
        };

        // === 2. Entities ===
        let mut storage_entities: Vec<CanonicalEntity> = snapshot.entities.clone();
        {
            let mut placeholders: Vec<StableSymbolKey> = snapshot
                .relations
                .iter()
                .map(|relation| relation.caller.clone())
                .filter(|key| key.is_file_placeholder())
                .collect();
            placeholders.sort_by_key(|key| key.sort_key());
            placeholders.dedup_by(|a, b| a.sort_key() == b.sort_key());
            for key in placeholders {
                storage_entities.push(CanonicalEntity {
                    key,
                    entity_id: None,
                    name: "<file>".to_string(),
                    signature: String::new(),
                    parameters: Vec::new(),
                    return_type: None,
                    span: Span::default(),
                    depth: 0,
                    parent: None,
                    doc_comment: None,
                    modifiers: Vec::new(),
                    attributes: BTreeMap::new(),
                    metadata: BTreeMap::new(),
                    is_stdlib: false,
                    stdlib_category: None,
                    subtype: None,
                });
            }
        }
        let entity_sql = "INSERT INTO relation_snapshot_entities (
            project_id, relation_epoch, file_id, scoped_name, kind_json,
            overload_discriminator, entity_id, name, signature, parameters_json,
            return_type, span_json, depth, doc_comment, modifiers_json,
            attributes_json, metadata_json, is_stdlib, stdlib_category_json, subtype
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                   ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";
        let symbol_ids: HashMap<StableSymbolKey, i64> = {
            let kind_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.key.kind))
                .collect::<Result<_, _>>()?;
            let parameters_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.parameters))
                .collect::<Result<_, _>>()?;
            let span_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.span))
                .collect::<Result<_, _>>()?;
            let modifiers_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.modifiers))
                .collect::<Result<_, _>>()?;
            let attributes_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.attributes))
                .collect::<Result<_, _>>()?;
            let metadata_json: Vec<String> = storage_entities
                .iter()
                .map(|entity| to_json(&entity.metadata))
                .collect::<Result<_, _>>()?;
            let stdlib_category_json: Vec<Option<String>> = storage_entities
                .iter()
                .map(|entity| optional_json(&entity.stdlib_category))
                .collect::<Result<_, _>>()?;
            let entity_ids: Vec<Option<i64>> = storage_entities
                .iter()
                .map(|entity| entity.entity_id.map(|id| id as i64))
                .collect();
            let depths: Vec<i64> = storage_entities
                .iter()
                .map(|entity| entity.depth as i64)
                .collect();
            let is_stdlibs: Vec<i64> = storage_entities
                .iter()
                .map(|entity| i64::from(entity.is_stdlib))
                .collect();
            let mut rows: Vec<Vec<&dyn ToSql>> = Vec::with_capacity(storage_entities.len());
            for (i, entity) in storage_entities.iter().enumerate() {
                let file_id = required(&file_ids, &entity.key.file_path, "entity file")?;
                rows.push(vec![
                    &project_id,
                    &epoch,
                    file_id,
                    &entity.key.scoped_name,
                    &kind_json[i],
                    &entity.key.overload_discriminator,
                    &entity_ids[i],
                    &entity.name,
                    &entity.signature,
                    &parameters_json[i],
                    &entity.return_type,
                    &span_json[i],
                    &depths[i],
                    &entity.doc_comment,
                    &modifiers_json[i],
                    &attributes_json[i],
                    &metadata_json[i],
                    &is_stdlibs[i],
                    &stdlib_category_json[i],
                    &entity.subtype,
                ]);
            }
            let row_ids =
                execute_insert_batch(tx, entity_sql, &rows, "relation_snapshot_entities")?;
            storage_entities
                .iter()
                .zip(row_ids)
                .map(|(entity, row_id)| (entity.key.clone(), row_id))
                .collect()
        };

        // === 3. Parent fix-ups (prepared once) ===
        {
            let mut parent_stmt = tx
                .prepare(
                    "UPDATE relation_snapshot_entities SET parent_symbol_id = ?3
                     WHERE project_id = ?1 AND id = ?2",
                )
                .map_err(|e| {
                    StorageError::Transaction(format!("failed to prepare parent update: {e}"))
                })?;
            for entity in &snapshot.entities {
                if let Some(parent) = &entity.parent {
                    let entity_id = required(&symbol_ids, &entity.key, "entity")?;
                    let parent_id = required(&symbol_ids, parent, "parent entity")?;
                    parent_stmt
                        .execute(params![project_id, entity_id, parent_id])
                        .map_err(query_error)?;
                }
            }
        }

        // === 4. Relations ===
        let relation_sql = "INSERT INTO relation_snapshot_relations (
            project_id, relation_epoch, caller_symbol_id, target_symbol_id,
            target_state, raw_target, relation_type_json, span_json,
            external_type_json, unresolved_reason, stdlib_category_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";
        {
            let relation_type_json: Vec<String> = snapshot
                .relations
                .iter()
                .map(|relation| to_json(&relation.relation_type))
                .collect::<Result<_, _>>()?;
            let span_json: Vec<String> = snapshot
                .relations
                .iter()
                .map(|relation| to_json(&relation.span))
                .collect::<Result<_, _>>()?;
            let stdlib_category_json: Vec<Option<String>> = snapshot
                .relations
                .iter()
                .map(|relation| optional_json(&relation.stdlib_category))
                .collect::<Result<_, _>>()?;
            let target_ids: Vec<Option<i64>> = snapshot
                .relations
                .iter()
                .map(|relation| match &relation.target {
                    CanonicalRelationTarget::Internal { key } => {
                        required(&symbol_ids, key, "relation target").map(|id| Some(*id))
                    }
                    _ => Ok(None),
                })
                .collect::<Result<_, StorageError>>()?;
            let target_states: Vec<String> = snapshot
                .relations
                .iter()
                .map(|relation| match &relation.target {
                    CanonicalRelationTarget::Internal { .. } => "internal".to_string(),
                    CanonicalRelationTarget::External { .. } => "external".to_string(),
                    CanonicalRelationTarget::Unresolved { .. } => "unresolved".to_string(),
                })
                .collect();
            let externals: Vec<Option<String>> = snapshot
                .relations
                .iter()
                .map(|relation| match &relation.target {
                    CanonicalRelationTarget::External { classification } => {
                        optional_json(classification)
                    }
                    _ => Ok(None),
                })
                .collect::<Result<_, StorageError>>()?;
            let unresolveds: Vec<Option<String>> = snapshot
                .relations
                .iter()
                .map(|relation| match &relation.target {
                    CanonicalRelationTarget::Unresolved { reason } => {
                        Some(reason.as_str().to_string())
                    }
                    _ => None,
                })
                .collect();
            let mut rows: Vec<Vec<&dyn ToSql>> = Vec::with_capacity(snapshot.relations.len());
            for (i, relation) in snapshot.relations.iter().enumerate() {
                let caller_id = required(&symbol_ids, &relation.caller, "relation caller")?;
                rows.push(vec![
                    &project_id,
                    &epoch,
                    caller_id,
                    &target_ids[i],
                    &target_states[i],
                    &relation.raw_target,
                    &relation_type_json[i],
                    &span_json[i],
                    &externals[i],
                    &unresolveds[i],
                    &stdlib_category_json[i],
                ]);
            }
            execute_insert_batch(tx, relation_sql, &rows, "relation_snapshot_relations")?;
        }

        // === 5. Exports ===
        let export_sql = "INSERT INTO relation_snapshot_exports
            (project_id, relation_epoch, file_id, symbol_id, export_type)
         VALUES (?1, ?2, ?3, ?4, ?5)";
        {
            let mut rows: Vec<Vec<&dyn ToSql>> = Vec::new();
            for file in &snapshot.files {
                let file_id = required(&file_ids, &file.path, "export file")?;
                for export in &file.exports {
                    let symbol_id = required(&symbol_ids, &export.symbol, "export symbol")?;
                    rows.push(vec![
                        &project_id,
                        &epoch,
                        file_id,
                        symbol_id,
                        &export.export_type,
                    ]);
                }
            }
            execute_insert_batch(tx, export_sql, &rows, "relation_snapshot_exports")?;
        }

        // === 6. Dependencies ===
        let dependency_sql = "INSERT INTO relation_snapshot_dependencies
            (project_id, relation_epoch, source_file_id, target_path, source)
         VALUES (?1, ?2, ?3, ?4, ?5)";
        {
            let mut rows: Vec<Vec<&dyn ToSql>> = Vec::with_capacity(snapshot.dependencies.len());
            for dependency in &snapshot.dependencies {
                let source_id = required(&file_ids, &dependency.source_file, "dependency source")?;
                rows.push(vec![
                    &project_id,
                    &epoch,
                    source_id,
                    &dependency.target_file,
                    &dependency.source,
                ]);
            }
            execute_insert_batch(tx, dependency_sql, &rows, "relation_snapshot_dependencies")?;
        }

        let changed = tx
            .execute(
                "UPDATE relation_snapshot_manifest SET
                    state = 'ready', input_fingerprint = ?3, snapshot_fingerprint = ?4,
                    file_count = ?5, entity_count = ?6, relation_count = ?7,
                    dependency_count = ?8, validated_at = ?9, failure_reason = NULL,
                    symbol_key_conflict_count = ?10, symbol_key_conflict_samples_json = ?11
                 WHERE project_id = ?1 AND relation_epoch = ?2 AND state = 'building'",
                params![
                    project_id,
                    epoch,
                    input_fingerprint,
                    snapshot_fingerprint,
                    snapshot.files.len() as i64,
                    snapshot.entities.len() as i64,
                    snapshot.relations.len() as i64,
                    snapshot.dependencies.len() as i64,
                    chrono::Utc::now().timestamp(),
                    snapshot.build_metadata.symbol_key_conflict_count as i64,
                    to_json(&snapshot.build_metadata.symbol_key_conflict_samples)?
                ],
            )
            .map_err(query_error)?;
        if changed != 1 {
            return Err(StorageError::Transaction(format!(
                "epoch {epoch} did not transition to ready"
            )));
        }
        Ok(())
    }

    /// Write an incremental delta blob and insert a Delta manifest row.
    ///
    /// The caller must already have allocated a building epoch via
    /// [`Self::allocate_building`]. This method transitions the epoch from
    /// `building` to `delta`, writes the compressed delta payload, and
    /// inserts an `Active` manifest for the base epoch if not already active.
    pub fn write_delta(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
        delta: &cce_types::SnapshotDelta,
    ) -> Result<(), StorageError> {
        require_state(tx, project_id, epoch, RelationSnapshotState::Building)?;

        let delta_json = serde_json::to_vec(delta)
            .map_err(|e| StorageError::Validation(format!("delta serialization: {e}")))?;
        let compressed = zstd::encode_all(&*delta_json, 3)
            .map_err(|e| StorageError::Validation(format!("delta compression: {e}")))?;

        tx.execute(
            "UPDATE relation_snapshot_manifest SET state = 'delta'
             WHERE project_id = ?1 AND relation_epoch = ?2",
            params![project_id, epoch],
        )
        .map_err(query_error)?;

        tx.execute(
            "INSERT INTO relation_snapshot_deltas (project_id, base_epoch, delta_epoch, delta_data, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, delta.base_epoch, epoch, compressed, compressed.len() as i64],
        )
        .map_err(query_error)?;

        Ok(())
    }
}
