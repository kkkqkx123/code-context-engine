//! Shard-directory and per-shard component reads for canonical epochs.

use std::collections::{HashMap, HashSet};

use cce_types::relation::classification::ExternalCallType;
use cce_types::relation::snapshot_shard::external_call_type_json;
use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
    CanonicalRelationTarget, RelationShardComponents, RelationShardDescriptor, ShardDirectory,
    ShardEntityRef, ShardLocation, StableSymbolKey, StorageError,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json;

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
            .prepare(&entity_query("e.shard_id = ?3"))
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
                .prepare(&entity_query(&format!(
                    "e.shard_id <> ?3 AND e.id IN ({})",
                    vec!["?"; needed_ids.len()].join(",")
                )))
                .map_err(query_error)?;
            let mut params: Vec<i64> = vec![project_id, epoch, shard_id];
            params.extend(needed_ids.iter().copied());
            let mut rows = statement
                .query(rusqlite::params_from_iter(params))
                .map_err(query_error)?;
            while let Some(row) = rows.next().map_err(query_error)? {
                let (id, key, _) = read_entity_row(row)?;
                let entity_id: Option<i64> = row.get(19).map_err(query_error)?;
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

        components
            .entities
            .sort_by_key(|entity| entity.key.sort_key());

        Ok(Some(components))
    }
}

fn entity_query(predicate: &str) -> String {
    format!(
        "SELECT e.id, f.path, e.scoped_name, e.kind_json, e.overload_discriminator,\n                e.name, e.signature, e.parameters_json, e.return_type, e.span_json,\n                e.depth, e.parent_symbol_id, e.doc_comment, e.modifiers_json,\n                e.attributes_json, e.metadata_json, e.is_stdlib,\n                e.stdlib_category_json, e.subtype, e.entity_id\n         FROM relation_snapshot_entities e\n         JOIN relation_snapshot_files f ON f.id = e.file_id\n         WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND {predicate}\n         ORDER BY e.id"
    )
}

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
            "SELECT r.target_symbol_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2 AND c.shard_id = ?3\n                   AND r.target_symbol_id IS NOT NULL\n                 UNION\n                 SELECT e.parent_symbol_id\n                 FROM relation_snapshot_entities e\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.shard_id = ?3\n                   AND e.parent_symbol_id IS NOT NULL
                 UNION
                 SELECT x.symbol_id
                 FROM relation_snapshot_exports x
                 JOIN relation_snapshot_files f ON f.id = x.file_id
                 WHERE x.project_id = ?1 AND x.relation_epoch = ?2 AND f.shard_id = ?3",
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

