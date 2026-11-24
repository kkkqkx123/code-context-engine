use cce_types::{
    CanonicalComponentBatch, CanonicalRelationTarget, CanonicalSnapshotArtifact,
    DEFAULT_SNAPSHOT_BATCH_ITEMS, SnapshotArtifactCounts, Span, StableSymbolKey, StorageError,
};
use rusqlite::{OptionalExtension, Transaction, params};

use super::{RelationSnapshotRepository, RelationSnapshotState};
use super::{optional_json, query_error, require_state, to_json};

const INSERT_FILE: &str = "INSERT INTO relation_snapshot_files
    (project_id, relation_epoch, path, language, input_hash, file_size, imports_json)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

const INSERT_ENTITY: &str = "INSERT INTO relation_snapshot_entities (
    project_id, relation_epoch, file_id, scoped_name, kind_json,
    overload_discriminator, entity_id, name, signature, parameters_json,
    return_type, span_json, depth, doc_comment, modifiers_json,
    attributes_json, metadata_json, is_stdlib, stdlib_category_json, subtype
 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
           ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";

const FIND_FILE: &str = "SELECT id FROM relation_snapshot_files
    WHERE project_id = ?1 AND relation_epoch = ?2 AND path = ?3";

const FIND_ENTITY: &str = "SELECT e.id FROM relation_snapshot_entities e
    JOIN relation_snapshot_files f ON f.id = e.file_id
    WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND f.path = ?3
      AND e.scoped_name = ?4 AND e.kind_json = ?5 AND e.overload_discriminator = ?6";

