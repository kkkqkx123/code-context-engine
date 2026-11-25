//! Shard-directory and per-shard component reads for PostgreSQL.

use std::collections::{HashMap, HashSet};

use cce_types::relation::classification::ExternalCallType;
use cce_types::{
    CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
    CanonicalRelationTarget, RelationShardComponents, RelationShardDescriptor, ShardDirectory,
    ShardEntityRef, ShardLocation, StableSymbolKey, StorageError,
};

use crate::PostgresClient;
use crate::error::classify_pg;
use crate::json::{from_json, optional_from_json};

impl PostgresClient {
    pub(crate) async fn snapshot_shard_directory(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<ShardDirectory, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let mut directory = ShardDirectory::default();
            let rows = client
                .query(
                    "SELECT shard_id, file_count, entity_count, relation_count,\n                     dependency_count, estimated_bytes, path_start, path_end\n                     FROM relation_snapshot_shards\n                     WHERE project_id = $1 AND relation_epoch = $2 ORDER BY shard_id",
                    &[&project_id, &relation_epoch],
                )
                .await
                .map_err(classify_pg)?;
            for row in &rows {
                directory.descriptors.push(RelationShardDescriptor {
                    shard_id: row.try_get(0).map_err(classify_pg)?,
                    file_count: row.get::<_, i64>(1) as usize,
                    entity_count: row.get::<_, i64>(2) as usize,
                    relation_count: row.get::<_, i64>(3) as usize,
                    dependency_count: row.get::<_, i64>(4) as usize,
                    estimated_bytes: row.get::<_, i64>(5) as u64,
                    path_start: row.try_get(6).map_err(classify_pg)?,
                    path_end: row.try_get(7).map_err(classify_pg)?,
                });
            }
            Ok(directory)
        })
        .await
    }

    pub(crate) async fn snapshot_shard_components(
        &self,
        project_id: i64,
        relation_epoch: i64,
        shard_id: i64,
    ) -> Result<Option<RelationShardComponents>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let exists = client
                .query_opt(
                    "SELECT shard_id FROM relation_snapshot_shards\n                     WHERE project_id = $1 AND relation_epoch = $2 AND shard_id = $3",
                    &[&project_id, &relation_epoch, &shard_id],
                )
                .await
                .map_err(classify_pg)?;
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

            let rows = client
                .query(
                    "SELECT path, language, input_hash, file_size, imports_json\n                     FROM relation_snapshot_files\n                     WHERE project_id = $1 AND relation_epoch = $2 AND shard_id = $3 ORDER BY path",
                    &[&project_id, &relation_epoch, &shard_id],
                )
                .await
                .map_err(classify_pg)?;
            for row in &rows {
                components.files.push(CanonicalFile {
                    path: row.try_get(0).map_err(classify_pg)?,
                    language: row.try_get(1).map_err(classify_pg)?,
                    input_hash: row.try_get(2).map_err(classify_pg)?,
                    file_size: row.get::<_, i64>(3) as u64,
                    imports: from_json(&row.get::<_, String>(4))?,
                    exports: Vec::new(),
                });
            }

            let mut keys: HashMap<i64, StableSymbolKey> = HashMap::new();
            let rows = client
                .query(
                    &entity_query("e.shard_id = $3"),
                    &[&project_id, &relation_epoch, &shard_id],
                )
                .await
                .map_err(classify_pg)?;
            for row in &rows {
                let (id, key, entity) = read_entity_row(row)?;
                keys.insert(id, key.clone());
                if !key.is_file_placeholder() {
                    components.entities.push(entity);
                }
            }

            let needed: Vec<i64> = {
                let rows = client
                    .query(
                        "SELECT r.target_symbol_id\n                         FROM relation_snapshot_relations r\n                         JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                         WHERE r.project_id = $1 AND r.relation_epoch = $2 AND c.shard_id = $3\n                           AND r.target_symbol_id IS NOT NULL\n                         UNION\n                         SELECT e.parent_symbol_id\n                         FROM relation_snapshot_entities e\n                         WHERE e.project_id = $1 AND e.relation_epoch = $2 AND e.shard_id = $3\n                           AND e.parent_symbol_id IS NOT NULL
                         UNION
                         SELECT x.symbol_id
                         FROM relation_snapshot_exports x
                         JOIN relation_snapshot_files f ON f.id = x.file_id
                         WHERE x.project_id = $1 AND x.relation_epoch = $2 AND f.shard_id = $3",
                        &[&project_id, &relation_epoch, &shard_id],
                    )
                    .await
                    .map_err(classify_pg)?;
                let mut set = HashSet::new();
                for row in &rows {
                    set.insert(row.get::<_, i64>(0));
                }
                set.into_iter().collect()
            };
            if !needed.is_empty() {
                let rows = client
                    .query(
                        &entity_query("e.shard_id <> $3 AND e.id = ANY($4)"),
                        &[
                            &project_id,
                            &relation_epoch,
                            &shard_id,
                            &needed,
                        ],
                    )
                    .await
                    .map_err(classify_pg)?;
                for row in &rows {
                    let (id, key, _) = read_entity_row(row)?;
                    if let Some(entity_id) = row.get::<_, Option<i64>>(19) {
                        keys.insert(id, key.clone());
                        components.target_entities.push(ShardEntityRef {
                            key,
                            entity_id: entity_id as u64,
                            file_path: row.try_get(1).map_err(classify_pg)?,
                        });
                    }
                }
            }

            let rows = client
                .query(
                    "SELECT r.caller_symbol_id, r.target_symbol_id, r.target_state, r.raw_target,\n                     r.relation_type_json, r.span_json, r.external_type_json,\n                     r.unresolved_reason, r.stdlib_category_json,\n                     r.call_context_json, r.owner_type, r.overload_signature,\n                     r.callee_symbol_json, r.call_frequency, r.cfg_condition\n                     FROM relation_snapshot_relations r\n                     JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id\n                     WHERE r.project_id = $1 AND r.relation_epoch = $2 AND c.shard_id = $3\n                     ORDER BY r.id",
                    &[&project_id, &relation_epoch, &shard_id],
                )
                .await
                .map_err(classify_pg)?;
            for row in &rows {
                let caller_id: i64 = row.get(0);
                let target_id: Option<i64> = row.get(1);
                let state: String = row.try_get(2).map_err(classify_pg)?;
                let target = match state.as_str() {
                    "internal" => CanonicalRelationTarget::Internal {
                        key: keys
                            .get(&target_id.ok_or_else(|| {
                                StorageError::validation("internal relation has no target")
                            })?)
                            .ok_or_else(|| {
                                StorageError::validation("relation target entity is missing")
                            })?
                            .clone(),
                    },
                    "external" => CanonicalRelationTarget::External {
                        classification: optional_from_json(row.get::<_, Option<String>>(6))?,
                    },
                    "unresolved" => CanonicalRelationTarget::Unresolved {
                        reason: row
                            .get::<_, Option<String>>(7)
                            .ok_or_else(|| StorageError::validation("unresolved relation has no reason"))?
                            .parse()
                            .map_err(StorageError::Validation)?,
                    },
                    other => {
                        return Err(StorageError::validation(format!(
                            "invalid relation target state: {other}"
                        )));
                    }
                };
                let call_context = match row.get::<_, Option<String>>(9) {
                    Some(raw) => from_json(&raw)?,
                    None => cce_types::relation::CallContext::Direct,
                };
                components.relations.push(CanonicalRelation {
                    caller: keys
                        .get(&caller_id)
                        .ok_or_else(|| StorageError::validation("relation caller is missing"))?
                        .clone(),
                    target,
                    raw_target: row.try_get(3).map_err(classify_pg)?,
                    relation_type: from_json(&row.get::<_, String>(4))?,
                    span: from_json(&row.get::<_, String>(5))?,
                    stdlib_category: optional_from_json(row.get::<_, Option<String>>(8))?,
                    overload_signature: row.get(11),
                    callee_symbol: optional_from_json(row.get::<_, Option<String>>(12))?,
                    owner_type: row.get(10),
                    call_context,
                    call_frequency: row.get::<_, i64>(13) as u64,
                    cfg_condition: row.get(14),
                });
            }

            for file in &mut components.files {
                let rows = client
                    .query(
                        "SELECT e.scoped_name, e.kind_json, e.overload_discriminator,\n                         x.export_type, sf.path\n                         FROM relation_snapshot_exports x\n                         JOIN relation_snapshot_entities e ON e.id = x.symbol_id\n                         JOIN relation_snapshot_files sf ON sf.id = e.file_id\n                         WHERE x.project_id = $1 AND x.relation_epoch = $2 AND x.file_id = (\n                            SELECT id FROM relation_snapshot_files\n                            WHERE project_id = $1 AND relation_epoch = $2 AND path = $3\n                         )",
                        &[&project_id, &relation_epoch, &file.path],
                    )
                    .await
                    .map_err(classify_pg)?;
                for row in &rows {
                    file.exports.push(CanonicalExport {
                        symbol: StableSymbolKey {
                            file_path: row.try_get(4).map_err(classify_pg)?,
                            scoped_name: row.try_get(0).map_err(classify_pg)?,
                            kind: from_json(&row.get::<_, String>(1))?,
                            overload_discriminator: row.try_get(2).map_err(classify_pg)?,
                        },
                        export_type: row.try_get(3).map_err(classify_pg)?,
                    });
                }
            }

            let rows = client
                .query(
                    "SELECT sf.path, d.target_path, d.source\n                     FROM relation_snapshot_dependencies d\n                     JOIN relation_snapshot_files sf ON sf.id = d.source_file_id\n                     WHERE d.project_id = $1 AND d.relation_epoch = $2 AND sf.shard_id = $3",
                    &[&project_id, &relation_epoch, &shard_id],
                )
                .await
                .map_err(classify_pg)?;
            for row in &rows {
                components.dependencies.push(CanonicalDependency {
                    source_file: row.try_get(0).map_err(classify_pg)?,
                    target_file: row.try_get(1).map_err(classify_pg)?,
                    source: row.try_get(2).map_err(classify_pg)?,
                });
            }
            components.entities.sort_by_key(|entity| entity.key.sort_key());
            Ok(Some(components))
        })
        .await
    }
}