impl RelationSnapshotRepository {
    pub fn entity_location_by_id(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        conn.query_row(
            "SELECT e.shard_id, f.path, e.name\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.entity_id = ?3\n                 ORDER BY e.id LIMIT 1",
            params![project_id, epoch, entity_id as i64],
            |row| {
                Ok(ShardLocation {
                    shard_id: row.get(0)?,
                    entity_id,
                    file_path: row.get(1)?,
                    name: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(query_error)
    }

    pub fn symbol_key_by_entity_id(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        conn.query_row(
            "SELECT f.path, e.scoped_name, e.kind_json, e.overload_discriminator\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.entity_id = ?3\n                 ORDER BY e.id LIMIT 1",
            params![project_id, epoch, entity_id as i64],
            |row| {
                Ok(StableSymbolKey {
                    file_path: row.get(0)?,
                    scoped_name: row.get(1)?,
                    kind: serde_json::from_str(&row.get::<_, String>(2)?)
                        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        ))?,
                    overload_discriminator: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(query_error)
    }

    pub fn entity_location_by_symbol(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        conn.query_row(
            "SELECT e.shard_id, e.entity_id, e.name\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND f.path = ?3\n                   AND e.scoped_name = ?4 AND e.kind_json = ?5\n                   AND e.overload_discriminator = ?6 AND e.entity_id IS NOT NULL\n                 ORDER BY e.id LIMIT 1",
            params![
                project_id,
                epoch,
                key.file_path,
                key.scoped_name,
                serde_json::to_string(&key.kind).map_err(|error| StorageError::validation(error.to_string()))?,
                key.overload_discriminator
            ],
            |row| {
                let entity_id: i64 = row.get(1)?;
                Ok(ShardLocation {
                    shard_id: row.get(0)?,
                    entity_id: entity_id as u64,
                    file_path: key.file_path.clone(),
                    name: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(query_error)
    }

    pub fn entity_location_by_stable_id(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        conn.query_row(
            "SELECT e.shard_id, e.entity_id, f.path, e.name\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2 AND e.stable_id = ?3\n                 ORDER BY e.id LIMIT 1",
            params![project_id, epoch, stable_id],
            |row| {
                let entity_id: i64 = row.get(1)?;
                Ok(ShardLocation {
                    shard_id: row.get(0)?,
                    entity_id: entity_id as u64,
                    file_path: row.get(2)?,
                    name: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(query_error)
    }

    pub fn name_shards(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT shard_id FROM relation_snapshot_entities\n                 WHERE project_id = ?1 AND relation_epoch = ?2 AND name = ?3 ORDER BY shard_id",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, name])
            .map_err(query_error)?;
        let mut shards = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            shards.push(row.get(0).map_err(query_error)?);
        }
        Ok(shards)
    }

    pub fn file_shard(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        conn.query_row(
            "SELECT shard_id FROM relation_snapshot_files\n                 WHERE project_id = ?1 AND relation_epoch = ?2 AND path = ?3",
            params![project_id, epoch, path],
            |row| row.get(0),
        )
        .optional()
        .map_err(query_error)
    }

    pub fn caller_shards(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT c.shard_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 JOIN relation_snapshot_entities t ON t.id = r.target_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2 AND t.entity_id = ?3\n                 ORDER BY c.shard_id",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, callee as i64])
            .map_err(query_error)?;
        let mut shards = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            shards.push(row.get(0).map_err(query_error)?);
        }
        Ok(shards)
    }

    pub fn classification_shards(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT c.shard_id\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2\n                   AND r.target_state = 'external' AND r.external_type_json = ?3\n                   AND c.scoped_name <> '<file>' ORDER BY c.shard_id",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![
                project_id,
                epoch,
                external_call_type_json(classification)
            ])
            .map_err(query_error)?;
        let mut shards = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            shards.push(row.get(0).map_err(query_error)?);
        }
        Ok(shards)
    }

    pub fn classification_counts(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<HashMap<ExternalCallType, usize>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT r.external_type_json, COUNT(*)\n                 FROM relation_snapshot_relations r\n                 JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                 WHERE r.project_id = ?1 AND r.relation_epoch = ?2\n                   AND r.target_state = 'external' AND r.external_type_json IS NOT NULL\n                   AND c.scoped_name <> '<file>'\n                 GROUP BY r.external_type_json",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch])
            .map_err(query_error)?;
        let mut counts = HashMap::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            let classification: ExternalCallType =
                from_json(&row.get::<_, String>(0).map_err(query_error)?)?;
            counts.insert(
                classification,
                row.get::<_, i64>(1).map_err(query_error)? as usize,
            );
        }
        Ok(counts)
    }

    pub fn directory_files(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT path FROM relation_snapshot_files\n                 WHERE project_id = ?1 AND relation_epoch = ?2 ORDER BY path",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch])
            .map_err(query_error)?;
        let mut paths = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            paths.push(row.get(0).map_err(query_error)?);
        }
        Ok(paths)
    }

    pub fn max_entity_id(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<u64, StorageError> {
        let value: Option<i64> = conn
            .query_row(
                "SELECT MAX(entity_id) FROM relation_snapshot_entities\n                 WHERE project_id = ?1 AND relation_epoch = ?2 AND entity_id IS NOT NULL",
                params![project_id, epoch],
                |row| row.get(0),
            )
            .optional()
            .map_err(query_error)?;
        Ok(value.unwrap_or(0) as u64)
    }

    pub fn stable_symbol_keys(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        let mut sql = String::from(
            "SELECT f.path, e.scoped_name, e.kind_json, e.overload_discriminator\n                 FROM relation_snapshot_entities e\n                 JOIN relation_snapshot_files f ON f.id = e.file_id\n                 WHERE e.project_id = ?1 AND e.relation_epoch = ?2",
        );
        if files.is_some() {
            sql.push_str(" AND f.path IN (SELECT value FROM json_each(?3))");
        }
        let mut statement = conn.prepare(&sql).map_err(query_error)?;
        let mut rows = match files {
            Some(files) => {
                let json = serde_json::to_string(&files.iter().collect::<Vec<_>>())
                    .map_err(|error| StorageError::validation(error.to_string()))?;
                statement
                    .query(params![project_id, epoch, json])
                    .map_err(query_error)?
            }
            None => statement
                .query(params![project_id, epoch])
                .map_err(query_error)?,
        };
        let mut keys = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            keys.push(StableSymbolKey {
                file_path: row.get(0).map_err(query_error)?,
                scoped_name: row.get(1).map_err(query_error)?,
                kind: from_json(&row.get::<_, String>(2).map_err(query_error)?)?,
                overload_discriminator: row.get(3).map_err(query_error)?,
            });
        }
        Ok(keys)
    }

    pub fn dependencies_of(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT target_path FROM relation_snapshot_dependencies d\n                 JOIN relation_snapshot_files f ON f.id = d.source_file_id\n                 WHERE d.project_id = ?1 AND d.relation_epoch = ?2 AND f.path = ?3\n                 ORDER BY target_path",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, source])
            .map_err(query_error)?;
        let mut targets = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            targets.push(row.get(0).map_err(query_error)?);
        }
        Ok(targets)
    }

    pub fn dependents_of(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT f.path FROM relation_snapshot_dependencies d\n                 JOIN relation_snapshot_files f ON f.id = d.source_file_id\n                 WHERE d.project_id = ?1 AND d.relation_epoch = ?2 AND d.target_path = ?3\n                 ORDER BY f.path",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch, file])
            .map_err(query_error)?;
        let mut sources = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            sources.push(row.get(0).map_err(query_error)?);
        }
        Ok(sources)
    }

    pub fn all_dependencies(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let mut statement = conn
            .prepare(
                "SELECT f.path, d.target_path\n                 FROM relation_snapshot_dependencies d\n                 JOIN relation_snapshot_files f ON f.id = d.source_file_id\n                 WHERE d.project_id = ?1 AND d.relation_epoch = ?2",
            )
            .map_err(query_error)?;
        let mut rows = statement
            .query(params![project_id, epoch])
            .map_err(query_error)?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().map_err(query_error)? {
            pairs.push((
                row.get(0).map_err(query_error)?,
                row.get(1).map_err(query_error)?,
            ));
        }
        Ok(pairs)
    }
}
