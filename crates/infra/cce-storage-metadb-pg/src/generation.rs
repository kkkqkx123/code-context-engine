//! Cross-generation content operations for the PostgreSQL branch.
//!
//! Every method copies or removes rows inside one transaction so a retry
//! after a transient failure cannot leave a half-materialized generation.

use std::collections::HashMap;

use cce_storage_common::metadb::{GenerationGcPlan, OverrideDisposition};
use cce_types::StorageError;

use crate::error::classify_pg;
use crate::rows::{ENTITY_COLUMNS, entity_from_row};

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn generation_copy_file(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let exists: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM files \
                     WHERE project_id = $1 AND epoch = $2 AND path = $3)",
                    &[&project_id, &target_epoch, &path],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            if exists {
                return Ok(0);
            }
            let source = tx
                .query_opt(
                    "SELECT language, category, last_modified, created_at, content_hash, batch_id \
                     FROM files WHERE project_id = $1 AND epoch = $2 AND path = $3",
                    &[&project_id, &source_epoch, &path],
                )
                .await
                .map_err(classify_pg)?;
            let Some(source) = source else {
                return Ok(0);
            };
            let new_file_id: i64 = tx
                .query_one(
                    "INSERT INTO files (path, language, category, last_modified, created_at, \
                     project_id, content_hash, epoch, batch_id) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
                    &[
                        &path,
                        &source.get::<_, String>(0),
                        &source.get::<_, i64>(1),
                        &source.get::<_, i64>(2),
                        &source.get::<_, i64>(3),
                        &project_id,
                        &source.get::<_, Option<String>>(4),
                        &target_epoch,
                        &source.get::<_, i64>(5),
                    ],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            let entities = load_entities(&tx, project_id, source_epoch, Some(&path)).await?;
            let copied = copy_entities(&tx, project_id, target_epoch, entities, |_| {
                Some(new_file_id)
            })
            .await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(copied.len())
        })
        .await
    }

    pub(crate) async fn generation_materialize(
        &self,
        project_id: i64,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), StorageError> {
        let excluded_paths = excluded_paths.to_vec();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let source_paths: Vec<String> = if excluded_paths.is_empty() {
                tx.query(
                    "SELECT path FROM files WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &source_epoch],
                )
                .await
                .map_err(classify_pg)?
                .into_iter()
                .map(|row| row.get(0))
                .collect()
            } else {
                tx.query(
                    "SELECT path FROM files WHERE project_id = $1 AND epoch = $2 \
                     AND path <> ALL($3)",
                    &[&project_id, &source_epoch, &excluded_paths],
                )
                .await
                .map_err(classify_pg)?
                .into_iter()
                .map(|row| row.get(0))
                .collect()
            };
            for path in &source_paths {
                tx.execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND epoch = $2 AND file_path = $3",
                    &[&project_id, &target_epoch, path],
                )
                .await
                .map_err(classify_pg)?;
            }
            let files = if excluded_paths.is_empty() {
                tx.query(
                    "SELECT id, path, language, category, last_modified, created_at, \
                     content_hash, batch_id FROM files WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &source_epoch],
                )
                .await
                .map_err(classify_pg)?
            } else {
                tx.query(
                    "SELECT id, path, language, category, last_modified, created_at, \
                     content_hash, batch_id FROM files \
                     WHERE project_id = $1 AND epoch = $2 AND path <> ALL($3)",
                    &[&project_id, &source_epoch, &excluded_paths],
                )
                .await
                .map_err(classify_pg)?
            };
            let mut file_ids = HashMap::new();
            for row in &files {
                let old_id: i64 = row.get("id");
                let new_id: i64 = tx
                    .query_one(
                        "INSERT INTO files (path, language, category, last_modified, created_at, \
                         project_id, content_hash, epoch, batch_id) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
                        &[
                            &row.get::<_, String>("path"),
                            &row.get::<_, String>("language"),
                            &row.get::<_, i64>("category"),
                            &row.get::<_, i64>("last_modified"),
                            &row.get::<_, i64>("created_at"),
                            &project_id,
                            &row.get::<_, Option<String>>("content_hash"),
                            &target_epoch,
                            &row.get::<_, i64>("batch_id"),
                        ],
                    )
                    .await
                    .map_err(classify_pg)?
                    .get(0);
                file_ids.insert(old_id, new_id);
            }
            let entities = load_entities(&tx, project_id, source_epoch, None).await?;
            let entity_ids =
                copy_entities(&tx, project_id, target_epoch, entities, |old_file_id| {
                    file_ids.get(&old_file_id).copied()
                })
                .await?;
            let mappings = tx
                .query(
                    "SELECT entity_id, qdrant_point_ids, bm25_doc_ids, chunk_count, \
                     created_at, updated_at FROM entity_detail_mappings \
                     WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &source_epoch],
                )
                .await
                .map_err(classify_pg)?;
            for row in &mappings {
                let old_entity_id: i64 = row.get("entity_id");
                let Some(new_entity_id) = entity_ids.get(&old_entity_id) else {
                    continue;
                };
                tx.execute(
                    "INSERT INTO entity_detail_mappings (entity_id, project_id, epoch, \
                     qdrant_point_ids, bm25_doc_ids, chunk_count, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                     ON CONFLICT (project_id, epoch, entity_id) DO UPDATE SET \
                        qdrant_point_ids = EXCLUDED.qdrant_point_ids, \
                        bm25_doc_ids = EXCLUDED.bm25_doc_ids, \
                        chunk_count = EXCLUDED.chunk_count",
                    &[
                        new_entity_id,
                        &project_id,
                        &target_epoch,
                        &row.get::<_, String>("qdrant_point_ids"),
                        &row.get::<_, String>("bm25_doc_ids"),
                        &row.get::<_, i64>("chunk_count"),
                        &row.get::<_, i64>("created_at"),
                        &row.get::<_, i64>("updated_at"),
                    ],
                )
                .await
                .map_err(classify_pg)?;
            }
            if excluded_paths.is_empty() {
                tx.execute(
                    "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, \
                     entity_ids, entity_names, entity_kinds, group_title, chunk_type, \
                     test_status, test_source, created_at, updated_at, project_id, epoch, \
                     batch_id, path, bm25_keywords, segment_id, truncated) \
                     SELECT chunk_id, file_path, content, start_line, end_line, entity_ids, \
                        entity_names, entity_kinds, group_title, chunk_type, test_status, \
                        test_source, created_at, updated_at, project_id, $2, batch_id, path, \
                        bm25_keywords, segment_id, truncated \
                     FROM chunks WHERE project_id = $1 AND epoch = $3 \
                     ON CONFLICT (project_id, epoch, file_path, chunk_id) DO NOTHING",
                    &[&project_id, &target_epoch, &source_epoch],
                )
                .await
                .map_err(classify_pg)?;
            } else {
                tx.execute(
                    "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, \
                     entity_ids, entity_names, entity_kinds, group_title, chunk_type, \
                     test_status, test_source, created_at, updated_at, project_id, epoch, \
                     batch_id, path, bm25_keywords, segment_id, truncated) \
                     SELECT chunk_id, file_path, content, start_line, end_line, entity_ids, \
                        entity_names, entity_kinds, group_title, chunk_type, test_status, \
                        test_source, created_at, updated_at, project_id, $2, batch_id, path, \
                        bm25_keywords, segment_id, truncated \
                     FROM chunks WHERE project_id = $1 AND epoch = $3 \
                        AND file_path <> ALL($4) \
                     ON CONFLICT (project_id, epoch, file_path, chunk_id) DO NOTHING",
                    &[&project_id, &target_epoch, &source_epoch, &excluded_paths],
                )
                .await
                .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn generation_gc_plan(
        &self,
        project_id: i64,
        keep_active_generations: usize,
        stale_before: i64,
    ) -> Result<GenerationGcPlan, StorageError> {
        let keep = keep_active_generations.max(1) as i64;
        self.run(async move {
            let client = self.pooled().await?;
            let mut plan = GenerationGcPlan::default();
            let rows = client
                .query(
                    "SELECT publication_epoch, data_epoch, relation_epoch, state, operation_id, \
                     created_at, parent_data_epoch FROM project_index_manifests \
                     WHERE project_id = $1 ORDER BY publication_epoch DESC",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            let mut active_count = 0i64;
            for row in rows {
                let publication: i64 = row.get("publication_epoch");
                let data_epoch: i64 = row.get("data_epoch");
                let relation_epoch: i64 = row.get("relation_epoch");
                let state: String = row.get("state");
                let operation_id: String = row.get("operation_id");
                let created_at: i64 = row.get("created_at");
                let parent: Option<i64> = row.get("parent_data_epoch");
                let checkpoint_active: bool = client
                    .query_one(
                        "SELECT EXISTS(SELECT 1 FROM checkpoint \
                         WHERE project_id = $1 AND operation_id = $2 AND status = 'in_progress')",
                        &[&project_id, &operation_id],
                    )
                    .await
                    .map_err(classify_pg)?
                    .get(0);
                let keep_generation = match state.as_str() {
                    "active" => {
                        let keep = active_count < keep;
                        active_count += 1;
                        keep
                    }
                    "building" => checkpoint_active || created_at >= stale_before,
                    _ => created_at >= stale_before,
                };
                if keep_generation {
                    if data_epoch > 0 {
                        push_unique(&mut plan.protected_data_epochs, data_epoch);
                    }
                    let mut ancestor = parent;
                    let mut depth = 0;
                    while let Some(epoch) = ancestor
                        && epoch > 0
                        && depth < 2
                    {
                        push_unique(&mut plan.protected_data_epochs, epoch);
                        ancestor = client
                            .query_opt(
                                "SELECT parent_data_epoch FROM project_index_manifests \
                                 WHERE project_id = $1 AND data_epoch = $2 \
                                 ORDER BY publication_epoch DESC LIMIT 1",
                                &[&project_id, &epoch],
                            )
                            .await
                            .map_err(classify_pg)?
                            .and_then(|row| row.get(0));
                        depth += 1;
                    }
                    if relation_epoch > 0 {
                        push_unique(&mut plan.protected_relation_epochs, relation_epoch);
                    }
                } else {
                    plan.stale_publication_epochs.push(publication);
                    if data_epoch > 0 {
                        push_unique(&mut plan.stale_data_epochs, data_epoch);
                    }
                    if relation_epoch > 0 {
                        push_unique(&mut plan.stale_relation_epochs, relation_epoch);
                    }
                }
            }
            for key in ["active_epoch", "epoch"] {
                let value: Option<String> = client
                    .query_opt(
                        "SELECT value FROM project_meta WHERE project_id = $1 AND key = $2",
                        &[&project_id, &key],
                    )
                    .await
                    .map_err(classify_pg)?
                    .map(|row| row.get(0));
                if let Some(epoch) = value.and_then(|value| value.parse::<i64>().ok())
                    && epoch > 0
                {
                    push_unique(&mut plan.protected_data_epochs, epoch);
                }
            }
            let active_relation: Option<String> = client
                .query_opt(
                    "SELECT value FROM project_meta \
                     WHERE project_id = $1 AND key = 'active_relation_epoch'",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?
                .map(|row| row.get(0));
            if let Some(mut epoch) = active_relation.and_then(|value| value.parse::<i64>().ok())
                && epoch > 0
            {
                loop {
                    push_unique(&mut plan.protected_relation_epochs, epoch);
                    let base: Option<i64> = client
                        .query_opt(
                            "SELECT base_epoch FROM relation_snapshot_deltas \
                             WHERE project_id = $1 AND delta_epoch = $2",
                            &[&project_id, &epoch],
                        )
                        .await
                        .map_err(classify_pg)?
                        .map(|row| row.get(0));
                    match base {
                        Some(base) if base > 0 && base != epoch => epoch = base,
                        _ => break,
                    }
                }
            }
            let relation_rows = client
                .query(
                    "SELECT relation_epoch, created_at FROM relation_snapshot_manifest \
                     WHERE project_id = $1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            for row in relation_rows {
                let epoch: i64 = row.get(0);
                let created_at: i64 = row.get(1);
                if epoch > 0
                    && created_at < stale_before
                    && !plan.protected_relation_epochs.contains(&epoch)
                {
                    push_unique(&mut plan.stale_relation_epochs, epoch);
                }
            }
            plan.stale_data_epochs
                .retain(|epoch| !plan.protected_data_epochs.contains(epoch));
            plan.stale_relation_epochs
                .retain(|epoch| !plan.protected_relation_epochs.contains(epoch));
            Ok(plan)
        })
        .await
    }

    pub(crate) async fn generation_gc_apply(
        &self,
        project_id: i64,
        plan: &GenerationGcPlan,
    ) -> Result<(), StorageError> {
        let plan = plan.clone();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            for epoch in plan.stale_data_epochs {
                for table in [
                    "entity_detail_mappings",
                    "chunks",
                    "entities",
                    "files",
                    "generation_overrides",
                ] {
                    tx.execute(
                        &format!("DELETE FROM {table} WHERE project_id = $1 AND epoch = $2"),
                        &[&project_id, &epoch],
                    )
                    .await
                    .map_err(classify_pg)?;
                }
            }
            for epoch in plan.stale_publication_epochs {
                tx.execute(
                    "DELETE FROM project_index_manifests \
                     WHERE project_id = $1 AND publication_epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            }
            for epoch in plan.stale_relation_epochs {
                crate::snapshot::snapshot_delete_epoch_in(&tx, project_id, epoch).await?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn candidate_begin(
        &self,
        project_id: i64,
        candidate_epoch: i64,
        operation_id: &str,
        parent_data_epoch: Option<i64>,
    ) -> Result<(), StorageError> {
        let operation_id = operation_id.to_string();
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            crate::project::manifest_begin_building_in(
                &tx,
                project_id,
                candidate_epoch,
                &operation_id,
                None,
                chrono::Utc::now().timestamp(),
            )
            .await?;
            tx.execute(
                "UPDATE project_index_manifests SET parent_data_epoch = $1 \
                 WHERE project_id = $2 AND operation_id = $3",
                &[&parent_data_epoch, &project_id, &operation_id],
            )
            .await
            .map_err(classify_pg)?;
            clear_candidate_rows(&tx, project_id, candidate_epoch, &[]).await?;
            tx.execute(
                "DELETE FROM generation_overrides WHERE project_id = $1 AND epoch = $2",
                &[&project_id, &candidate_epoch],
            )
            .await
            .map_err(classify_pg)?;
            tx.execute(
                "UPDATE project_index_manifests SET candidate_ready = 1 \
                 WHERE project_id = $1 AND operation_id = $2 AND state = 'building'",
                &[&project_id, &operation_id],
            )
            .await
            .map_err(classify_pg)?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn candidate_clear_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut removed = clear_candidate_rows(&tx, project_id, epoch, &[]).await?;
            removed += tx
                .execute(
                    "DELETE FROM generation_overrides WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)? as usize;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed)
        })
        .await
    }

    pub(crate) async fn candidate_delete_file_rows(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
    ) -> Result<usize, StorageError> {
        let paths = paths.to_vec();
        if paths.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let removed = clear_candidate_rows(&tx, project_id, epoch, &paths).await?;
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed)
        })
        .await
    }

    pub(crate) async fn candidate_prepare_channel(
        &self,
        project_id: i64,
        epoch: i64,
        paths: &[String],
        channel: &str,
    ) -> Result<(), StorageError> {
        let paths = paths.to_vec();
        let channel = channel.to_string();
        if paths.is_empty() {
            return Ok(());
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            for path in &paths {
                tx.execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND epoch = $2 \
                     AND file_path = $3 AND path = $4",
                    &[&project_id, &epoch, path, &channel],
                )
                .await
                .map_err(classify_pg)?;
                tx.execute(
                    "INSERT INTO generation_overrides (project_id, epoch, file_path, disposition) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (project_id, epoch, file_path) DO UPDATE SET \
                        disposition = EXCLUDED.disposition",
                    &[
                        &project_id,
                        &epoch,
                        path,
                        &OverrideDisposition::Replaced.as_str(),
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
}

/// Load entities of one generation, optionally narrowed to a file path.
async fn load_entities(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    path: Option<&str>,
) -> Result<Vec<cce_storage_common::metadb::EntityRecord>, StorageError> {
    let rows = match path {
        Some(path) => tx
            .query(
                &format!(
                    "SELECT {ENTITY_COLUMNS} FROM entities e JOIN files f ON f.id = e.file_id \
                     WHERE e.project_id = $1 AND e.epoch = $2 AND f.path = $3 ORDER BY e.id"
                ),
                &[&project_id, &epoch, &path],
            )
            .await
            .map_err(classify_pg)?,
        None => tx
            .query(
                &format!(
                    "SELECT {ENTITY_COLUMNS} FROM entities \
                     WHERE project_id = $1 AND epoch = $2 ORDER BY id"
                ),
                &[&project_id, &epoch],
            )
            .await
            .map_err(classify_pg)?,
    };
    rows.iter().map(entity_from_row).collect()
}

/// Copy entities into a target generation and remap parent references.
async fn copy_entities(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    target_epoch: i64,
    entities: Vec<cce_storage_common::metadb::EntityRecord>,
    file_id: impl Fn(i64) -> Option<i64>,
) -> Result<HashMap<i64, i64>, StorageError> {
    let mut ids = HashMap::new();
    let mut parents = Vec::new();
    for mut entity in entities {
        let old_id = entity.id;
        let Some(new_file_id) = file_id(entity.file_id) else {
            continue;
        };
        entity.file_id = new_file_id;
        entity.epoch = target_epoch;
        entity.project_id = project_id;
        let old_parent = entity.parent_id;
        entity.id = 0;
        entity.parent_id = None;
        let new_id = insert_entity_in(tx, &entity).await?;
        ids.insert(old_id, new_id);
        parents.push((new_id, old_parent));
    }
    for (new_id, old_parent) in parents {
        if let Some(old_parent) = old_parent
            && let Some(new_parent) = ids.get(&old_parent)
        {
            tx.execute(
                "UPDATE entities SET parent_id = $1 WHERE id = $2",
                &[new_parent, &new_id],
            )
            .await
            .map_err(classify_pg)?;
        }
    }
    Ok(ids)
}

async fn insert_entity_in(
    tx: &tokio_postgres::Transaction<'_>,
    entity: &cce_storage_common::metadb::EntityRecord,
) -> Result<i64, StorageError> {
    tx.query_one(
        "INSERT INTO entities (name, kind, file_id, signature, span_start_row, span_end_row, \
         span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, \
         overload_discriminator, depth, metadata, parameters_json, return_type, doc_comment, \
         modifiers_json, project_id, epoch, batch_id, rank) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
         $18, $19, $20, $21, $22) RETURNING id",
        &[
            &entity.name,
            &entity.kind,
            &entity.file_id,
            &entity.signature,
            &entity.span_start_row,
            &entity.span_end_row,
            &entity.span_start_column,
            &entity.span_end_column,
            &entity.span_start_byte,
            &entity.span_end_byte,
            &entity.scoped_name,
            &entity.overload_discriminator,
            &entity.depth,
            &entity.metadata,
            &entity.parameters_json,
            &entity.return_type,
            &entity.doc_comment,
            &entity.modifiers_json,
            &entity.project_id,
            &entity.epoch,
            &entity.batch_id,
            &(entity.rank as f64),
        ],
    )
    .await
    .map_err(classify_pg)?
    .try_get(0)
    .map_err(classify_pg)
}

/// Clear candidate content rows, optionally restricted to one path set.
async fn clear_candidate_rows(
    tx: &tokio_postgres::Transaction<'_>,
    project_id: i64,
    epoch: i64,
    paths: &[String],
) -> Result<usize, StorageError> {
    let mut removed = 0u64;
    if paths.is_empty() {
        for table in ["chunks", "entities", "files"] {
            removed += tx
                .execute(
                    &format!("DELETE FROM {table} WHERE project_id = $1 AND epoch = $2"),
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
        }
    } else {
        for path in paths {
            removed += tx
                .execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND epoch = $2 AND file_path = $3",
                    &[&project_id, &epoch, path],
                )
                .await
                .map_err(classify_pg)?;
            removed += tx
                .execute(
                    "DELETE FROM entities WHERE project_id = $1 AND epoch = $2 \
                     AND file_id IN (SELECT id FROM files WHERE project_id = $1 AND epoch = $2 \
                     AND path = $3)",
                    &[&project_id, &epoch, path],
                )
                .await
                .map_err(classify_pg)?;
        }
    }
    Ok(removed as usize)
}

fn push_unique(values: &mut Vec<i64>, value: i64) {
    if !values.contains(&value) {
        values.push(value);
    }
}
