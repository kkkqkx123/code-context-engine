use cce_types::StorageError;

use cce_storage_common::relation::{
    GenerationOverride, OverrideDisposition, ProjectIndexManifest, ProjectIndexManifestState,
    ProjectRecord,
};

use crate::error::classify_pg;
use crate::rows::{MANIFEST_COLUMNS, PROJECT_COLUMNS, manifest_from_row, project_from_row};

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn ensure_project(
        &self,
        project_id: i64,
        root_path: &str,
    ) -> Result<(), StorageError> {
        let root_path = root_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().timestamp();
            let name = format!("project_{project_id}");
            client
                .execute(
                    "INSERT INTO projects (id, name, root_path, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $4) ON CONFLICT (id) DO NOTHING",
                    &[&project_id, &name, &root_path, &now],
                )
                .await
                .map_err(classify_pg)?;
            client
                .execute(
                    "SELECT setval(pg_get_serial_sequence('projects', 'id'), \
                     (SELECT MAX(id) FROM projects))",
                    &[],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn project_record(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = $1"),
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| project_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn project_meta_get_int(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<i64, StorageError> {
        let key = key.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let value: Option<String> = client
                .query_opt(
                    "SELECT value FROM project_meta WHERE project_id = $1 AND key = $2",
                    &[&project_id, &key],
                )
                .await
                .map_err(classify_pg)?
                .map(|row| row.get(0));
            value
                .ok_or_else(|| StorageError::not_found(format!("project_meta {key}")))
                .and_then(|raw| {
                    raw.parse::<i64>().map_err(|_| {
                        StorageError::query(format!("project_meta {key} is not an integer"))
                    })
                })
        })
        .await
    }

    pub(crate) async fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        let key = key.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().timestamp();
            client
                .execute(
                    "INSERT INTO project_meta (project_id, key, value, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $4) \
                     ON CONFLICT (project_id, key) DO UPDATE SET \
                        value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
                    &[&project_id, &key, &value.to_string(), &now],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn manifest_begin_building(
        &self,
        project_id: i64,
        data_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        let operation_id = operation_id.to_string();
        let input_fingerprint = input_fingerprint.map(str::to_string);
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let existing = tx
                .query_opt(
                    &format!(
                        "SELECT {MANIFEST_COLUMNS} FROM project_index_manifests \
                         WHERE project_id = $1 AND operation_id = $2"
                    ),
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?
                .map(|row| manifest_from_row(&row))
                .transpose()?;
            if let Some(existing) = existing {
                if existing.data_epoch != data_epoch {
                    return Err(StorageError::validation(format!(
                        "operation {operation_id} already targets data epoch {} instead of {data_epoch}",
                        existing.data_epoch
                    )));
                }
                match existing.state {
                    ProjectIndexManifestState::Building | ProjectIndexManifestState::Active => {
                        tx.commit().await.map_err(classify_pg)?;
                        return Ok(existing);
                    }
                    ProjectIndexManifestState::Failed => {
                        let now = chrono::Utc::now().timestamp();
                        tx.execute(
                            "UPDATE project_index_manifests SET state = 'building', \
                             relation_epoch = 0, activated_at = NULL, failure_reason = NULL, \
                             candidate_ready = 0, created_at = $1 \
                             WHERE project_id = $2 AND operation_id = $3",
                            &[&now, &project_id, &operation_id],
                        )
                        .await
                        .map_err(classify_pg)?;
                        tx.commit().await.map_err(classify_pg)?;
                        return Ok(ProjectIndexManifest {
                            state: ProjectIndexManifestState::Building,
                            relation_epoch: 0,
                            candidate_ready: false,
                            parent_data_epoch: None,
                            ..existing
                        });
                    }
                }
            }
            let publication_epoch: i64 = tx
                .query_one(
                    "SELECT COALESCE(MAX(publication_epoch), 0) + 1 \
                     FROM project_index_manifests WHERE project_id = $1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            let now = chrono::Utc::now().timestamp();
            tx.execute(
                "INSERT INTO project_index_manifests (project_id, publication_epoch, data_epoch, \
                 relation_epoch, operation_id, state, input_fingerprint, created_at) \
                 VALUES ($1, $2, $3, 0, $4, 'building', $5, $6)",
                &[
                    &project_id,
                    &publication_epoch,
                    &data_epoch,
                    &operation_id,
                    &input_fingerprint,
                    &now,
                ],
            )
            .await
            .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(ProjectIndexManifest {
                project_id,
                publication_epoch,
                data_epoch,
                relation_epoch: 0,
                operation_id,
                state: ProjectIndexManifestState::Building,
                input_fingerprint,
                candidate_ready: false,
                parent_data_epoch: None,
            })
        })
        .await
    }

    pub(crate) async fn manifest_mark_candidate_ready(
        &self,
        project_id: i64,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            client
                .execute(
                    "UPDATE project_index_manifests SET candidate_ready = 1 \
                     WHERE project_id = $1 AND operation_id = $2 AND state = 'building'",
                    &[&project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn manifest_activate(
        &self,
        project_id: i64,
        data_epoch: i64,
        relation_epoch: i64,
        operation_id: &str,
        input_fingerprint: Option<&str>,
    ) -> Result<ProjectIndexManifest, StorageError> {
        let operation_id = operation_id.to_string();
        let input_fingerprint = input_fingerprint.map(str::to_string);
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let now = chrono::Utc::now().timestamp();
            if relation_epoch > 0 {
                let state: Option<String> = tx
                    .query_opt(
                        "SELECT state FROM relation_snapshot_manifest \
                         WHERE project_id = $1 AND relation_epoch = $2",
                        &[&project_id, &relation_epoch],
                    )
                    .await
                    .map_err(classify_pg)?
                    .map(|row| row.get(0));
                match state.as_deref() {
                    Some("ready") | Some("active") | Some("delta") => {}
                    _ => {
                        return Err(StorageError::validation(format!(
                            "relation epoch {relation_epoch} is not publishable"
                        )));
                    }
                }
                crate::snapshot::snapshot_activate_in(&tx, project_id, relation_epoch, now).await?;
            }
            let building = manifest_begin_building_in(
                &tx,
                project_id,
                data_epoch,
                &operation_id,
                input_fingerprint.as_deref(),
                now,
            )
            .await?;
            if building.state == ProjectIndexManifestState::Active {
                if building.relation_epoch == relation_epoch {
                    tx.commit().await.map_err(classify_pg)?;
                    return Ok(building);
                }
                return Err(StorageError::validation(format!(
                    "operation {operation_id} is already active at relation epoch {}",
                    building.relation_epoch
                )));
            }
            let changed = tx
                .execute(
                    "UPDATE project_index_manifests SET relation_epoch = $1, state = 'active', \
                     activated_at = $2, input_fingerprint = COALESCE($3, input_fingerprint), \
                     failure_reason = NULL \
                     WHERE project_id = $4 AND publication_epoch = $5 AND state = 'building'",
                    &[
                        &relation_epoch,
                        &now,
                        &input_fingerprint,
                        &project_id,
                        &building.publication_epoch,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            if changed != 1 {
                return Err(StorageError::transaction(format!(
                    "manifest for operation {operation_id} is not building"
                )));
            }
            for (key, value) in [
                ("epoch", data_epoch),
                ("active_epoch", data_epoch),
                ("active_relation_epoch", relation_epoch),
                ("epoch_ready", 1),
            ] {
                tx.execute(
                    "INSERT INTO project_meta (project_id, key, value, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $4) ON CONFLICT (project_id, key) DO UPDATE SET \
                        value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
                    &[&project_id, &key, &value.to_string(), &now],
                )
                .await
                .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(ProjectIndexManifest {
                project_id,
                publication_epoch: building.publication_epoch,
                data_epoch,
                relation_epoch,
                operation_id,
                state: ProjectIndexManifestState::Active,
                input_fingerprint,
                candidate_ready: building.candidate_ready,
                parent_data_epoch: building.parent_data_epoch,
            })
        })
        .await
    }

    pub(crate) async fn manifest_mark_failed(
        &self,
        project_id: i64,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        let reason = reason.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            client
                .execute(
                    "UPDATE project_index_manifests SET state = 'failed', failure_reason = $1 \
                     WHERE project_id = $2 AND operation_id = $3 AND state = 'building'",
                    &[&reason, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn manifest_active(
        &self,
        project_id: i64,
    ) -> Result<Option<ProjectIndexManifest>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {MANIFEST_COLUMNS} FROM project_index_manifests \
                         WHERE project_id = $1 AND state = 'active' \
                         ORDER BY publication_epoch DESC LIMIT 1"
                    ),
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| manifest_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn manifest_recycle_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut removed = 0u64;
            for table in ["files", "entities", "chunks", "entity_detail_mappings"] {
                removed += tx
                    .execute(
                        &format!("DELETE FROM {table} WHERE project_id = $1 AND epoch = $2"),
                        &[&project_id, &epoch],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            removed += tx
                .execute(
                    "DELETE FROM file_summaries WHERE epoch = $1 AND file_id IN \
                     (SELECT id FROM files WHERE project_id = $2)",
                    &[&epoch, &project_id],
                )
                .await
                .map_err(classify_pg)?;
            removed += tx
                .execute(
                    "DELETE FROM generation_overrides WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            removed += crate::snapshot::snapshot_delete_epoch_in(&tx, project_id, epoch).await?;
            removed += tx
                .execute(
                    "DELETE FROM project_index_manifests \
                     WHERE project_id = $1 AND data_epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn overrides_replace(
        &self,
        project_id: i64,
        epoch: i64,
        overrides: &[GenerationOverride],
    ) -> Result<(), StorageError> {
        let overrides = overrides.to_vec();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            tx.execute(
                "DELETE FROM generation_overrides WHERE project_id = $1 AND epoch = $2",
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?;
            for entry in &overrides {
                tx.execute(
                    "INSERT INTO generation_overrides (project_id, epoch, file_path, disposition) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (project_id, epoch, file_path) DO UPDATE SET \
                        disposition = EXCLUDED.disposition",
                    &[
                        &project_id,
                        &epoch,
                        &entry.file_path,
                        &entry.disposition.as_str(),
                    ],
                )
                .await
                .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn overrides_for_generation(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<GenerationOverride>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT file_path, disposition FROM generation_overrides \
                     WHERE project_id = $1 AND epoch = $2 ORDER BY file_path",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| {
                    let file_path: String = row.try_get("file_path").map_err(classify_pg)?;
                    let disposition: String = row.try_get("disposition").map_err(classify_pg)?;
                    let disposition = match disposition.as_str() {
                        "replaced" => OverrideDisposition::Replaced,
                        "deleted" => OverrideDisposition::Deleted,
                        other => {
                            return Err(StorageError::query(format!(
                                "invalid generation override disposition: {other}"
                            )));
                        }
                    };
                    Ok(GenerationOverride {
                        file_path,
                        disposition,
                    })
                })
                .collect()
        })
        .await
    }
}

async fn manifest_begin_building_in(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    data_epoch: i64,
    operation_id: &str,
    input_fingerprint: Option<&str>,
    now: i64,
) -> Result<ProjectIndexManifest, StorageError> {
    let existing = tx
        .query_opt(
            &format!(
                "SELECT {MANIFEST_COLUMNS} FROM project_index_manifests \
                 WHERE project_id = $1 AND operation_id = $2"
            ),
            &[&project_id, &operation_id],
        )
        .await
        .map_err(classify_pg)?
        .map(|row| manifest_from_row(&row))
        .transpose()?;
    if let Some(existing) = existing {
        if existing.data_epoch != data_epoch {
            return Err(StorageError::validation(format!(
                "operation {operation_id} already targets data epoch {} instead of {data_epoch}",
                existing.data_epoch
            )));
        }
        match existing.state {
            ProjectIndexManifestState::Building | ProjectIndexManifestState::Active => {
                return Ok(existing);
            }
            ProjectIndexManifestState::Failed => {
                tx.execute(
                    "UPDATE project_index_manifests SET state = 'building', relation_epoch = 0, \
                     activated_at = NULL, failure_reason = NULL, candidate_ready = 0, \
                     created_at = $1 WHERE project_id = $2 AND operation_id = $3",
                    &[&now, &project_id, &operation_id],
                )
                .await
                .map_err(classify_pg)?;
                return Ok(ProjectIndexManifest {
                    state: ProjectIndexManifestState::Building,
                    relation_epoch: 0,
                    candidate_ready: false,
                    parent_data_epoch: None,
                    ..existing
                });
            }
        }
    }
    let publication_epoch: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(publication_epoch), 0) + 1 \
             FROM project_index_manifests WHERE project_id = $1",
            &[&project_id],
        )
        .await
        .map_err(classify_pg)?
        .get(0);
    tx.execute(
        "INSERT INTO project_index_manifests (project_id, publication_epoch, data_epoch, \
         relation_epoch, operation_id, state, input_fingerprint, created_at) \
         VALUES ($1, $2, $3, 0, $4, 'building', $5, $6)",
        &[
            &project_id,
            &publication_epoch,
            &data_epoch,
            &operation_id,
            &input_fingerprint,
            &now,
        ],
    )
    .await
    .map_err(classify_pg)?;
    Ok(ProjectIndexManifest {
        project_id,
        publication_epoch,
        data_epoch,
        relation_epoch: 0,
        operation_id: operation_id.to_string(),
        state: ProjectIndexManifestState::Building,
        input_fingerprint: input_fingerprint.map(str::to_string),
        candidate_ready: false,
        parent_data_epoch: None,
    })
}