impl RelationSnapshotRepository {
    /// Persist a canonical snapshot artifact in bounded batches.
    ///
    /// All row lookups go through the snapshot's unique indexes instead of a
    /// project-wide `HashMap` of file paths and symbol keys, so the writer's
    /// memory is bounded by one artifact batch rather than by project size.
    /// The caller's transaction keeps the all-or-nothing snapshot contract:
    /// the manifest stays `building` until every row is written.
    pub fn write_snapshot_and_mark_ready(
        tx: &Transaction<'_>,
        project_id: i64,
        epoch: i64,
        artifact: &CanonicalSnapshotArtifact,
    ) -> Result<(), StorageError> {
        require_state(tx, project_id, epoch, RelationSnapshotState::Building)?;
        let counts: SnapshotArtifactCounts = artifact.counts();
        let mut file_stmt = tx.prepare(INSERT_FILE).map_err(query_error)?;
        let mut entity_stmt = tx.prepare(INSERT_ENTITY).map_err(query_error)?;
        let mut parent_stmt = tx
            .prepare(
                "UPDATE relation_snapshot_entities SET parent_symbol_id = ?3
                 WHERE project_id = ?1 AND id = ?2",
            )
            .map_err(query_error)?;
        let relation_sql = "INSERT INTO relation_snapshot_relations (
            project_id, relation_epoch, caller_symbol_id, target_symbol_id,
            target_state, raw_target, relation_type_json, span_json,
            external_type_json, unresolved_reason, stdlib_category_json,
            call_context_json, owner_type, overload_signature, callee_symbol_json,
            call_frequency, cfg_condition
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)";
        let mut relation_stmt = tx.prepare(relation_sql).map_err(query_error)?;
        let mut placeholder_stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO relation_snapshot_entities (
                    project_id, relation_epoch, file_id, scoped_name, kind_json,
                    overload_discriminator, entity_id, name, signature, parameters_json,
                    return_type, span_json, depth, doc_comment, modifiers_json,
                    attributes_json, metadata_json, is_stdlib, stdlib_category_json, subtype
                 ) SELECT ?1, ?2, id, '<file>', ?3, ?4, NULL, '<file>', '', '[]',
                          NULL, ?5, 0, NULL, '[]', '{}', '{}', 0, NULL, NULL
                   FROM relation_snapshot_files
                   WHERE project_id = ?1 AND relation_epoch = ?2 AND path = ?6",
            )
            .map_err(query_error)?;
        let mut export_stmt = tx
            .prepare(
                "INSERT INTO relation_snapshot_exports
                 (project_id, relation_epoch, file_id, symbol_id, export_type)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(query_error)?;
        let mut dependency_stmt = tx
            .prepare(
                "INSERT INTO relation_snapshot_dependencies
                 (project_id, relation_epoch, source_file_id, target_path, source)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(query_error)?;

        for batch in artifact.batches(DEFAULT_SNAPSHOT_BATCH_ITEMS) {
            match batch.map_err(|error| StorageError::Transaction(error.to_string()))? {
                CanonicalComponentBatch::Files(files) => {
                    for file in files {
                        let imports_json = to_json(&file.imports)?;
                        file_stmt
                            .execute(params![
                                project_id,
                                epoch,
                                file.path,
                                file.language,
                                file.input_hash,
                                file.file_size as i64,
                                imports_json,
                            ])
                            .map_err(|error| unique_or_query("relation_snapshot_files", error))?;
                        // Exports are written with their file after all
                        // entities exist, so only the file row is created here.
                    }
                }
                CanonicalComponentBatch::Entities(entities) => {
                    let mut ids = Vec::with_capacity(entities.len());
                    for entity in &entities {
                        let file_id = find_file(tx, project_id, epoch, &entity.key.file_path)?;
                        let kind_json = to_json(&entity.key.kind)?;
                        let parameters_json = to_json(&entity.parameters)?;
                        let span_json = to_json(&entity.span)?;
                        let modifiers_json = to_json(&entity.modifiers)?;
                        let attributes_json = to_json(&entity.attributes)?;
                        let metadata_json = to_json(&entity.metadata)?;
                        let stdlib_category_json = optional_json(&entity.stdlib_category)?;
                        let entity_id = entity.entity_id.map(|id| id as i64);
                        let depth = entity.depth as i64;
                        let is_stdlib = i64::from(entity.is_stdlib);
                        entity_stmt
                            .execute(params![
                                project_id,
                                epoch,
                                file_id,
                                entity.key.scoped_name,
                                kind_json,
                                entity.key.overload_discriminator,
                                entity_id,
                                entity.name,
                                entity.signature,
                                parameters_json,
                                entity.return_type,
                                span_json,
                                depth,
                                entity.doc_comment,
                                modifiers_json,
                                attributes_json,
                                metadata_json,
                                is_stdlib,
                                stdlib_category_json,
                                entity.subtype,
                            ])
                            .map_err(|error| {
                                unique_or_query("relation_snapshot_entities", error)
                            })?;
                        ids.push(tx.last_insert_rowid());
                    }
                    for (entity, id) in entities.iter().zip(ids) {
                        if let Some(parent) = &entity.parent {
                            let parent_id = find_entity(tx, project_id, epoch, parent)?;
                            parent_stmt
                                .execute(params![project_id, id, parent_id])
                                .map_err(query_error)?;
                        }
                    }
                }
                CanonicalComponentBatch::Relations(relations) => {
                    for relation in relations {
                        if relation.caller.is_file_placeholder() {
                            let kind_json = to_json(&relation.caller.kind)?;
                            let span_json = to_json(&Span::default())?;
                            placeholder_stmt
                                .execute(params![
                                    project_id,
                                    epoch,
                                    kind_json,
                                    relation.caller.overload_discriminator,
                                    span_json,
                                    relation.caller.file_path,
                                ])
                                .map_err(query_error)?;
                        }
                        let caller_id = find_entity(tx, project_id, epoch, &relation.caller)?;
                        let (target_id, target_state, external_json, unresolved_reason) =
                            match &relation.target {
                                CanonicalRelationTarget::Internal { key } => (
                                    Some(find_entity(tx, project_id, epoch, key)?),
                                    "internal",
                                    None,
                                    None,
                                ),
                                CanonicalRelationTarget::External { classification } => {
                                    (None, "external", optional_json(classification)?, None)
                                }
                                CanonicalRelationTarget::Unresolved { reason } => {
                                    (None, "unresolved", None, Some(reason.as_str().to_string()))
                                }
                            };
                        let relation_type_json = to_json(&relation.relation_type)?;
                        let span_json = to_json(&relation.span)?;
                        let stdlib_category_json = optional_json(&relation.stdlib_category)?;
                        let call_context_json = to_json(&relation.call_context)?;
                        let callee_symbol_json = optional_json(&relation.callee_symbol)?;
                        relation_stmt
                            .execute(params![
                                project_id,
                                epoch,
                                caller_id,
                                target_id,
                                target_state,
                                relation.raw_target,
                                relation_type_json,
                                span_json,
                                external_json,
                                unresolved_reason,
                                stdlib_category_json,
                                call_context_json,
                                relation.owner_type,
                                relation.overload_signature,
                                callee_symbol_json,
                                relation.call_frequency as i64,
                                relation.cfg_condition,
                            ])
                            .map_err(query_error)?;
                    }
                }
                CanonicalComponentBatch::Dependencies(dependencies) => {
                    for dependency in dependencies {
                        let source_id = find_file(tx, project_id, epoch, &dependency.source_file)?;
                        dependency_stmt
                            .execute(params![
                                project_id,
                                epoch,
                                source_id,
                                dependency.target_file,
                                dependency.source,
                            ])
                            .map_err(query_error)?;
                    }
                }
            }
        }

        // Exports need both file rows and entity rows, so they are emitted
        // from a dedicated streaming pass over the artifact's file rows.
        artifact.for_each_file(|file| {
            let file_id = find_file(tx, project_id, epoch, &file.path)?;
            for export in file.exports {
                let Ok(symbol_id) = find_entity(tx, project_id, epoch, &export.symbol) else {
                    tracing::warn!(
                        file = %file.path,
                        symbol = ?export.symbol,
                        "Skipping export with no local snapshot entity"
                    );
                    continue;
                };
                export_stmt
                    .execute(params![
                        project_id,
                        epoch,
                        file_id,
                        symbol_id,
                        export.export_type,
                    ])
                    .map_err(query_error)?;
            }
            Ok(())
        })?;

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
                    artifact.input_fingerprint(),
                    artifact.snapshot_fingerprint(),
                    counts.files as i64,
                    counts.entities as i64,
                    counts.relations as i64,
                    counts.dependencies as i64,
                    chrono::Utc::now().timestamp(),
                    artifact.header().build_metadata.symbol_key_conflict_count as i64,
                    to_json(&artifact.header().build_metadata.symbol_key_conflict_samples)?,
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

fn find_file(
    tx: &Transaction<'_>,
    project_id: i64,
    epoch: i64,
    path: &str,
) -> Result<i64, StorageError> {
    tx.query_row(FIND_FILE, params![project_id, epoch, path], |row| {
        row.get(0)
    })
    .optional()
    .map_err(query_error)?
    .ok_or_else(|| StorageError::validation(format!("snapshot file {path} is missing")))
}

fn find_entity(
    tx: &Transaction<'_>,
    project_id: i64,
    epoch: i64,
    key: &StableSymbolKey,
) -> Result<i64, StorageError> {
    let kind_json = to_json(&key.kind)?;
    tx.query_row(
        FIND_ENTITY,
        params![
            project_id,
            epoch,
            key.file_path,
            key.scoped_name,
            kind_json,
            key.overload_discriminator,
        ],
        |row| row.get(0),
    )
    .optional()
    .map_err(query_error)?
    .ok_or_else(|| {
        StorageError::validation(format!(
            "snapshot symbol {} in {} is missing",
            key.scoped_name, key.file_path
        ))
    })
}

fn unique_or_query(table: &str, error: rusqlite::Error) -> StorageError {
    if matches!(
        error,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ffi::ErrorCode::ConstraintViolation,
                ..
            },
            _
        )
    ) {
        return StorageError::unique_violation(format!("duplicate key in {table}: {}", error));
    }
    query_error(error)
}
