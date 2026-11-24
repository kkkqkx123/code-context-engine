//! Shard-directory and per-shard component reads for canonical epochs.

use std::collections::{HashMap, HashSet};

use cce_types::relation::classification::ExternalCallType;
use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
    CanonicalRelationTarget, RelationShardComponents, RelationShardDescriptor, ShardDirectory,
    ShardEntityRef, ShardLocation, StableSymbolKey, StorageError,
};
use rusqlite::{Connection, OptionalExtension, params};

use super::{RelationSnapshotManifest, RelationSnapshotRepository};
use super::{from_json, optional_from_json, query_error, required};

impl RelationSnapshotRepository {
    pub fn read_shard_directory(
        conn: &Connection,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        let mut directory = ShardDirectory::default();
        let mut statement = conn
            .prepare(
                "SELECT shard_id, file_count, entity_count, relation_count,\n                        dependency_count, estimated_bytes, path_start, path_end\n                 FROM relation_snapshot_shards\n                 WHERE project_id = ?1 AND relation_epoch = ?2 ORDER BY shard_id",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            directory.descriptors.push(RelationShardDescriptor {
                shard_id: row.get(0).map_err(query_error)?,
                file_count: row.get::<_, i64>(1).map_err(query_error)? as usize,
                entity_count: row.get::<_, i64>(2).map_err(query_error)? as usize,
                relation_count: row.get::<_, i64>(3).map_err(query_error)? as usize,
                dependency_count: row.get::<_, i64>(4).map_err(query_error)? as usize,
                estimated_bytes: row.get::<_, i64>(5).map_err(query_error)? as u64,
                path_start: row.get(6).map_err(query_error)?,
                path_end: row.get(7).map_err(query_error)?,
            });
        }
        drop(rows);
        drop(statement);

        let mut statement = conn
            .prepare(
                "SELECT e.entity_id, f.path, e.scoped_name, e.kind_json,\n                        e.overload_discriminator, e.stable_id, e.shard_id\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.entity_id IS NOT NULL",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let entity_id: i64 = row.get(0).map_err(query_error)?;
            let file_path: String = row.get(1).map_err(query_error)?;
            let key = StableSymbolKey {
                file_path: file_path.clone(),
                scoped_name: row.get(2).map_err(query_error)?,
                kind: from_json(&row.get::<_, String>(3).map_err(query_error)?)?,
                overload_discriminator: row.get(4).map_err(query_error)?,
            };
            let location = ShardLocation {
                shard_id: row.get(6).map_err(query_error)?,
                entity_id: entity_id as u64,
                file_path,
            };
            let stable_id: Option<String> = row.get(5).map_err(query_error)?;
            directory
                .entity_locations
                .entry(entity_id as u64)
                .or_insert_with(|| location.clone());
            directory
                .symbol_locations
                .entry(key.clone())
                .or_insert_with(|| location.clone());
            if let Some(stable_id) = stable_id {
                directory
                    .stable_id_locations
                    .entry(stable_id)
                    .or_insert(location.clone());
            }
        }
        drop(rows);
        drop(statement);

        let mut statement = conn
            .prepare(
                "SELECT path, shard_id FROM relation_snapshot_files\n                 WHERE project_id = ?1 AND relation_epoch = ?2",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            directory
                .file_shards
                .insert(row.get(0).map_err(query_error)?, row.get(1).map_err(query_error)?);
        }
        drop(rows);
        drop(statement);

        let mut statement = conn
            .prepare(
                "SELECT t.entity_id, c.shard_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 JOIN relation_snapshot_entities t ON t.id = r.target_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2 AND t.entity_id IS NOT NULL",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let target: i64 = row.get(0).map_err(query_error)?;
            let shard: i64 = row.get(1).map_err(query_error)?;
            let shards = directory.target_callers.entry(target as u64).or_default();
            if !shards.contains(&shard) {
                shards.push(shard);
            }
        }
        drop(rows);
        drop(statement);

        let mut statement = conn
            .prepare(
                "SELECT r.external_type_json, c.shard_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2\n                   AND r.target_state = 'external' AND r.external_type_json IS NOT NULL",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let classification: ExternalCallType =
                from_json(&row.get::<_, String>(0).map_err(query_error)?)?;
            let shard: i64 = row.get(1).map_err(query_error)?;
            let shards = directory
                .classification_shards
                .entry(classification.clone())
                .or_default();
            if !shards.contains(&shard) {
                shards.push(shard);
            }
            *directory.classification_counts.entry(classification).or_default() += 1;
        }
        drop(rows);
        drop(statement);

        let mut statement = conn
            .prepare(
                "SELECT sf.path, tf.path\n                 FROM relation_snapshot_dependencies d\n                 JOIN relation_snapshot_files sf ON sf.id = d.source_file_id\n                 LEFT JOIN relation_snapshot_files tf\n                   ON tf.project_id = d.project_id AND tf.relation_epoch = d.relation_epoch\n                  AND tf.path = d.target_path\n                 WHERE d.project_id = ?1 AND d.relation_epoch = ?2",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, relation_epoch])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let source: String = row.get(0).map_err(query_error)?;
            let target: String = row.get(1).map_err(query_error)?;
            let targets = directory.dependencies.entry(source.clone()).or_default();
            if !targets.contains(&target) {
                targets.push(target.clone());
            }
            let sources = directory.dependents.entry(target).or_default();
            if !sources.contains(&source) {
                sources.push(source);
            }
        }

        Ok(directory)
    }

    pub fn read_shard_components(
        conn: &Connection,
        manifest: &RelationSnapshotManifest,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        let project_id = manifest.project_id;
        let epoch = manifest.relation_epoch;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT shard_id FROM relation_snapshot_shards\n                 WHERE project_id = ?1 AND relation_epoch = ?2 AND shard_id = ?3",
                params![project_id, epoch, shard_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?;
        if exists.is_none() {
            return Ok(None);
        }
        let mut components = RelationShardComponents {
            files: Vec::new(),
            entities: Vec::new(),
            relations: Vec::new(),
            dependencies: Vec::new(),
            target_entities: Vec::new(),
        };

        let mut statement = conn
            .prepare(
                "SELECT path, language, input_hash, file_size, imports_json\n                 FROM relation_snapshot_files\n                 WHERE project_id = ?1 AND relation_epoch = ?2 AND shard_id = ?3 ORDER BY path",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, shard_id])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            components.files.push(CanonicalFile {
                path: row.get(0).map_err(query_error)?,
                language: row.get(1).map_err(query_error)?,
                input_hash: row.get(2).map_err(query_error)?,
                file_size: row.get::<_, i64>(3).map_err(query_error)? as u64,
                imports: from_json(&row.get::<_, String>(4).map_err(query_error)?)?,
                exports: Vec::new(),
            });
        }
        drop(rows);
        drop(statement);

        let mut keys: HashMap<i64, StableSymbolKey> = HashMap::new();
        let mut statement = conn
            .prepare(&ENTITY_QUERY.with_shard("e.shard_id = ?3"))
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, shard_id])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let (id, key, entity) = read_entity_row(row)?;
            keys.insert(id, key.clone());
            if !key.is_file_placeholder() {
                components.entities.push(entity);
            }
        }
        drop(rows);
        drop(statement);

        let needed_ids: HashSet<i64> = read_needed_entity_ids(conn, project_id, epoch, shard_id)?;
        if !needed_ids.is_empty() {
            let mut statement = conn
                .prepare(&ENTITY_QUERY.with_parent(&format!(
                    "e.shard_id <> ?3 AND e.id IN ({})",
                    vec!["?"; needed_ids.len()].join(",")
                )))
                .map_err(query_error)?;
            let mut params: Vec<i64> = vec![project_id, epoch, shard_id];
            params.extend(needed_ids.iter().copied());
            let mut rows = statement.query(rusqlite::params_from_iter(params)).map_err(query_error)?;
            while let Some(row) = rows.next().map_err(query_error)? {
                let (id, key, _) = read_entity_row(row)?;
                let entity_id: Option<i64> = row.get(20).map_err(query_error)?;
                let file_path = key.file_path.clone();
                if let Some(entity_id) = entity_id {
                    keys.insert(id, key.clone());
                    components.target_entities.push(ShardEntityRef {
                        key,
                        entity_id: entity_id as u64,
                        file_path,
                    });
                }
            }
        }

        let mut statement = conn
            .prepare(
                "SELECT r.caller_symbol_id, r.target_symbol_id, r.target_state, r.raw_target,\n                        r.relation_type_json, r.span_json, r.external_type_json,\n                        r.unresolved_reason, r.stdlib_category_json,\n                        r.call_context_json, r.owner_type, r.overload_signature,\n                        r.callee_symbol_json, r.call_frequency, r.cfg_condition\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2 AND c.shard_id = ?3\n                 ORDER BY r.id",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, shard_id])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            let caller_id: i64 = row.get(0).map_err(query_error)?;
            let target_id: Option<i64> = row.get(1).map_err(query_error)?;
            let state: String = row.get(2).map_err(query_error)?;
            let target = match state.as_str() {
                "internal" => CanonicalRelationTarget::Internal {
                    key: required(
                        &keys,
                        &target_id.ok_or_else(|| {
                            StorageError::validation("internal relation has no target")
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
                            StorageError::validation("unresolved relation has no reason")
                        })?
                        .parse()
                        .map_err(StorageError::Validation)?,
                },
                other => {
                    return Err(StorageError::validation(format!(
                        "invalid relation target state: {other}"
                    )));
                }
            };
            let call_context = match row.get::<_, Option<String>>(9).map_err(query_error)? {
                Some(raw) => from_json(&raw)?,
                None => cce_types::relation::CallContext::Direct,
            };
            components.relations.push(CanonicalRelation {
                caller: required(&keys, &caller_id, "relation caller")?.clone(),
                target,
                raw_target: row.get(3).map_err(query_error)?,
                relation_type: from_json(&row.get::<_, String>(4).map_err(query_error)?)?,
                span: from_json(&row.get::<_, String>(5).map_err(query_error)?)?,
                stdlib_category: optional_from_json(row.get(8).map_err(query_error)?)?,
                overload_signature: row.get(11).map_err(query_error)?,
                callee_symbol: optional_from_json(row.get(12).map_err(query_error)?)?,
                owner_type: row.get(10).map_err(query_error)?,
                call_context,
                call_frequency: row.get::<_, i64>(13).map_err(query_error)? as u64,
                cfg_condition: row.get(14).map_err(query_error)?,
            });
        }
        drop(rows);
        drop(statement);

        for file in &mut components.files {
            let mut export_stmt = conn
                .prepare(
                    "SELECT e.scoped_name, e.kind_json, e.overload_discriminator,
                            x.export_type, sf.path
                     FROM relation_snapshot_exports x
                     JOIN relation_snapshot_entities e ON e.id = x.symbol_id
                     JOIN relation_snapshot_files sf ON sf.id = e.file_id
                     WHERE x.project_id = ?1 AND x.relation_epoch = ?2 AND x.file_id = (
                        SELECT id FROM relation_snapshot_files
                        WHERE project_id = ?1 AND relation_epoch = ?2 AND path = ?3
                     )",
                )
                .map_err(query_error)?;
            let mut export_rows = export_stmt
                .query(params![project_id, epoch, file.path])
                .map_err(query_error)?;
            while let Some(row) = export_rows.next().map_err(query_error)? {
                let kind_json: String = row.get(1).map_err(query_error)?;
                file.exports.push(CanonicalExport {
                    symbol: StableSymbolKey {
                        file_path: row.get(4).map_err(query_error)?,
                        scoped_name: row.get(0).map_err(query_error)?,
                        kind: from_json(&kind_json)?,
                        overload_discriminator: row.get(2).map_err(query_error)?,
                    },
                    export_type: row.get(3).map_err(query_error)?,
                });
            }
        }

        let mut statement = conn
            .prepare(
                "SELECT sf.path, d.target_path, d.source\n                 FROM relation_snapshot_dependencies d\n                 JOIN relation_snapshot_files sf ON sf.id = d.source_file_id\n                 WHERE d.project_id = ?1 AND d.relation_epoch = ?2 AND sf.shard_id = ?3",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, shard_id])
            .map_err(query_error)?;
        while let Some(row) = rows.next().map_err(query_error)? {
            components.dependencies.push(CanonicalDependency {
                source_file: row.get(0).map_err(query_error)?,
                target_file: row.get(1).map_err(query_error)?,
                source: row.get(2).map_err(query_error)?,
            });
        }

        components.entities.sort_by_key(|entity| entity.key.sort_key());

        Ok(Some(components))
    }
}

