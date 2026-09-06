use std::collections::HashMap;

use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
    CanonicalRelationSnapshot, CanonicalRelationTarget, StableSymbolKey, StorageError,
};
use rusqlite::{Connection, params};

use super::{RelationSnapshotManifest, RelationSnapshotRepository};
use super::{from_json, optional_from_json, query_error, required};

impl RelationSnapshotRepository {
    pub fn read_snapshot(
        conn: &Connection,
        manifest: &RelationSnapshotManifest,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        let mut snapshot = CanonicalRelationSnapshot::new(manifest.config_fingerprint.clone());
        snapshot.schema_version = manifest.schema_version;
        snapshot.parser_version = manifest.parser_version;
        snapshot.resolver_version = manifest.resolver_version;
        snapshot.path_normalization_version = manifest.path_normalization_version;

        let mut file_rows = conn
            .prepare(
                "SELECT id, path, language, input_hash, file_size, imports_json
                 FROM relation_snapshot_files
                 WHERE project_id = ?1 AND relation_epoch = ?2 ORDER BY path",
            )
            .map_err(query_error)?;
        let mut rows = file_rows
            .query(params![manifest.project_id, manifest.relation_epoch])
            .map_err(query_error)?;
        let mut file_indexes = HashMap::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            let id: i64 = row.get(0).map_err(query_error)?;
            let index = snapshot.files.len();
            snapshot.files.push(CanonicalFile {
                path: row.get(1).map_err(query_error)?,
                language: row.get(2).map_err(query_error)?,
                input_hash: row.get(3).map_err(query_error)?,
                file_size: row.get::<_, i64>(4).map_err(query_error)? as u64,
                imports: from_json(&row.get::<_, String>(5).map_err(query_error)?)?,
                exports: Vec::new(),
            });
            file_indexes.insert(id, index);
        }
        drop(rows);
        drop(file_rows);

        let mut entity_rows = conn
            .prepare(
                "SELECT e.id, f.path, e.scoped_name, e.kind_json, e.overload_discriminator,
                        e.name, e.signature, e.parameters_json, e.return_type, e.span_json,
                        e.depth, e.parent_symbol_id, e.doc_comment, e.modifiers_json,
                        e.attributes_json, e.metadata_json, e.is_stdlib,
                        e.stdlib_category_json, e.subtype,
                        e.entity_id
                 FROM relation_snapshot_entities e
                 JOIN relation_snapshot_files f ON f.id = e.file_id
                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 ORDER BY e.id",
            )
            .map_err(query_error)?;
        let mut rows = entity_rows
            .query(params![manifest.project_id, manifest.relation_epoch])
            .map_err(query_error)?;
        let mut entity_ids = HashMap::new();
        let mut parents = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            let id: i64 = row.get(0).map_err(query_error)?;
            let key = StableSymbolKey {
                file_path: row.get(1).map_err(query_error)?,
                scoped_name: row.get(2).map_err(query_error)?,
                kind: from_json(&row.get::<_, String>(3).map_err(query_error)?)?,
                overload_discriminator: row.get(4).map_err(query_error)?,
            };
            let persisted_entity_id: Option<i64> = row.get(19).map_err(query_error)?;
            entity_ids.insert(id, key.clone());
            if key.is_file_placeholder() {
                continue;
            }
            parents.push(row.get::<_, Option<i64>>(11).map_err(query_error)?);
            snapshot.entities.push(CanonicalEntity {
                key,
                entity_id: persisted_entity_id.map(|id| id as u64),
                name: row.get(5).map_err(query_error)?,
                signature: row.get(6).map_err(query_error)?,
                parameters: from_json(&row.get::<_, String>(7).map_err(query_error)?)?,
                return_type: row.get(8).map_err(query_error)?,
                span: from_json(&row.get::<_, String>(9).map_err(query_error)?)?,
                depth: row.get::<_, i64>(10).map_err(query_error)? as usize,
                parent: None,
                doc_comment: row.get(12).map_err(query_error)?,
                modifiers: from_json(&row.get::<_, String>(13).map_err(query_error)?)?,
                attributes: from_json(&row.get::<_, String>(14).map_err(query_error)?)?,
                metadata: from_json(&row.get::<_, String>(15).map_err(query_error)?)?,
                is_stdlib: row.get::<_, i64>(16).map_err(query_error)? != 0,
                stdlib_category: optional_from_json(row.get(17).map_err(query_error)?)?,
                subtype: row.get(18).map_err(query_error)?,
            });
        }
        for (entity, parent_id) in snapshot.entities.iter_mut().zip(parents) {
            entity.parent = parent_id
                .map(|id| required(&entity_ids, &id, "parent symbol").cloned())
                .transpose()?;
        }
        drop(rows);
        drop(entity_rows);

        read_relations(conn, manifest, &entity_ids, &mut snapshot)?;
        read_exports(conn, manifest, &entity_ids, &file_indexes, &mut snapshot)?;
        read_dependencies(conn, manifest, &file_indexes, &mut snapshot)?;
        snapshot.build_metadata.symbol_key_conflict_count = manifest.symbol_key_conflict_count;
        snapshot.build_metadata.symbol_key_conflict_samples =
            manifest.symbol_key_conflict_samples.clone();
        snapshot.normalize();

        Ok(snapshot)
    }
}

