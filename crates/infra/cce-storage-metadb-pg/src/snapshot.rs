use std::collections::HashMap;

use cce_types::StorageError;
use cce_types::{
    CanonicalRelationSnapshot, CanonicalSnapshotArtifact, RelationSnapshotManifest, SnapshotDelta,
    StableSymbolKey, plan_snapshot_shards,
};

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

    pub(crate) async fn snapshot_write_artifact(
        &self,
        project_id: i64,
        epoch: i64,
        artifact: &CanonicalSnapshotArtifact,
    ) -> Result<(), StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            snapshot_write_artifact_in(&tx, project_id, epoch, &artifact).await?;
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
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let manifest = snapshot_manifest_in(&tx, project_id, epoch)
                .await?
                .ok_or_else(|| StorageError::not_found(format!("relation snapshot {epoch}")))?;
            let snapshot = snapshot_read_in(&tx, &manifest).await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(snapshot)
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

    pub(crate) async fn snapshot_manifest_by_operation(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<Option<RelationSnapshotManifest>, StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {SNAPSHOT_MANIFEST_COLUMNS} FROM relation_snapshot_manifest \
                         WHERE project_id = $1 AND operation_id = $2 LIMIT 1"
                    ),
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| snapshot_manifest_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn snapshot_activate(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            snapshot_activate_in(&tx, project_id, epoch, chrono::Utc::now().timestamp()).await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn snapshot_retry_failed(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "UPDATE relation_snapshot_manifest SET \
                        state = 'building', input_fingerprint = NULL, \
                        snapshot_fingerprint = NULL, file_count = NULL, \
                        entity_count = NULL, relation_count = NULL, dependency_count = NULL, \
                        validated_at = NULL, activated_at = NULL, failure_reason = NULL, \
                        symbol_key_conflict_count = 0, \
                        symbol_key_conflict_samples_json = NULL \
                     WHERE project_id = $1 AND relation_epoch = $2 AND state = 'failed'",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            if changed != 1 {
                return Err(StorageError::transaction(format!(
                    "epoch {epoch} is not retryable"
                )));
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn snapshot_collect_garbage(
        &self,
        project_id: i64,
        stale_before: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM relation_snapshot_manifest \
                     WHERE project_id = $1 AND state <> 'active' \
                       AND operation_id NOT IN (
                          SELECT operation_id FROM checkpoint \
                          WHERE project_id = $1 AND status <> 'completed'
                       ) \
                       AND (\
                          (state IN ('failed', 'building') AND created_at < $2) \
                          OR (state = 'ready' AND relation_epoch NOT IN (
                             SELECT relation_epoch FROM relation_snapshot_manifest \
                             WHERE project_id = $1 AND state = 'ready' \
                             ORDER BY relation_epoch DESC LIMIT 1
                          ))\
                       )",
                    &[&project_id, &stale_before],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn snapshot_entities_count(
        &self,
        project_id: i64,
        relation_epoch: i64,
    ) -> Result<i64, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let count: i64 = client
                .query_one(
                    "SELECT COUNT(*) FROM relation_snapshot_entities \
                     WHERE project_id = $1 AND relation_epoch = $2",
                    &[&project_id, &relation_epoch],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(count)
        })
        .await
    }

    pub(crate) async fn snapshot_delete_manifests_except(
        &self,
        project_id: i64,
        keep_epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM relation_snapshot_manifest \
                     WHERE project_id = $1 AND relation_epoch <> $2",
                    &[&project_id, &keep_epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn snapshot_write_delta(
        &self,
        project_id: i64,
        epoch: i64,
        delta: &SnapshotDelta,
    ) -> Result<(), StorageError> {
        let delta = delta.clone();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
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
            let json = serde_json::to_vec(&delta).map_err(|error| {
                StorageError::validation(format!("delta serialization: {error}"))
            })?;
            let compressed = zstd::encode_all(&*json, 3)
                .map_err(|error| StorageError::validation(format!("delta compression: {error}")))?;
            let size = compressed.len() as i64;
            tx.execute(
                "UPDATE relation_snapshot_manifest SET state = 'delta' \
                 WHERE project_id = $1 AND relation_epoch = $2",
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?;
            tx.execute(
                "INSERT INTO relation_snapshot_deltas (project_id, base_epoch, delta_epoch, \
                 delta_data, size_bytes) VALUES ($1, $2, $3, $4, $5)",
                &[&project_id, &delta.base_epoch, &epoch, &compressed, &size],
            )
            .await
            .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn snapshot_delta_chain_info(
        &self,
        project_id: i64,
        active_epoch: i64,
    ) -> Result<(usize, i64, i64), StorageError> {
        let base_epoch = self
            .snapshot_find_base(project_id, active_epoch)
            .await?
            .unwrap_or(active_epoch);
        if base_epoch >= active_epoch {
            return Ok((0, 0, 0));
        }
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_one(
                    "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) \
                     FROM relation_snapshot_deltas \
                     WHERE project_id = $1 AND delta_epoch > $2 AND delta_epoch <= $3",
                    &[&project_id, &base_epoch, &active_epoch],
                )
                .await
                .map_err(classify_pg)?;
            let count: i64 = row.get(0);
            let total_size: i64 = row.get(1);
            let size_sql = [
                "SELECT COALESCE(SUM(\
                    COALESCE(length(path), 0) + COALESCE(length(language), 0) + \
                    COALESCE(length(input_hash), 0) + COALESCE(length(imports_json), 0)\
                 ), 0) FROM relation_snapshot_files \
                 WHERE project_id = $1 AND relation_epoch = $2",
                "SELECT COALESCE(SUM(\
                    COALESCE(length(scoped_name), 0) + COALESCE(length(kind_json), 0) + \
                    COALESCE(length(name), 0) + COALESCE(length(signature), 0) + \
                    COALESCE(length(parameters_json), 0) + COALESCE(length(return_type), 0) + \
                    COALESCE(length(span_json), 0) + COALESCE(length(doc_comment), 0) + \
                    COALESCE(length(modifiers_json), 0) + COALESCE(length(attributes_json), 0) + \
                    COALESCE(length(metadata_json), 0) + COALESCE(length(subtype), 0)\
                 ), 0) FROM relation_snapshot_entities \
                 WHERE project_id = $1 AND relation_epoch = $2",
                "SELECT COALESCE(SUM(\
                    COALESCE(length(raw_target), 0) + COALESCE(length(relation_type_json), 0) + \
                    COALESCE(length(span_json), 0) + COALESCE(length(external_type_json), 0) + \
                    COALESCE(length(unresolved_reason), 0) + \
                    COALESCE(length(stdlib_category_json), 0)\
                 ), 0) FROM relation_snapshot_relations \
                 WHERE project_id = $1 AND relation_epoch = $2",
                "SELECT COALESCE(SUM(COALESCE(length(export_type), 0)), 0) \
                 FROM relation_snapshot_exports \
                 WHERE project_id = $1 AND relation_epoch = $2",
                "SELECT COALESCE(SUM(\
                    COALESCE(length(target_path), 0) + COALESCE(length(source), 0)\
                 ), 0) FROM relation_snapshot_dependencies \
                 WHERE project_id = $1 AND relation_epoch = $2",
            ];
            let mut base_size = 0i64;
            for sql in size_sql {
                let value: i64 = client
                    .query_one(sql, &[&project_id, &base_epoch])
                    .await
                    .map_err(classify_pg)?
                    .get(0);
                base_size = base_size.saturating_add(value);
            }
            Ok((count.max(0) as usize, total_size, base_size.max(1)))
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
    executor: &impl deadpool_postgres::GenericClient,
    project_id: i64,
    epoch: i64,
) -> Result<Option<RelationSnapshotManifest>, StorageError> {
    let row = executor
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

async fn snapshot_write_artifact_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    artifact: &CanonicalSnapshotArtifact,
) -> Result<(), StorageError> {
    use cce_types::{CanonicalComponentBatch, CanonicalRelationTarget};

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

    let plan = plan_snapshot_shards(artifact, artifact.options().max_shard_items)?;
    let shard_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_shards (project_id, relation_epoch, shard_id,
             file_count, entity_count, relation_count, dependency_count, estimated_bytes,
             path_start, path_end) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .await
        .map_err(classify_pg)?;
    for descriptor in &plan.descriptors {
        let file_count = descriptor.file_count as i64;
        let entity_count = descriptor.entity_count as i64;
        let relation_count = descriptor.relation_count as i64;
        let dependency_count = descriptor.dependency_count as i64;
        let estimated_bytes = descriptor.estimated_bytes as i64;
        tx.execute(
            &shard_stmt,
            &[
                &project_id,
                &epoch,
                &descriptor.shard_id,
                &file_count,
                &entity_count,
                &relation_count,
                &dependency_count,
                &estimated_bytes,
                &descriptor.path_start,
                &descriptor.path_end,
            ],
        )
        .await
        .map_err(classify_pg)?;
    }

    let file_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_files (project_id, relation_epoch, path, \
             language, input_hash, file_size, imports_json, shard_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .await
        .map_err(classify_pg)?;
    let entity_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_entities (project_id, relation_epoch, file_id, \
             scoped_name, kind_json, overload_discriminator, entity_id, stable_id, shard_id, \
             name, signature, parameters_json, return_type, span_json, depth, doc_comment, \
             modifiers_json, attributes_json, metadata_json, is_stdlib, stdlib_category_json, \
             subtype) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, \
                     $14, $15, $16, $17, $18, $19, $20, $21, $22)",
        )
        .await
        .map_err(classify_pg)?;
    let parent_stmt = tx
        .prepare(
            "UPDATE relation_snapshot_entities SET parent_symbol_id = $3 \
             WHERE project_id = $1 AND id = $2",
        )
        .await
        .map_err(classify_pg)?;
    let relation_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_relations (project_id, relation_epoch, \
             caller_symbol_id, target_symbol_id, target_state, raw_target, relation_type_json, \
             span_json, external_type_json, unresolved_reason, stdlib_category_json, \
             call_context_json, owner_type, overload_signature, callee_symbol_json, \
             call_frequency, cfg_condition) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
        )
        .await
        .map_err(classify_pg)?;
    let placeholder_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_entities (project_id, relation_epoch, file_id, \
             scoped_name, kind_json, overload_discriminator, entity_id, stable_id, shard_id, \
             name, signature, parameters_json, return_type, span_json, depth, doc_comment, \
             modifiers_json, attributes_json, metadata_json, is_stdlib, stdlib_category_json, \
             subtype) SELECT $1, $2, id, '<file>', $3, $4, NULL, $5, $6, '<file>', '', '[]', \
                    NULL, $7, 0, NULL, '[]', '{}', '{}', 0, NULL, NULL \
             FROM relation_snapshot_files \
             WHERE project_id = $1 AND relation_epoch = $2 AND path = $8 \
             ON CONFLICT DO NOTHING",
        )
        .await
        .map_err(classify_pg)?;
    let export_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_exports (project_id, relation_epoch, file_id, \
             symbol_id, export_type) VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT DO NOTHING",
        )
        .await
        .map_err(classify_pg)?;
    let dependency_stmt = tx
        .prepare(
            "INSERT INTO relation_snapshot_dependencies (project_id, relation_epoch, \
             source_file_id, target_path, source) VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT DO NOTHING",
        )
        .await
        .map_err(classify_pg)?;

    for batch in artifact.batches(cce_types::DEFAULT_SNAPSHOT_BATCH_ITEMS) {
        match batch.map_err(|error| StorageError::Transaction(error.to_string()))? {
            CanonicalComponentBatch::Files(files) => {
                for file in files {
                    let file_size = file.file_size as i64;
                    let imports_json = to_json(&file.imports)?;
                    tx.execute(
                        &file_stmt,
                        &[
                            &project_id,
                            &epoch,
                            &file.path,
                            &file.language,
                            &file.input_hash,
                            &file_size,
                            &imports_json,
                            &plan.shard_of(&file.path)?,
                        ],
                    )
                    .await
                    .map_err(classify_pg)?;
                }
            }
            CanonicalComponentBatch::Entities(entities) => {
                let mut ids = Vec::with_capacity(entities.len());
                for entity in &entities {
                    let file_id =
                        find_file_in(tx, project_id, epoch, &entity.key.file_path).await?;
                    let kind_json = to_json(&entity.key.kind)?;
                    let parameters_json = to_json(&entity.parameters)?;
                    let span_json = to_json(&entity.span)?;
                    let modifiers_json = to_json(&entity.modifiers)?;
                    let attributes_json = to_json(&entity.attributes)?;
                    let metadata_json = to_json(&entity.metadata)?;
                    let stdlib_category_json = optional_json(&entity.stdlib_category)?;
                    let entity_id = entity.entity_id.map(|id| id as i64);
                    let stable_id = entity.key.stable_id().0;
                    let shard_id = plan.shard_of(&entity.key.file_path)?;
                    let depth = entity.depth as i64;
                    let is_stdlib = i64::from(entity.is_stdlib);
                    let id: i64 = tx
                        .query_one(
                            &entity_stmt,
                            &[
                                &project_id,
                                &epoch,
                                &file_id,
                                &entity.key.scoped_name,
                                &kind_json,
                                &entity.key.overload_discriminator,
                                &entity_id,
                                &stable_id,
                                &shard_id,
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
                    ids.push(id);
                }
                for (entity, id) in entities.iter().zip(ids) {
                    if let Some(parent) = &entity.parent {
                        let parent_id = find_entity_in(tx, project_id, epoch, parent).await?;
                        tx.execute(&parent_stmt, &[&project_id, &id, &parent_id])
                            .await
                            .map_err(classify_pg)?;
                    }
                }
            }
            CanonicalComponentBatch::Relations(relations) => {
                for relation in relations {
                    if relation.caller.is_file_placeholder() {
                        let kind_json = to_json(&relation.caller.kind)?;
                        let span_json = to_json(&cce_types::Span::default())?;
                        tx.execute(
                            &placeholder_stmt,
                            &[
                                &project_id,
                                &epoch,
                                &kind_json,
                                &relation.caller.overload_discriminator,
                                &relation.caller.stable_id().0,
                                &plan.shard_of(&relation.caller.file_path)?,
                                &span_json,
                                &relation.caller.file_path,
                            ],
                        )
                        .await
                        .map_err(classify_pg)?;
                    }
                    let caller_id = find_entity_in(tx, project_id, epoch, &relation.caller).await?;
                    let (target_id, target_state, external_json, unresolved_reason) =
                        match &relation.target {
                            CanonicalRelationTarget::Internal { key } => (
                                Some(find_entity_in(tx, project_id, epoch, key).await?),
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
                        &relation_stmt,
                        &[
                            &project_id,
                            &epoch,
                            &caller_id,
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
            }
            CanonicalComponentBatch::Dependencies(dependencies) => {
                for dependency in dependencies {
                    let source_id =
                        find_file_in(tx, project_id, epoch, &dependency.source_file).await?;
                    tx.execute(
                        &dependency_stmt,
                        &[
                            &project_id,
                            &epoch,
                            &source_id,
                            &dependency.target_file,
                            &dependency.source,
                        ],
                    )
                    .await
                    .map_err(classify_pg)?;
                }
            }
        }
    }

    for files in artifact.file_batches(cce_types::DEFAULT_SNAPSHOT_BATCH_ITEMS) {
        for file in files? {
            let file_id = find_file_in(tx, project_id, epoch, &file.path).await?;
            for export in file.exports {
                let Ok(symbol_id) = find_entity_in(tx, project_id, epoch, &export.symbol).await
                else {
                    tracing::warn!(
                        file = %file.path,
                        symbol = ?export.symbol,
                        "Skipping export with no local snapshot entity"
                    );
                    continue;
                };
                tx.execute(
                    &export_stmt,
                    &[
                        &project_id,
                        &epoch,
                        &file_id,
                        &symbol_id,
                        &export.export_type,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            }
        }
    }

    let counts = artifact.counts();
    let conflict_samples_json =
        to_json(&artifact.header().build_metadata.symbol_key_conflict_samples)?;
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
                &artifact.input_fingerprint().to_string(),
                &artifact.snapshot_fingerprint().to_string(),
                &(counts.files as i64),
                &(counts.entities as i64),
                &(counts.relations as i64),
                &(counts.dependencies as i64),
                &chrono::Utc::now().timestamp(),
                &(artifact.header().build_metadata.symbol_key_conflict_count as i64),
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

async fn find_file_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    path: &str,
) -> Result<i64, StorageError> {
    tx.query_opt(
        "SELECT id FROM relation_snapshot_files \
         WHERE project_id = $1 AND relation_epoch = $2 AND path = $3",
        &[&project_id, &epoch, &path],
    )
    .await
    .map_err(classify_pg)?
    .map(|row| row.get(0))
    .ok_or_else(|| StorageError::validation(format!("snapshot file {path} is missing")))
}

async fn find_entity_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    key: &StableSymbolKey,
) -> Result<i64, StorageError> {
    let kind_json = to_json(&key.kind)?;
    tx.query_opt(
        "SELECT e.id FROM relation_snapshot_entities e \
         JOIN relation_snapshot_files f ON f.id = e.file_id \
         WHERE e.project_id = $1 AND e.relation_epoch = $2 AND f.path = $3 \
           AND e.scoped_name = $4 AND e.kind_json = $5 AND e.overload_discriminator = $6",
        &[
            &project_id,
            &epoch,
            &key.file_path,
            &key.scoped_name,
            &kind_json,
            &key.overload_discriminator,
        ],
    )
    .await
    .map_err(classify_pg)?
    .map(|row| row.get(0))
    .ok_or_else(|| {
        StorageError::validation(format!(
            "snapshot symbol {} in {} is missing",
            key.scoped_name, key.file_path
        ))
    })
}

async fn snapshot_read_in(
    executor: &impl deadpool_postgres::GenericClient,
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

    let file_rows = executor
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

    let entity_rows = executor
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

    let relation_rows = executor
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

    let export_rows = executor
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

    let dependency_rows = executor
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
