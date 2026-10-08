use std::collections::HashMap;

use cce_types::StorageError;
use cce_types::{CanonicalRelationSnapshot, RelationSnapshotManifest, SnapshotDelta};

use crate::error::classify_pg;
use crate::json::{from_json, optional_from_json, optional_json, to_json};

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn snapshot_allocate(
        &self,
        project_id: i64,
        operation_id: &str,
        config_fingerprint: &str,
    ) -> Result<i64, StorageError> {
        let operation_id = operation_id.to_string();
        let config_fingerprint = config_fingerprint.to_string();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let epoch =
                snapshot_allocate_in(&tx, project_id, &operation_id, &config_fingerprint).await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(epoch)
        })
        .await
    }

    pub(crate) async fn snapshot_write_ready(
        &self,
        project_id: i64,
        epoch: i64,
        snapshot: &CanonicalRelationSnapshot,
        input_fingerprint: &str,
        snapshot_fingerprint: &str,
    ) -> Result<(), StorageError> {
        let snapshot = snapshot.clone();
        let input_fingerprint = input_fingerprint.to_string();
        let snapshot_fingerprint = snapshot_fingerprint.to_string();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            snapshot_write_ready_in(
                &tx,
                project_id,
                epoch,
                &snapshot,
                &input_fingerprint,
                &snapshot_fingerprint,
            )
            .await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn snapshot_read(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<CanonicalRelationSnapshot, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let manifest = snapshot_manifest_in(&client, project_id, epoch)
                .await?
                .ok_or_else(|| StorageError::not_found(format!("relation snapshot {epoch}")))?;
            snapshot_read_in(&client, &manifest).await
        })
        .await
    }

    pub(crate) async fn snapshot_manifest(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            snapshot_manifest_in(&client, project_id, epoch).await
        })
        .await
    }

    pub(crate) async fn snapshot_delta_chain(
        &self,
        project_id: i64,
        after_epoch: i64,
        up_to_epoch: i64,
    ) -> Result<Vec<SnapshotDelta>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT delta_data FROM relation_snapshot_deltas \
                     WHERE project_id = $1 AND delta_epoch > $2 AND delta_epoch <= $3 \
                     ORDER BY delta_epoch ASC",
                    &[&project_id, &after_epoch, &up_to_epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(decode_delta_row).collect()
        })
        .await
    }

    pub(crate) async fn snapshot_find_base(
        &self,
        project_id: i64,
        delta_epoch: i64,
    ) -> Result<Option<i64>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let mut current = delta_epoch;
            loop {
                let manifest = snapshot_manifest_in(&client, project_id, current).await?;
                match manifest {
                    Some(manifest)
                        if manifest.state == cce_types::relation::RelationSnapshotState::Active =>
                    {
                        return Ok(Some(current));
                    }
                    Some(manifest)
                        if manifest.state == cce_types::relation::RelationSnapshotState::Delta =>
                    {
                        let base: Option<i64> = client
                            .query_opt(
                                "SELECT base_epoch FROM relation_snapshot_deltas \
                                 WHERE project_id = $1 AND delta_epoch = $2",
                                &[&project_id, &current],
                            )
                            .await
                            .map_err(classify_pg)?
                            .map(|row| row.get(0));
                        match base {
                            Some(base_epoch) => {
                                if base_epoch == current || base_epoch < 0 {
                                    return Err(StorageError::validation(format!(
                                        "delta chain broken at epoch {current}: \
                                         invalid base_epoch {base_epoch}"
                                    )));
                                }
                                current = base_epoch;
                            }
                            None => {
                                return Err(StorageError::validation(format!(
                                    "delta chain broken at epoch {current}: \
                                     missing base_epoch reference"
                                )));
                            }
                        }
                    }
                    Some(_) => {
                        return Ok(Some(current));
                    }
                    None => {
                        return Ok(None);
                    }
                }
            }
        })
        .await
    }

    pub(crate) async fn snapshot_mark_failed(
        &self,
        project_id: i64,
        epoch: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        let reason = reason.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            client
                .execute(
                    "UPDATE relation_snapshot_manifest SET state = 'failed', failure_reason = $3 \
                     WHERE project_id = $1 AND relation_epoch = $2 AND state <> 'active'",
                    &[&project_id, &epoch, &reason],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn snapshot_delete_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let removed = snapshot_delete_epoch_in(&tx, project_id, epoch).await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn snapshot_delete_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut removed = 0u64;
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
                        &format!("DELETE FROM {table} WHERE project_id = $1"),
                        &[&project_id],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            removed += tx
                .execute(
                    "DELETE FROM relation_snapshot_manifest WHERE project_id = $1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn db_size(&self) -> Result<u64, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let size: i64 = client
                .query_one("SELECT pg_database_size(current_database())", &[])
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(size.max(0) as u64)
        })
        .await
    }

    pub(crate) async fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut removed = 0u64;
            for table in [
                "relation_snapshot_deltas",
                "relation_snapshot_dependencies",
                "relation_snapshot_exports",
                "relation_snapshot_relations",
                "relation_snapshot_entities",
                "relation_snapshot_files",
                "relation_snapshot_manifest",
            ] {
                removed += tx
                    .execute(
                        &format!("DELETE FROM {table} WHERE project_id = $1"),
                        &[&project_id],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            for (table, scope) in [
                ("files", "project_id"),
                ("entities", "project_id"),
                ("chunks", "project_id"),
                ("entity_detail_mappings", "project_id"),
                ("checkpoint", "project_id"),
                ("checkpoint_batch", "project_id"),
                ("checkpoint_file", "project_id"),
                ("work_unit_checkpoint", "project_id"),
                ("index_state_projection", "project_id"),
                ("generation_overrides", "project_id"),
                ("project_index_manifests", "project_id"),
                ("project_meta", "project_id"),
            ] {
                removed += tx
                    .execute(
                        &format!("DELETE FROM {table} WHERE {scope} = $1"),
                        &[&project_id],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            removed += tx
                .execute(
                    "DELETE FROM file_summaries WHERE file_id IN \
                     (SELECT id FROM files WHERE project_id = $1)",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            removed += tx
                .execute("DELETE FROM projects WHERE id = $1", &[&project_id])
                .await
                .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }
}

const SNAPSHOT_MANIFEST_COLUMNS: &str = "project_id, relation_epoch, operation_id, state, \
     schema_version, parser_version, resolver_version, path_normalization_version, \
     config_fingerprint, input_fingerprint, snapshot_fingerprint, file_count, entity_count, \
     relation_count, dependency_count, failure_reason, symbol_key_conflict_count, \
     symbol_key_conflict_samples_json";

fn snapshot_manifest_from_row(
    row: &tokio_postgres::Row,
) -> Result<RelationSnapshotManifest, StorageError> {
    let state: String = row.try_get("state").map_err(classify_pg)?;
    let samples: Option<String> = row
        .try_get("symbol_key_conflict_samples_json")
        .map_err(classify_pg)?;
    let symbol_key_conflict_samples = match samples {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => Vec::new(),
    };
    let schema_version: i64 = row.try_get("schema_version").map_err(classify_pg)?;
    let parser_version: i64 = row.try_get("parser_version").map_err(classify_pg)?;
    let resolver_version: i64 = row.try_get("resolver_version").map_err(classify_pg)?;
    let path_normalization_version: i64 = row
        .try_get("path_normalization_version")
        .map_err(classify_pg)?;
    let file_count: Option<i64> = row.try_get("file_count").map_err(classify_pg)?;
    let entity_count: Option<i64> = row.try_get("entity_count").map_err(classify_pg)?;
    let relation_count: Option<i64> = row.try_get("relation_count").map_err(classify_pg)?;
    let dependency_count: Option<i64> = row.try_get("dependency_count").map_err(classify_pg)?;
    let symbol_key_conflict_count: i64 = row
        .try_get("symbol_key_conflict_count")
        .map_err(classify_pg)?;
    Ok(RelationSnapshotManifest {
        project_id: row.try_get("project_id").map_err(classify_pg)?,
        relation_epoch: row.try_get("relation_epoch").map_err(classify_pg)?,
        operation_id: row.try_get("operation_id").map_err(classify_pg)?,
        state: cce_types::relation::RelationSnapshotState::parse(&state)?,
        schema_version: schema_version as u32,
        parser_version: parser_version as u32,
        resolver_version: resolver_version as u32,
        path_normalization_version: path_normalization_version as u32,
        config_fingerprint: row.try_get("config_fingerprint").map_err(classify_pg)?,
        input_fingerprint: row.try_get("input_fingerprint").map_err(classify_pg)?,
        snapshot_fingerprint: row.try_get("snapshot_fingerprint").map_err(classify_pg)?,
        file_count: file_count.map(|value| value as usize),
        entity_count: entity_count.map(|value| value as usize),
        relation_count: relation_count.map(|value| value as usize),
        dependency_count: dependency_count.map(|value| value as usize),
        failure_reason: row.try_get("failure_reason").map_err(classify_pg)?,
        symbol_key_conflict_count: symbol_key_conflict_count as u64,
        symbol_key_conflict_samples,
    })
}

async fn snapshot_manifest_in(
    client: &deadpool_postgres::Client,
    project_id: i64,
    epoch: i64,
) -> Result<Option<RelationSnapshotManifest>, StorageError> {
    let row = client
        .query_opt(
            &format!(
                "SELECT {SNAPSHOT_MANIFEST_COLUMNS} FROM relation_snapshot_manifest \
                 WHERE project_id = $1 AND relation_epoch = $2"
            ),
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?;
    row.map(|row| snapshot_manifest_from_row(&row)).transpose()
}

fn decode_delta_row(row: &tokio_postgres::Row) -> Result<SnapshotDelta, StorageError> {
    let compressed: Vec<u8> = row.try_get("delta_data").map_err(classify_pg)?;
    let decompressed = zstd::decode_all(&*compressed)
        .map_err(|error| StorageError::validation(format!("delta decompression: {error}")))?;
    serde_json::from_slice(&decompressed)
        .map_err(|error| StorageError::validation(format!("delta deserialization: {error}")))
}

async fn snapshot_allocate_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    operation_id: &str,
    config_fingerprint: &str,
) -> Result<i64, StorageError> {
    let existing: Option<(i64, String)> = tx
        .query_opt(
            "SELECT relation_epoch, state FROM relation_snapshot_manifest \
             WHERE project_id = $1 AND operation_id = $2",
            &[&project_id, &operation_id],
        )
        .await
        .map_err(classify_pg)?
        .map(|row| (row.get(0), row.get(1)));
    if let Some((epoch, state)) = existing {
        if state == "failed" {
            tx.execute(
                "UPDATE relation_snapshot_manifest SET state = 'building', \
                 input_fingerprint = NULL, snapshot_fingerprint = NULL, file_count = NULL, \
                 entity_count = NULL, relation_count = NULL, dependency_count = NULL, \
                 validated_at = NULL, activated_at = NULL, failure_reason = NULL, \
                 symbol_key_conflict_count = 0, symbol_key_conflict_samples_json = NULL \
                 WHERE project_id = $1 AND relation_epoch = $2",
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?;
        }
        return Ok(epoch);
    }
    let epoch: i64 = tx
        .query_one(
            "SELECT GREATEST(
                COALESCE((SELECT MAX(relation_epoch) FROM relation_snapshot_manifest \
                          WHERE project_id = $1), 0),
                COALESCE((SELECT MAX(value::BIGINT) FROM project_meta \
                          WHERE project_id = $1 AND key = 'active_relation_epoch'), 0)
             ) + 1",
            &[&project_id],
        )
        .await
        .map_err(classify_pg)?
        .get(0);
    let now = chrono::Utc::now().timestamp();
    tx.execute(
        "INSERT INTO relation_snapshot_manifest (project_id, relation_epoch, operation_id, \
         state, schema_version, parser_version, resolver_version, path_normalization_version, \
         config_fingerprint, created_at) \
         VALUES ($1, $2, $3, 'building', $4, $5, $6, $7, $8, $9)",
        &[
            &project_id,
            &epoch,
            &operation_id,
            &(cce_types::RELATION_SNAPSHOT_SCHEMA_VERSION as i64),
            &(cce_types::RELATION_PARSER_VERSION as i64),
            &(cce_types::RELATION_RESOLVER_VERSION as i64),
            &(cce_types::RELATION_PATH_NORMALIZATION_VERSION as i64),
            &config_fingerprint,
            &now,
        ],
    )
    .await
    .map_err(classify_pg)?;
    Ok(epoch)
}

pub(crate) async fn snapshot_activate_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    now: i64,
) -> Result<(), StorageError> {
    let state: Option<String> = tx
        .query_opt(
            "SELECT state FROM relation_snapshot_manifest \
             WHERE project_id = $1 AND relation_epoch = $2",
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?
        .map(|row| row.get(0));
    match state.as_deref() {
        Some("active") => return Ok(()),
        Some("delta") => {}
        Some("ready") => {
            let changed = tx
                .execute(
                    "UPDATE relation_snapshot_manifest SET state = 'active', activated_at = $3 \
                     WHERE project_id = $1 AND relation_epoch = $2 AND state = 'ready'",
                    &[&project_id, &epoch, &now],
                )
                .await
                .map_err(classify_pg)?;
            if changed != 1 {
                return Err(StorageError::transaction(format!(
                    "epoch {epoch} is not ready"
                )));
            }
            tx.execute(
                "UPDATE relation_snapshot_manifest SET state = 'ready' \
                 WHERE project_id = $1 AND relation_epoch <> $2 AND state = 'active'",
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?;
        }
        _ => {
            return Err(StorageError::transaction(format!(
                "epoch {epoch} is not activatable (state: {state:?})"
            )));
        }
    }
    tx.execute(
        "INSERT INTO project_meta (project_id, key, value, created_at, updated_at) \
         VALUES ($1, 'active_relation_epoch', $2, $3, $3) \
         ON CONFLICT (project_id, key) DO UPDATE SET \
            value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
        &[&project_id, &epoch.to_string(), &now],
    )
    .await
    .map_err(classify_pg)?;
    Ok(())
}

async fn snapshot_write_ready_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    snapshot: &CanonicalRelationSnapshot,
    input_fingerprint: &str,
    snapshot_fingerprint: &str,
) -> Result<(), StorageError> {
    use cce_types::{CanonicalRelationTarget, StableSymbolKey};
    use std::collections::BTreeMap;

    let state: Option<String> = tx
        .query_opt(
            "SELECT state FROM relation_snapshot_manifest \
             WHERE project_id = $1 AND relation_epoch = $2",
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?
        .map(|row| row.get(0));
    if state.as_deref() != Some("building") {
        return Err(StorageError::transaction(format!(
            "epoch {epoch} is {state:?}, expected building"
        )));
    }

    let mut file_ids: HashMap<String, i64> = HashMap::with_capacity(snapshot.files.len());
    for file in &snapshot.files {
        let file_size = file.file_size as i64;
        let imports_json = to_json(&file.imports)?;
        let id: i64 = tx
            .query_one(
                "INSERT INTO relation_snapshot_files (project_id, relation_epoch, path, \
                 language, input_hash, file_size, imports_json) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
                &[
                    &project_id,
                    &epoch,
                    &file.path,
                    &file.language,
                    &file.input_hash,
                    &file_size,
                    &imports_json,
                ],
            )
            .await
            .map_err(classify_pg)?
            .get(0);
        file_ids.insert(file.path.clone(), id);
    }

    let mut storage_entities: Vec<cce_types::CanonicalEntity> = snapshot.entities.clone();
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
            storage_entities.push(cce_types::CanonicalEntity {
                key,
                entity_id: None,
                name: "<file>".to_string(),
                signature: String::new(),
                parameters: Vec::new(),
                return_type: None,
                span: Default::default(),
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
    let mut symbol_ids: HashMap<StableSymbolKey, i64> =
        HashMap::with_capacity(storage_entities.len());
    for entity in &storage_entities {
        let file_id = required(&file_ids, &entity.key.file_path, "entity file")?;
        let kind_json = to_json(&entity.key.kind)?;
        let parameters_json = to_json(&entity.parameters)?;
        let span_json = to_json(&entity.span)?;
        let modifiers_json = to_json(&entity.modifiers)?;
        let attributes_json = to_json(&entity.attributes)?;
        let metadata_json = to_json(&entity.metadata)?;
        let stdlib_category_json = optional_json(&entity.stdlib_category)?;
        let entity_id: Option<i64> = entity.entity_id.map(|id| id as i64);
        let depth = entity.depth as i64;
        let is_stdlib: i64 = i64::from(entity.is_stdlib);
        let id: i64 = tx
            .query_one(
                "INSERT INTO relation_snapshot_entities (project_id, relation_epoch, file_id, \
                 scoped_name, kind_json, overload_discriminator, entity_id, name, signature, \
                 parameters_json, return_type, span_json, depth, doc_comment, modifiers_json, \
                 attributes_json, metadata_json, is_stdlib, stdlib_category_json, subtype) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
                         $11, $12, $13, $14, $15, $16, $17, $18, $19, $20) RETURNING id",
                &[
                    &project_id,
                    &epoch,
                    file_id,
                    &entity.key.scoped_name,
                    &kind_json,
                    &entity.key.overload_discriminator,
                    &entity_id,
                    &entity.name,
                    &entity.signature,
                    &parameters_json,
                    &entity.return_type,
                    &span_json,
                    &depth,
                    &entity.doc_comment,
                    &modifiers_json,
                    &attributes_json,
                    &metadata_json,
                    &is_stdlib,
                    &stdlib_category_json,
                    &entity.subtype,
                ],
            )
            .await
            .map_err(classify_pg)?
            .get(0);
        symbol_ids.insert(entity.key.clone(), id);
    }
    for entity in &snapshot.entities {
        if let Some(parent) = &entity.parent {
            let entity_id = required(&symbol_ids, &entity.key, "entity")?;
            let parent_id = required(&symbol_ids, parent, "parent entity")?;
            tx.execute(
                "UPDATE relation_snapshot_entities SET parent_symbol_id = $3 \
                 WHERE project_id = $1 AND id = $2",
                &[&project_id, entity_id, parent_id],
            )
            .await
            .map_err(classify_pg)?;
        }
    }

    for relation in &snapshot.relations {
        let caller_id = required(&symbol_ids, &relation.caller, "relation caller")?;
        let (target_id, target_state, external_json, unresolved_reason) = match &relation.target {
            CanonicalRelationTarget::Internal { key } => (
                Some(*required(&symbol_ids, key, "relation target")?),
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
        let call_frequency = relation.call_frequency as i64;
        tx.execute(
            "INSERT INTO relation_snapshot_relations (project_id, relation_epoch, \
             caller_symbol_id, target_symbol_id, target_state, raw_target, relation_type_json, \
             span_json, external_type_json, unresolved_reason, stdlib_category_json, \
             call_context_json, owner_type, overload_signature, callee_symbol_json, \
             call_frequency, cfg_condition) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
            &[
                &project_id,
                &epoch,
                caller_id,
                &target_id,
                &target_state,
                &relation.raw_target,
                &relation_type_json,
                &span_json,
                &external_json,
                &unresolved_reason,
                &stdlib_category_json,
                &call_context_json,
                &relation.owner_type,
                &relation.overload_signature,
                &callee_symbol_json,
                &call_frequency,
                &relation.cfg_condition,
            ],
        )
        .await
        .map_err(classify_pg)?;
    }

    for file in &snapshot.files {
        let file_id = required(&file_ids, &file.path, "export file")?;
        for export in &file.exports {
            let symbol_id = required(&symbol_ids, &export.symbol, "export symbol")?;
            tx.execute(
                "INSERT INTO relation_snapshot_exports (project_id, relation_epoch, file_id, \
                 symbol_id, export_type) VALUES ($1, $2, $3, $4, $5)",
                &[&project_id, &epoch, file_id, symbol_id, &export.export_type],
            )
            .await
            .map_err(classify_pg)?;
        }
    }

    for dependency in &snapshot.dependencies {
        let source_id = required(&file_ids, &dependency.source_file, "dependency source")?;
        tx.execute(
            "INSERT INTO relation_snapshot_dependencies (project_id, relation_epoch, \
             source_file_id, target_path, source) VALUES ($1, $2, $3, $4, $5)",
            &[
                &project_id,
                &epoch,
                source_id,
                &dependency.target_file,
                &dependency.source,
            ],
        )
        .await
        .map_err(classify_pg)?;
    }

    let conflict_samples_json = to_json(&snapshot.build_metadata.symbol_key_conflict_samples)?;
    let changed = tx
        .execute(
            "UPDATE relation_snapshot_manifest SET state = 'ready', input_fingerprint = $3, \
             snapshot_fingerprint = $4, file_count = $5, entity_count = $6, relation_count = $7, \
             dependency_count = $8, validated_at = $9, failure_reason = NULL, \
             symbol_key_conflict_count = $10, symbol_key_conflict_samples_json = $11 \
             WHERE project_id = $1 AND relation_epoch = $2 AND state = 'building'",
            &[
                &project_id,
                &epoch,
                &input_fingerprint,
                &snapshot_fingerprint,
                &(snapshot.files.len() as i64),
                &(snapshot.entities.len() as i64),
                &(snapshot.relations.len() as i64),
                &(snapshot.dependencies.len() as i64),
                &chrono::Utc::now().timestamp(),
                &(snapshot.build_metadata.symbol_key_conflict_count as i64),
                &conflict_samples_json,
            ],
        )
        .await
        .map_err(classify_pg)?;
    if changed != 1 {
        return Err(StorageError::transaction(format!(
            "epoch {epoch} did not transition to ready"
        )));
    }
    Ok(())
}

async fn snapshot_read_in(
    client: &deadpool_postgres::Client,
    manifest: &RelationSnapshotManifest,
) -> Result<CanonicalRelationSnapshot, StorageError> {
    use cce_types::{
        CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile, CanonicalRelation,
        CanonicalRelationTarget, StableSymbolKey,
    };

    let mut snapshot = CanonicalRelationSnapshot::new(manifest.config_fingerprint.clone());
    snapshot.schema_version = manifest.schema_version;
    snapshot.parser_version = manifest.parser_version;
    snapshot.resolver_version = manifest.resolver_version;
    snapshot.path_normalization_version = manifest.path_normalization_version;

    let file_rows = client
        .query(
            "SELECT id, path, language, input_hash, file_size, imports_json \
             FROM relation_snapshot_files \
             WHERE project_id = $1 AND relation_epoch = $2 ORDER BY path",
            &[&manifest.project_id, &manifest.relation_epoch],
        )
        .await
        .map_err(classify_pg)?;
    let mut file_indexes: HashMap<i64, usize> = HashMap::new();
    for row in &file_rows {
        let id: i64 = row.try_get("id").map_err(classify_pg)?;
        let file_size: i64 = row.try_get("file_size").map_err(classify_pg)?;
        let imports_json: String = row.try_get("imports_json").map_err(classify_pg)?;
        file_indexes.insert(id, snapshot.files.len());
        snapshot.files.push(CanonicalFile {
            path: row.try_get("path").map_err(classify_pg)?,
            language: row.try_get("language").map_err(classify_pg)?,
            input_hash: row.try_get("input_hash").map_err(classify_pg)?,
            file_size: file_size as u64,
            imports: from_json(&imports_json)?,
            exports: Vec::new(),
        });
    }

    let entity_rows = client
        .query(
            "SELECT e.id, f.path, e.scoped_name, e.kind_json, e.overload_discriminator, e.name, \
             e.signature, e.parameters_json, e.return_type, e.span_json, e.depth, \
             e.parent_symbol_id, e.doc_comment, e.modifiers_json, e.attributes_json, \
             e.metadata_json, e.is_stdlib, e.stdlib_category_json, e.subtype, e.entity_id \
             FROM relation_snapshot_entities e \
             JOIN relation_snapshot_files f ON f.id = e.file_id \
             WHERE e.project_id = $1 AND e.relation_epoch = $2 ORDER BY e.id",
            &[&manifest.project_id, &manifest.relation_epoch],
        )
        .await
        .map_err(classify_pg)?;
    let mut entity_ids: HashMap<i64, StableSymbolKey> = HashMap::new();
    let mut parents: Vec<Option<i64>> = Vec::new();
    for row in &entity_rows {
        let id: i64 = row.try_get("id").map_err(classify_pg)?;
        let kind_json: String = row.try_get("kind_json").map_err(classify_pg)?;
        let key = StableSymbolKey {
            file_path: row.try_get("path").map_err(classify_pg)?,
            scoped_name: row.try_get("scoped_name").map_err(classify_pg)?,
            kind: from_json(&kind_json)?,
            overload_discriminator: row.try_get("overload_discriminator").map_err(classify_pg)?,
        };
        let persisted_entity_id: Option<i64> = row.try_get("entity_id").map_err(classify_pg)?;
        let parent_symbol_id: Option<i64> = row.try_get("parent_symbol_id").map_err(classify_pg)?;
        entity_ids.insert(id, key.clone());
        if key.is_file_placeholder() {
            continue;
        }
        parents.push(parent_symbol_id);
        let parameters_json: String = row.try_get("parameters_json").map_err(classify_pg)?;
        let span_json: String = row.try_get("span_json").map_err(classify_pg)?;
        let modifiers_json: String = row.try_get("modifiers_json").map_err(classify_pg)?;
        let attributes_json: String = row.try_get("attributes_json").map_err(classify_pg)?;
        let metadata_json: String = row.try_get("metadata_json").map_err(classify_pg)?;
        let stdlib_category_json: Option<String> =
            row.try_get("stdlib_category_json").map_err(classify_pg)?;
        let depth: i64 = row.try_get("depth").map_err(classify_pg)?;
        let is_stdlib: i64 = row.try_get("is_stdlib").map_err(classify_pg)?;
        snapshot.entities.push(CanonicalEntity {
            key,
            entity_id: persisted_entity_id.map(|id| id as u64),
            name: row.try_get("name").map_err(classify_pg)?,
            signature: row.try_get("signature").map_err(classify_pg)?,
            parameters: from_json(&parameters_json)?,
            return_type: row.try_get("return_type").map_err(classify_pg)?,
            span: from_json(&span_json)?,
            depth: depth as usize,
            parent: None,
            doc_comment: row.try_get("doc_comment").map_err(classify_pg)?,
            modifiers: from_json(&modifiers_json)?,
            attributes: from_json(&attributes_json)?,
            metadata: from_json(&metadata_json)?,
            is_stdlib: is_stdlib != 0,
            stdlib_category: optional_from_json(stdlib_category_json)?,
            subtype: row.try_get("subtype").map_err(classify_pg)?,
        });
    }
    for (entity, parent_id) in snapshot.entities.iter_mut().zip(parents) {
        entity.parent = parent_id
            .map(|id| required(&entity_ids, &id, "parent symbol").cloned())
            .transpose()?;
    }

    let relation_rows = client
        .query(
            "SELECT caller_symbol_id, target_symbol_id, target_state, raw_target, \
             relation_type_json, span_json, external_type_json, unresolved_reason, \
             stdlib_category_json, call_context_json, owner_type, overload_signature, \
             callee_symbol_json, call_frequency, cfg_condition \
             FROM relation_snapshot_relations \
             WHERE project_id = $1 AND relation_epoch = $2 ORDER BY id",
            &[&manifest.project_id, &manifest.relation_epoch],
        )
        .await
        .map_err(classify_pg)?;
    for row in &relation_rows {
        let caller_id: i64 = row.try_get("caller_symbol_id").map_err(classify_pg)?;
        let target_id: Option<i64> = row.try_get("target_symbol_id").map_err(classify_pg)?;
        let state: String = row.try_get("target_state").map_err(classify_pg)?;
        let target = match state.as_str() {
            "internal" => CanonicalRelationTarget::Internal {
                key: required(
                    &entity_ids,
                    &target_id.ok_or_else(|| {
                        StorageError::validation("internal relation has no target".to_string())
                    })?,
                    "relation target",
                )?
                .clone(),
            },
            "external" => CanonicalRelationTarget::External {
                classification: optional_from_json(
                    row.try_get("external_type_json").map_err(classify_pg)?,
                )?,
            },
            "unresolved" => CanonicalRelationTarget::Unresolved {
                reason: row
                    .try_get::<_, Option<String>>("unresolved_reason")
                    .map_err(classify_pg)?
                    .ok_or_else(|| {
                        StorageError::validation("unresolved relation has no reason".to_string())
                    })?
                    .parse()
                    .map_err(StorageError::validation)?,
            },
            _ => {
                return Err(StorageError::validation(format!(
                    "invalid relation target state: {state}"
                )));
            }
        };
        let call_context_json: Option<String> =
            row.try_get("call_context_json").map_err(classify_pg)?;
        let call_context = match call_context_json {
            Some(raw) => from_json(&raw)?,
            None => cce_types::relation::CallContext::Direct,
        };
        let relation_type_json: String = row.try_get("relation_type_json").map_err(classify_pg)?;
        let span_json: String = row.try_get("span_json").map_err(classify_pg)?;
        let call_frequency: i64 = row.try_get("call_frequency").map_err(classify_pg)?;
        snapshot.relations.push(CanonicalRelation {
            caller: required(&entity_ids, &caller_id, "relation caller")?.clone(),
            target,
            raw_target: row.try_get("raw_target").map_err(classify_pg)?,
            relation_type: from_json(&relation_type_json)?,
            span: from_json(&span_json)?,
            stdlib_category: optional_from_json(
                row.try_get("stdlib_category_json").map_err(classify_pg)?,
            )?,
            overload_signature: row.try_get("overload_signature").map_err(classify_pg)?,
            callee_symbol: optional_from_json(
                row.try_get("callee_symbol_json").map_err(classify_pg)?,
            )?,
            owner_type: row.try_get("owner_type").map_err(classify_pg)?,
            call_context,
            call_frequency: call_frequency as u64,
            cfg_condition: row.try_get("cfg_condition").map_err(classify_pg)?,
        });
    }

    let export_rows = client
        .query(
            "SELECT file_id, symbol_id, export_type FROM relation_snapshot_exports \
             WHERE project_id = $1 AND relation_epoch = $2",
            &[&manifest.project_id, &manifest.relation_epoch],
        )
        .await
        .map_err(classify_pg)?;
    for row in &export_rows {
        let file_id: i64 = row.try_get("file_id").map_err(classify_pg)?;
        let symbol_id: i64 = row.try_get("symbol_id").map_err(classify_pg)?;
        let file_index = *required(&file_indexes, &file_id, "export file")?;
        snapshot.files[file_index].exports.push(CanonicalExport {
            symbol: required(&entity_ids, &symbol_id, "export symbol")?.clone(),
            export_type: row.try_get("export_type").map_err(classify_pg)?,
        });
    }

    let dependency_rows = client
        .query(
            "SELECT source_file_id, target_path, source FROM relation_snapshot_dependencies \
             WHERE project_id = $1 AND relation_epoch = $2",
            &[&manifest.project_id, &manifest.relation_epoch],
        )
        .await
        .map_err(classify_pg)?;
    for row in &dependency_rows {
        let file_id: i64 = row.try_get("source_file_id").map_err(classify_pg)?;
        let file_index = *required(&file_indexes, &file_id, "dependency source")?;
        snapshot.dependencies.push(CanonicalDependency {
            source_file: snapshot.files[file_index].path.clone(),
            target_file: row.try_get("target_path").map_err(classify_pg)?,
            source: row.try_get("source").map_err(classify_pg)?,
        });
    }

    snapshot.build_metadata.symbol_key_conflict_count = manifest.symbol_key_conflict_count;
    snapshot.build_metadata.symbol_key_conflict_samples =
        manifest.symbol_key_conflict_samples.clone();
    snapshot.normalize();
    Ok(snapshot)
}

pub(crate) async fn snapshot_delete_epoch_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
) -> Result<u64, StorageError> {
    let mut removed = 0u64;
    for table in [
        "relation_snapshot_dependencies",
        "relation_snapshot_exports",
        "relation_snapshot_relations",
        "relation_snapshot_entities",
        "relation_snapshot_files",
    ] {
        removed += tx
            .execute(
                &format!("DELETE FROM {table} WHERE project_id = $1 AND relation_epoch = $2"),
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?;
    }
    removed += tx
        .execute(
            "DELETE FROM relation_snapshot_deltas WHERE project_id = $1 AND \
             (base_epoch = $2 OR delta_epoch = $2)",
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?;
    removed += tx
        .execute(
            "DELETE FROM relation_snapshot_manifest \
             WHERE project_id = $1 AND relation_epoch = $2",
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?;
    Ok(removed)
}

fn required<'a, K, V>(values: &'a HashMap<K, V>, key: &K, role: &str) -> Result<&'a V, StorageError>
where
    K: Eq + std::hash::Hash,
{
    values
        .get(key)
        .ok_or_else(|| StorageError::validation(format!("missing {role}")))
}