fn read_relations(
    conn: &Connection,
    manifest: &RelationSnapshotManifest,
    entities: &HashMap<i64, StableSymbolKey>,
    snapshot: &mut CanonicalRelationSnapshot,
) -> Result<(), StorageError> {
    let mut statement = conn
        .prepare(
            "SELECT caller_symbol_id, target_symbol_id, target_state, raw_target,
                    relation_type_json, span_json, external_type_json,
                    unresolved_reason, stdlib_category_json
             FROM relation_snapshot_relations
             WHERE project_id = ?1 AND relation_epoch = ?2 ORDER BY id",
        )
        .map_err(query_error)?;
    let mut rows = statement
        .query(params![manifest.project_id, manifest.relation_epoch])
        .map_err(query_error)?;
    while let Some(row) = rows.next().map_err(query_error)? {
        let caller_id: i64 = row.get(0).map_err(query_error)?;
        let target_id: Option<i64> = row.get(1).map_err(query_error)?;
        let state: String = row.get(2).map_err(query_error)?;
        let target = match state.as_str() {
            "internal" => CanonicalRelationTarget::Internal {
                key: required(
                    entities,
                    &target_id.ok_or_else(|| {
                        StorageError::Validation("internal relation has no target".to_string())
                    })?,
                    "relation target",
                )?
                .clone(),
            },
            "external" => CanonicalRelationTarget::External {
                classification: optional_from_json(row.get(6).map_err(query_error)?)?,
            },
            "unresolved" => CanonicalRelationTarget::Unresolved {
                reason: row
                    .get::<_, Option<String>>(7)
                    .map_err(query_error)?
                    .ok_or_else(|| {
                        StorageError::Validation("unresolved relation has no reason".to_string())
                    })?
                    .parse()
                    .map_err(StorageError::Validation)?,
            },
            _ => {
                return Err(StorageError::Validation(format!(
                    "invalid relation target state: {state}"
                )));
            }
        };
        snapshot.relations.push(CanonicalRelation {
            caller: required(entities, &caller_id, "relation caller")?.clone(),
            target,
            raw_target: row.get(3).map_err(query_error)?,
            relation_type: from_json(&row.get::<_, String>(4).map_err(query_error)?)?,
            span: from_json(&row.get::<_, String>(5).map_err(query_error)?)?,
            stdlib_category: optional_from_json(row.get(8).map_err(query_error)?)?,
            // The SQLite schema carries no overload column; rows read back
            // as single-candidate edges and re-resolution re-annotates them.
            overload_signature: None,
        });
    }
    Ok(())
}

fn read_exports(
    conn: &Connection,
    manifest: &RelationSnapshotManifest,
    entities: &HashMap<i64, StableSymbolKey>,
    files: &HashMap<i64, usize>,
    snapshot: &mut CanonicalRelationSnapshot,
) -> Result<(), StorageError> {
    let mut statement = conn
        .prepare(
            "SELECT file_id, symbol_id, export_type FROM relation_snapshot_exports
             WHERE project_id = ?1 AND relation_epoch = ?2",
        )
        .map_err(query_error)?;
    let mut rows = statement
        .query(params![manifest.project_id, manifest.relation_epoch])
        .map_err(query_error)?;
    while let Some(row) = rows.next().map_err(query_error)? {
        let file_id: i64 = row.get(0).map_err(query_error)?;
        let symbol_id: i64 = row.get(1).map_err(query_error)?;
        let file_index = *required(files, &file_id, "export file")?;
        snapshot.files[file_index].exports.push(CanonicalExport {
            symbol: required(entities, &symbol_id, "export symbol")?.clone(),
            export_type: row.get(2).map_err(query_error)?,
        });
    }
    Ok(())
}

fn read_dependencies(
    conn: &Connection,
    manifest: &RelationSnapshotManifest,
    files: &HashMap<i64, usize>,
    snapshot: &mut CanonicalRelationSnapshot,
) -> Result<(), StorageError> {
    let mut statement = conn
        .prepare(
            "SELECT source_file_id, target_path, source FROM relation_snapshot_dependencies
             WHERE project_id = ?1 AND relation_epoch = ?2",
        )
        .map_err(query_error)?;
    let mut rows = statement
        .query(params![manifest.project_id, manifest.relation_epoch])
        .map_err(query_error)?;
    while let Some(row) = rows.next().map_err(query_error)? {
        let file_id: i64 = row.get(0).map_err(query_error)?;
        let file_index = *required(files, &file_id, "dependency source")?;
        snapshot.dependencies.push(CanonicalDependency {
            source_file: snapshot.files[file_index].path.clone(),
            target_file: row.get(1).map_err(query_error)?,
            source: row.get(2).map_err(query_error)?,
        });
    }
    Ok(())
}