fn entity_query(predicate: &str) -> String {
    format!(
        "SELECT e.id, f.path, e.scoped_name, e.kind_json, e.overload_discriminator,\n         e.name, e.signature, e.parameters_json, e.return_type, e.span_json,\n         e.depth, e.parent_symbol_id, e.doc_comment, e.modifiers_json,\n         e.attributes_json, e.metadata_json, e.is_stdlib,\n         e.stdlib_category_json, e.subtype, e.entity_id\n         FROM relation_snapshot_entities e\n         JOIN relation_snapshot_files f ON f.id = e.file_id\n         WHERE e.project_id = $1 AND e.relation_epoch = $2 AND {predicate}\n         ORDER BY e.id"
    )
}

fn read_entity_row(
    row: &tokio_postgres::Row,
) -> Result<(i64, StableSymbolKey, CanonicalEntity), StorageError> {
    let key = StableSymbolKey {
        file_path: row.try_get(1).map_err(classify_pg)?,
        scoped_name: row.try_get(2).map_err(classify_pg)?,
        kind: from_json(&row.get::<_, String>(3))?,
        overload_discriminator: row.try_get(4).map_err(classify_pg)?,
    };
    let entity = CanonicalEntity {
        key: key.clone(),
        entity_id: row.get::<_, Option<i64>>(19).map(|id| id as u64),
        name: row.try_get(5).map_err(classify_pg)?,
        signature: row.try_get(6).map_err(classify_pg)?,
        parameters: from_json(&row.get::<_, String>(7))?,
        return_type: row.get(8),
        span: from_json(&row.get::<_, String>(9))?,
        depth: row.get::<_, i64>(10) as usize,
        parent: None,
        doc_comment: row.get(12),
        modifiers: from_json(&row.get::<_, String>(13))?,
        attributes: from_json(&row.get::<_, String>(14))?,
        metadata: from_json(&row.get::<_, String>(15))?,
        is_stdlib: row.get::<_, i64>(16) != 0,
        stdlib_category: optional_from_json(row.get::<_, Option<String>>(17))?,
        subtype: row.get(18),
    };
    Ok((row.get(0), key, entity))
}