struct EntityQuery(&'static str);

impl EntityQuery {
    fn with_shard(&self, predicate: &str) -> String {
        self.with_parent(predicate)
    }

    fn with_parent(&self, predicate: &str) -> String {
        format!(
            "SELECT e.id, f.path, e.scoped_name, e.kind_json, e.overload_discriminator,\n                    e.name, e.signature, e.parameters_json, e.return_type, e.span_json,\n                    e.depth, e.parent_symbol_id, e.doc_comment, e.modifiers_json,\n                    e.attributes_json, e.metadata_json, e.is_stdlib,\n                    e.stdlib_category_json, e.subtype, e.entity_id\n             FROM relation_snapshot_entities e\n             JOIN relation_snapshot_files f ON f.id = e.file_id\n             WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND {predicate}\n             ORDER BY e.id"
        )
    }
}

const ENTITY_QUERY: EntityQuery = EntityQuery("");

fn read_entity_row(
    row: &rusqlite::Row<'_>,
) -> Result<(i64, StableSymbolKey, CanonicalEntity), StorageError> {
    let id: i64 = row.get(0).map_err(query_error)?;
    let key = StableSymbolKey {
        file_path: row.get(1).map_err(query_error)?,
        scoped_name: row.get(2).map_err(query_error)?,
        kind: from_json(&row.get::<_, String>(3).map_err(query_error)?)?,
        overload_discriminator: row.get(4).map_err(query_error)?,
    };
    let persisted_entity_id: Option<i64> = row.get(19).map_err(query_error)?;
    let entity = CanonicalEntity {
        key: key.clone(),
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
    };
    Ok((id, key, entity))
}

fn read_needed_entity_ids(
    conn: &Connection,
    project_id: i64,
    epoch: i64,
    shard_id: i64,
) -> Result<HashSet<i64>, StorageError> {
    let mut needed = HashSet::new();
    let mut statement = conn
        .prepare(
            "SELECT r.target_symbol_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2 AND c.shard_id = ?3\n                   AND r.target_symbol_id IS NOT NULL\n                 UNION\n                 SELECT e.parent_symbol_id\n                 FROM relation_snapshot_entities e\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.shard_id = ?3\n                   AND e.parent_symbol_id IS NOT NULL",
        )
        .map_err(query_error)?;
    let mut rows = statement
        .query(params![project_id, epoch, shard_id])
        .map_err(query_error)?;
    while let Some(row) = rows.next().map_err(query_error)? {
        needed.insert(row.get(0).map_err(query_error)?);
    }
    Ok(needed)
}