impl PostgresClient {
    pub(crate) async fn shard_entity_location_by_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT e.shard_id, f.path, e.name FROM relation_snapshot_entities e
                     JOIN relation_snapshot_files f ON f.id = e.file_id
                     WHERE e.project_id = $1 AND e.relation_epoch = $2 AND e.entity_id = $3
                     ORDER BY e.id LIMIT 1",
                    &[&project_id, &epoch, &(entity_id as i64)],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.first().map(|row| ShardLocation {
                shard_id: row.get(0),
                entity_id,
                file_path: row.get(1),
                name: row.get(2),
            }))
        })
        .await
    }

    pub(crate) async fn shard_symbol_key_by_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
        entity_id: u64,
    ) -> Result<Option<StableSymbolKey>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT f.path, e.scoped_name, e.kind_json, e.overload_discriminator
                     FROM relation_snapshot_entities e
                     JOIN relation_snapshot_files f ON f.id = e.file_id
                     WHERE e.project_id = $1 AND e.relation_epoch = $2 AND e.entity_id = $3
                     ORDER BY e.id LIMIT 1",
                    &[&project_id, &epoch, &(entity_id as i64)],
                )
                .await
                .map_err(classify_pg)?;
            rows.first()
                .map(|row| {
                    Ok(StableSymbolKey {
                        file_path: row.try_get(0).map_err(classify_pg)?,
                        scoped_name: row.try_get(1).map_err(classify_pg)?,
                        kind: from_json(&row.get::<_, String>(2))?,
                        overload_discriminator: row.try_get(3).map_err(classify_pg)?,
                    })
                })
                .transpose()
        })
        .await
    }

    pub(crate) async fn shard_entity_location_by_symbol(
        &self,
        project_id: i64,
        epoch: i64,
        key: &StableSymbolKey,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        let kind = serde_json::to_string(&key.kind)
            .map_err(|error| StorageError::validation(error.to_string()))?;
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT e.shard_id, e.entity_id, e.name FROM relation_snapshot_entities e
                     JOIN relation_snapshot_files f ON f.id = e.file_id
                     WHERE e.project_id = $1 AND e.relation_epoch = $2 AND f.path = $3
                       AND e.scoped_name = $4 AND e.kind_json = $5
                       AND e.overload_discriminator = $6 AND e.entity_id IS NOT NULL
                     ORDER BY e.id LIMIT 1",
                    &[
                        &project_id,
                        &epoch,
                        &key.file_path,
                        &key.scoped_name,
                        &kind,
                        &key.overload_discriminator,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.first().map(|row| {
                let entity_id: i64 = row.get(1);
                ShardLocation {
                    shard_id: row.get(0),
                    entity_id: entity_id as u64,
                    file_path: key.file_path.clone(),
                    name: row.get(2),
                }
            }))
        })
        .await
    }

    pub(crate) async fn shard_entity_location_by_stable_id(
        &self,
        project_id: i64,
        epoch: i64,
        stable_id: &str,
    ) -> Result<Option<ShardLocation>, StorageError> {
        let client = self.clone();
        let stable_id = stable_id.to_string();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT e.shard_id, e.entity_id, f.path, e.name FROM relation_snapshot_entities e
                     JOIN relation_snapshot_files f ON f.id = e.file_id
                     WHERE e.project_id = $1 AND e.relation_epoch = $2 AND e.stable_id = $3
                     ORDER BY e.id LIMIT 1",
                    &[&project_id, &epoch, &stable_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.first().map(|row| {
                let entity_id: i64 = row.get(1);
                ShardLocation {
                    shard_id: row.get(0),
                    entity_id: entity_id as u64,
                    file_path: row.get(2),
                    name: row.get(3),
                }
            }))
        })
        .await
    }

    pub(crate) async fn shard_name_shards(
        &self,
        project_id: i64,
        epoch: i64,
        name: &str,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        let name = name.to_string();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT DISTINCT shard_id FROM relation_snapshot_entities
                     WHERE project_id = $1 AND relation_epoch = $2 AND name = $3 ORDER BY shard_id",
                    &[&project_id, &epoch, &name],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_file_shard(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Option<i64>, StorageError> {
        let client = self.clone();
        let path = path.to_string();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT shard_id FROM relation_snapshot_files
                     WHERE project_id = $1 AND relation_epoch = $2 AND path = $3",
                    &[&project_id, &epoch, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.first().map(|row| row.get(0)))
        })
        .await
    }

    pub(crate) async fn shard_caller_shards(
        &self,
        project_id: i64,
        epoch: i64,
        callee: u64,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT DISTINCT c.shard_id FROM relation_snapshot_relations r
                     JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id
                     JOIN relation_snapshot_entities t ON t.id = r.target_symbol_id
                     WHERE r.project_id = $1 AND r.relation_epoch = $2 AND t.entity_id = $3
                     ORDER BY c.shard_id",
                    &[&project_id, &epoch, &(callee as i64)],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_classification_shards(
        &self,
        project_id: i64,
        epoch: i64,
        classification: &ExternalCallType,
    ) -> Result<Vec<i64>, StorageError> {
        let client = self.clone();
        let json = cce_types::relation::snapshot_shard::external_call_type_json(classification);
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT DISTINCT c.shard_id FROM relation_snapshot_relations r
                     JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id
                     WHERE r.project_id = $1 AND r.relation_epoch = $2
                       AND r.target_state = 'external' AND r.external_type_json = $3
                       AND c.scoped_name <> '<file>' ORDER BY c.shard_id",
                    &[&project_id, &epoch, &json],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_classification_counts(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<HashMap<ExternalCallType, usize>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT r.external_type_json, COUNT(*) FROM relation_snapshot_relations r
                     JOIN relation_snapshot_entities c ON c.id = r.caller_symbol_id
                     WHERE r.project_id = $1 AND r.relation_epoch = $2
                       AND r.target_state = 'external' AND r.external_type_json IS NOT NULL
                       AND c.scoped_name <> '<file>' GROUP BY r.external_type_json",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            let mut counts = HashMap::new();
            for row in &rows {
                counts.insert(
                    from_json(&row.get::<_, String>(0))?,
                    row.get::<_, i64>(1) as usize,
                );
            }
            Ok(counts)
        })
        .await
    }

    pub(crate) async fn shard_directory_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT path FROM relation_snapshot_files
                     WHERE project_id = $1 AND relation_epoch = $2 ORDER BY path",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<u64, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT MAX(entity_id) FROM relation_snapshot_entities
                     WHERE project_id = $1 AND relation_epoch = $2 AND entity_id IS NOT NULL",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows
                .first()
                .and_then(|row| row.get::<_, Option<i64>>(0))
                .unwrap_or(0) as u64)
        })
        .await
    }

    pub(crate) async fn shard_stable_symbol_keys(
        &self,
        project_id: i64,
        epoch: i64,
        files: Option<&HashSet<String>>,
    ) -> Result<Vec<StableSymbolKey>, StorageError> {
        let client = self.clone();
        let files: Option<Vec<String>> = files.map(|files| files.iter().cloned().collect());
        self.run(async move {
            let client = client.pooled().await?;
            let rows = if let Some(files) = files {
                client
                    .query(
                        "SELECT f.path, e.scoped_name, e.kind_json, e.overload_discriminator
                         FROM relation_snapshot_entities e
                         JOIN relation_snapshot_files f ON f.id = e.file_id
                         WHERE e.project_id = $1 AND e.relation_epoch = $2 AND f.path = ANY($3)",
                        &[&project_id, &epoch, &files],
                    )
                    .await
                    .map_err(classify_pg)?
            } else {
                client
                    .query(
                        "SELECT f.path, e.scoped_name, e.kind_json, e.overload_discriminator
                         FROM relation_snapshot_entities e
                         JOIN relation_snapshot_files f ON f.id = e.file_id
                         WHERE e.project_id = $1 AND e.relation_epoch = $2",
                        &[&project_id, &epoch],
                    )
                    .await
                    .map_err(classify_pg)?
            };
            let mut keys = Vec::new();
            for row in &rows {
                keys.push(StableSymbolKey {
                    file_path: row.try_get(0).map_err(classify_pg)?,
                    scoped_name: row.try_get(1).map_err(classify_pg)?,
                    kind: from_json(&row.get::<_, String>(2))?,
                    overload_discriminator: row.try_get(3).map_err(classify_pg)?,
                });
            }
            Ok(keys)
        })
        .await
    }

    pub(crate) async fn shard_dependencies_of(
        &self,
        project_id: i64,
        epoch: i64,
        source: &str,
    ) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        let source = source.to_string();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT target_path FROM relation_snapshot_dependencies d
                     JOIN relation_snapshot_files f ON f.id = d.source_file_id
                     WHERE d.project_id = $1 AND d.relation_epoch = $2 AND f.path = $3
                     ORDER BY target_path",
                    &[&project_id, &epoch, &source],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_dependents_of(
        &self,
        project_id: i64,
        epoch: i64,
        file: &str,
    ) -> Result<Vec<String>, StorageError> {
        let client = self.clone();
        let file = file.to_string();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT f.path FROM relation_snapshot_dependencies d
                     JOIN relation_snapshot_files f ON f.id = d.source_file_id
                     WHERE d.project_id = $1 AND d.relation_epoch = $2 AND d.target_path = $3
                     ORDER BY f.path",
                    &[&project_id, &epoch, &file],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
        .await
    }

    pub(crate) async fn shard_all_dependencies(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let client = self.clone();
        self.run(async move {
            let client = client.pooled().await?;
            let rows = client
                .query(
                    "SELECT f.path, d.target_path FROM relation_snapshot_dependencies d
                     JOIN relation_snapshot_files f ON f.id = d.source_file_id
                     WHERE d.project_id = $1 AND d.relation_epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.iter().map(|row| (row.get(0), row.get(1))).collect())
        })
        .await
    }
}
