use cce_types::StorageError;

use cce_storage_common::metadb::{
    ChunkContent, ChunkRecord, EntityDetailMapping, EntityRecord, EntityWithFile, FileEnsureEntry,
    FileEpochEntry, FileHashEntry, FileRecord,
};

use crate::error::classify_pg;
use crate::rows::*;

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn files_upsert(
        &self,
        project_id: i64,
        epoch: i64,
        files: &[FileRecord],
    ) -> Result<usize, StorageError> {
        let files = files.to_vec();
        if files.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut count = 0usize;
            for file in &files {
                tx.execute(
                    "INSERT INTO files (path, language, category, last_modified, created_at, \
                     project_id, content_hash, epoch, batch_id) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0) \
                     ON CONFLICT (project_id, epoch, path) DO UPDATE SET \
                        language = EXCLUDED.language, category = EXCLUDED.category, \
                        last_modified = EXCLUDED.last_modified, \
                        content_hash = EXCLUDED.content_hash",
                    &[
                        &file.path,
                        &file.language,
                        &(file.category as i64),
                        &file.last_modified,
                        &file.created_at,
                        &project_id,
                        &file.content_hash,
                        &epoch,
                    ],
                )
                .await
                .map_err(classify_pg)?;
                count += 1;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(count)
        })
        .await
    }

    pub(crate) async fn files_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM files WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn files_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute("DELETE FROM files WHERE project_id = $1", &[&project_id])
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn entities_upsert(
        &self,
        entities: &[EntityRecord],
    ) -> Result<usize, StorageError> {
        let entities = entities.to_vec();
        if entities.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut count = 0usize;
            for entity in &entities {
                tx.execute(
                    "INSERT INTO entities (name, kind, file_id, signature, span_start_row, \
                     span_end_row, span_start_column, span_end_column, span_start_byte, \
                     span_end_byte, scoped_name, overload_discriminator, depth, parent_id, metadata, parameters_json, \
                     return_type, doc_comment, modifiers_json, project_id, epoch, batch_id, rank) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                     $16, $17, $18, $19, $20, $21, $22, $23) \
                     ON CONFLICT (project_id, epoch, file_id, scoped_name, kind, overload_discriminator) DO UPDATE SET \
                        name = EXCLUDED.name, signature = EXCLUDED.signature, \
                        span_start_row = EXCLUDED.span_start_row, \
                        span_end_row = EXCLUDED.span_end_row, \
                        span_start_column = EXCLUDED.span_start_column, \
                        span_end_column = EXCLUDED.span_end_column, \
                        span_start_byte = EXCLUDED.span_start_byte, \
                        span_end_byte = EXCLUDED.span_end_byte, depth = EXCLUDED.depth, \
                        parent_id = EXCLUDED.parent_id, metadata = EXCLUDED.metadata, \
                        parameters_json = EXCLUDED.parameters_json, \
                        return_type = EXCLUDED.return_type, doc_comment = EXCLUDED.doc_comment, \
                        modifiers_json = EXCLUDED.modifiers_json, batch_id = EXCLUDED.batch_id, \
                        rank = EXCLUDED.rank",
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
                        &entity.parent_id,
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
                .map_err(classify_pg)?;
                count += 1;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(count)
        })
        .await
    }

    pub(crate) async fn entities_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM entities WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn entities_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute("DELETE FROM entities WHERE project_id = $1", &[&project_id])
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn entities_count(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<i64, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let count: i64 = client
                .query_one(
                    "SELECT COUNT(*) FROM entities WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(count)
        })
        .await
    }

    pub(crate) async fn chunks_upsert(
        &self,
        chunks: &[ChunkRecord],
    ) -> Result<usize, StorageError> {
        let chunks = chunks.to_vec();
        if chunks.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut count = 0usize;
            for chunk in &chunks {
                let project_id: Option<i64> = chunk.project_id;
                tx.execute(
                    "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, \
                     entity_ids, entity_names, entity_kinds, group_title, chunk_type, test_status, test_source, created_at, \
                     updated_at, project_id, epoch, batch_id, path, bm25_keywords, segment_id, \
                     truncated) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                     $16, $17, $18, $19, $20, $21) \
                     ON CONFLICT (project_id, epoch, file_path, chunk_id) DO UPDATE SET \
                        file_path = EXCLUDED.file_path, content = EXCLUDED.content, \
                        start_line = EXCLUDED.start_line, end_line = EXCLUDED.end_line, \
                        entity_ids = EXCLUDED.entity_ids, entity_names = EXCLUDED.entity_names, \
                        entity_kinds = EXCLUDED.entity_kinds, group_title = EXCLUDED.group_title, \
                        chunk_type = EXCLUDED.chunk_type, test_status = EXCLUDED.test_status, \
                        test_source = EXCLUDED.test_source, updated_at = EXCLUDED.updated_at, \
                        batch_id = EXCLUDED.batch_id, path = EXCLUDED.path, \
                        bm25_keywords = EXCLUDED.bm25_keywords, \
                        segment_id = EXCLUDED.segment_id, truncated = EXCLUDED.truncated",
                    &[
                        &chunk.chunk_id,
                        &chunk.file_path,
                        &chunk.content,
                        &chunk.start_line,
                        &chunk.end_line,
                        &chunk.entity_ids,
                        &chunk.entity_names,
                        &chunk.entity_kinds,
                        &chunk.group_title,
                        &chunk.chunk_type,
                        &(chunk.test_status as i64),
                        &(chunk.test_source as i64),
                        &chunk.created_at,
                        &chunk.updated_at,
                        &project_id,
                        &chunk.epoch,
                        &chunk.batch_id,
                        &chunk.path,
                        &chunk.bm25_keywords,
                        &chunk.segment_id,
                        &(chunk.truncated as i64),
                    ],
                )
                .await
                .map_err(classify_pg)?;
                count += 1;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(count)
        })
        .await
    }

    pub(crate) async fn chunks_by_ids(
        &self,
        project_id: i64,
        chunk_ids: &[String],
        epochs: &[i64],
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        let chunk_ids = chunk_ids.to_vec();
        let epochs = epochs.to_vec();
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.run(async move {
            let client = self.pooled().await?;
            let rows = if epochs.is_empty() {
                client
                    .query(
                        &format!(
                            "SELECT {CHUNK_COLUMNS} FROM chunks \
                             WHERE project_id = $1 AND chunk_id = ANY($2)"
                        ),
                        &[&project_id, &chunk_ids],
                    )
                    .await
                    .map_err(classify_pg)?
            } else {
                client
                    .query(
                        &format!(
                            "SELECT {CHUNK_COLUMNS} FROM chunks \
                             WHERE project_id = $1 AND chunk_id = ANY($2) AND epoch = ANY($3)"
                        ),
                        &[&project_id, &chunk_ids, &epochs],
                    )
                    .await
                    .map_err(classify_pg)?
            };
            rows.iter().map(chunk_from_row).collect()
        })
        .await
    }

    pub(crate) async fn chunks_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn chunks_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute("DELETE FROM chunks WHERE project_id = $1", &[&project_id])
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn chunks_count(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<i64, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let count: i64 = client
                .query_one(
                    "SELECT COUNT(*) FROM chunks WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(count)
        })
        .await
    }

    pub(crate) async fn mappings_upsert(
        &self,
        mappings: &[EntityDetailMapping],
    ) -> Result<usize, StorageError> {
        let mappings = mappings.to_vec();
        if mappings.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut count = 0usize;
            for mapping in &mappings {
                tx.execute(
                    "INSERT INTO entity_detail_mappings (entity_id, project_id, epoch, \
                     qdrant_point_ids, bm25_doc_ids, chunk_count, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                     ON CONFLICT (project_id, epoch, entity_id) DO UPDATE SET \
                        qdrant_point_ids = EXCLUDED.qdrant_point_ids, \
                        bm25_doc_ids = EXCLUDED.bm25_doc_ids, \
                        chunk_count = EXCLUDED.chunk_count, \
                        updated_at = EXCLUDED.updated_at",
                    &[
                        &mapping.entity_id,
                        &mapping.project_id,
                        &mapping.epoch,
                        &mapping.qdrant_point_ids,
                        &mapping.bm25_doc_ids,
                        &mapping.chunk_count,
                        &mapping.created_at,
                        &mapping.updated_at,
                    ],
                )
                .await
                .map_err(classify_pg)?;
                count += 1;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(count)
        })
        .await
    }

    pub(crate) async fn mappings_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM entity_detail_mappings WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn mappings_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM entity_detail_mappings WHERE project_id = $1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }
}

/// Maximum query tokens admitted into the portable entity search.
const MAX_FTS_TOKENS: usize = 8;

impl PostgresClient {
    /// Portable substring entity search over one project and epoch.
    ///
    /// Baseline ranking ordered by row id; a future `tsvector` index can
    /// replace the `ILIKE` conjunction without changing the contract.
    /// FTS5 syntax is sanitized to plain tokens; an empty token set matches
    /// no rows. Excluded file paths are dropped inside the branch.
    pub(crate) async fn entities_search_fts(
        &self,
        project_id: i64,
        query: &str,
        limit: i64,
        epoch: i64,
        excluded_files: &[String],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        let tokens = sanitize_fts_query(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let excluded: Vec<String> = excluded_files.to_vec();
        self.run(async move {
            let columns = crate::rows::ENTITY_COLUMNS
                .split(',')
                .map(|column| format!("e.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ");
            let mut sql = format!(
                "SELECT {columns} FROM entities e JOIN files f ON f.id = e.file_id \
                 WHERE e.project_id = $1 AND e.epoch = $2"
            );
            let mut next = 3i32;
            for _ in &tokens {
                sql.push_str(&format!(
                    " AND (e.name ILIKE ${next} ESCAPE '\\' OR e.signature ILIKE ${next} ESCAPE '\\' \
                     OR e.scoped_name ILIKE ${next} ESCAPE '\\')"
                ));
                next += 1;
            }
            for _ in &excluded {
                sql.push_str(&format!(" AND f.path <> ${next}"));
                next += 1;
            }
            sql.push_str(&format!(" ORDER BY e.id LIMIT ${next}"));
            let mut boxed: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
            boxed.push(Box::new(project_id));
            boxed.push(Box::new(epoch));
            for token in &tokens {
                boxed.push(Box::new(format!("%{token}%")));
            }
            for path in &excluded {
                boxed.push(Box::new(path.clone()));
            }
            boxed.push(Box::new(limit));
            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                boxed.iter().map(|item| item.as_ref() as _).collect();
            let client = self.pooled().await?;
            let rows = client.query(&sql, &refs).await.map_err(classify_pg)?;
            rows.iter().map(crate::rows::entity_from_row).collect()
        })
        .await
    }

    /// List file rows of one project across the given epochs, ordered by path.
    ///
    /// An empty epoch list returns no rows; callers always scope explicitly.
    pub(crate) async fn files_list(
        &self,
        project_id: i64,
        epochs: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        if epochs.is_empty() {
            return Ok(Vec::new());
        }
        let epochs_vec = epochs.to_vec();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {} FROM files WHERE project_id = $1 AND epoch = ANY($2) ORDER BY path",
                        crate::rows::FILE_COLUMNS
                    ),
                    &[&project_id, &epochs_vec],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(crate::rows::file_from_row).collect()
        })
        .await
    }
}

impl PostgresClient {
    pub(crate) async fn chunks_page_with_category(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let columns = CHUNK_COLUMNS
                .split(',')
                .map(|column| format!("c.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = client
                .query(
                    &format!(
                        "SELECT {columns}, COALESCE(f.category, 4) AS file_category \
                         FROM chunks c LEFT JOIN files f \
                           ON f.project_id = c.project_id AND f.epoch = c.epoch \
                           AND f.path = c.file_path \
                         WHERE c.project_id = $1 AND c.epoch = $2 AND c.path = 'emb' \
                         ORDER BY c.file_path, c.start_line LIMIT $3 OFFSET $4"
                    ),
                    &[&project_id, &epoch, &limit, &offset],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| {
                    let category: i64 = row.try_get("file_category").map_err(classify_pg)?;
                    Ok((chunk_from_row(row)?, category.max(0) as u8))
                })
                .collect()
        })
        .await
    }

    pub(crate) async fn files_page(
        &self,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {FILE_COLUMNS} FROM files \
                         WHERE project_id = $1 AND epoch = $2 ORDER BY id LIMIT $3 OFFSET $4"
                    ),
                    &[&project_id, &epoch, &limit, &offset],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(file_from_row).collect()
        })
        .await
    }

    pub(crate) async fn files_visible_hashes(
        &self,
        project_id: i64,
    ) -> Result<Vec<FileHashEntry>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let mut own_epoch: i64 = client
                .query_opt(
                    "SELECT value FROM project_meta \
                     WHERE project_id = $1 AND key = 'active_epoch'",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?
                .map(|row| row.get::<_, String>(0))
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            let manifest: Option<(i64, Option<i64>)> = client
                .query_opt(
                    "SELECT data_epoch, parent_data_epoch FROM project_index_manifests \
                     WHERE project_id = $1 AND state = 'active' \
                     ORDER BY publication_epoch DESC LIMIT 1",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?
                .map(|row| (row.get(0), row.get(1)));
            let mut ancestors = Vec::new();
            let mut excluded = Vec::new();
            if let Some((epoch, parent)) = manifest {
                own_epoch = epoch;
                let mut current = parent;
                while ancestors.len() < 2
                    && let Some(epoch) = current
                    && epoch > 0
                {
                    ancestors.push(epoch);
                    current = client
                        .query_opt(
                            "SELECT parent_data_epoch FROM project_index_manifests \
                             WHERE project_id = $1 AND data_epoch = $2 \
                             ORDER BY publication_epoch DESC LIMIT 1",
                            &[&project_id, &epoch],
                        )
                        .await
                        .map_err(classify_pg)?
                        .and_then(|row| row.get(0));
                }
                let rows = client
                    .query(
                        "SELECT file_path FROM generation_overrides \
                         WHERE project_id = $1 AND epoch = $2",
                        &[&project_id, &epoch],
                    )
                    .await
                    .map_err(classify_pg)?;
                excluded = rows
                    .into_iter()
                    .map(|row| row.get::<_, String>(0))
                    .collect();
            }

            let mut visible: std::collections::HashMap<String, Option<String>> =
                std::collections::HashMap::new();
            for epoch in ancestors.iter().rev() {
                load_file_hashes(&client, project_id, *epoch, &mut visible).await?;
            }
            for path in &excluded {
                visible.remove(path);
            }
            load_file_hashes(&client, project_id, own_epoch, &mut visible).await?;
            Ok(visible
                .into_iter()
                .map(|(path, content_hash)| FileHashEntry { path, content_hash })
                .collect())
        })
        .await
    }

    pub(crate) async fn files_record_hashes(
        &self,
        project_id: i64,
        epoch: i64,
        entries: &[(String, String)],
    ) -> Result<usize, StorageError> {
        let entries = entries.to_vec();
        if entries.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let now = chrono::Utc::now().timestamp();
            for (path, hash) in &entries {
                tx.execute(
                    "INSERT INTO files (path, language, last_modified, created_at, \
                     project_id, content_hash, epoch) \
                     VALUES ($1, 'unknown', $2, $2, $3, $4, $5) \
                     ON CONFLICT (project_id, epoch, path) DO UPDATE SET \
                        content_hash = EXCLUDED.content_hash, \
                        last_modified = EXCLUDED.last_modified",
                    &[path, &now, &project_id, hash, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(entries.len())
        })
        .await
    }

    pub(crate) async fn files_remove_hashes(
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
            let mut removed = 0u64;
            for path in &paths {
                removed += tx
                    .execute(
                        "DELETE FROM files WHERE project_id = $1 AND epoch = $2 AND path = $3",
                        &[&project_id, &epoch, path],
                    )
                    .await
                    .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn files_ensure(
        &self,
        project_id: i64,
        epoch: i64,
        batch_id: i64,
        entries: &[FileEnsureEntry],
    ) -> Result<usize, StorageError> {
        let entries = entries.to_vec();
        if entries.is_empty() {
            return Ok(0);
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            for entry in &entries {
                tx.execute(
                    "INSERT INTO files (path, language, category, last_modified, created_at, \
                     project_id, content_hash, epoch, batch_id) \
                     VALUES ($1, $2, $3, $4, $4, $5, NULL, $6, $7) \
                     ON CONFLICT (project_id, epoch, path) DO UPDATE SET \
                        language = EXCLUDED.language, category = EXCLUDED.category, \
                        last_modified = EXCLUDED.last_modified, batch_id = EXCLUDED.batch_id",
                    &[
                        &entry.path,
                        &entry.language,
                        &(entry.category as i64),
                        &entry.last_modified,
                        &project_id,
                        &epoch,
                        &batch_id,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(entries.len())
        })
        .await
    }

    pub(crate) async fn files_delete_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM files WHERE project_id = $1 AND path = $2",
                    &[&project_id, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn files_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<FileRecord>, StorageError> {
        let ids = ids.to_vec();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {FILE_COLUMNS} FROM files WHERE project_id = $1 AND id = ANY($2)"
                    ),
                    &[&project_id, &ids],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(file_from_row).collect()
        })
        .await
    }

    pub(crate) async fn file_by_path(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<Option<FileRecord>, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {FILE_COLUMNS} FROM files \
                         WHERE project_id = $1 AND path = $2 ORDER BY id DESC LIMIT 1"
                    ),
                    &[&project_id, &path],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| file_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn files_epoch_entries(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FileEpochEntry>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT path, batch_id, content_hash FROM files \
                     WHERE project_id = $1 AND epoch = $2 ORDER BY path",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| {
                    Ok(FileEpochEntry {
                        path: row.try_get("path").map_err(classify_pg)?,
                        batch_id: row.try_get("batch_id").map_err(classify_pg)?,
                        content_hash: row.try_get("content_hash").map_err(classify_pg)?,
                    })
                })
                .collect()
        })
        .await
    }

    pub(crate) async fn files_set_epoch(
        &self,
        project_id: i64,
        path: &str,
        epoch: i64,
    ) -> Result<bool, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "UPDATE files SET epoch = $1 WHERE project_id = $2 AND path = $3",
                    &[&epoch, &project_id, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed > 0)
        })
        .await
    }

    pub(crate) async fn files_count(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<i64, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let count: i64 = client
                .query_one(
                    "SELECT COUNT(*) FROM files WHERE project_id = $1 AND epoch = $2",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(count)
        })
        .await
    }

    pub(crate) async fn files_exists_at_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<bool, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let exists: bool = client
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM files \
                     WHERE project_id = $1 AND epoch = $2 AND path = $3)",
                    &[&project_id, &epoch, &path],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(exists)
        })
        .await
    }

    pub(crate) async fn entities_insert_returning_ids(
        &self,
        entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError> {
        let entities = entities.to_vec();
        if entities.is_empty() {
            return Ok(Vec::new());
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            let mut ids = Vec::with_capacity(entities.len());
            for entity in &entities {
                let id: i64 = tx
                    .query_one(
                        "INSERT INTO entities (name, kind, file_id, signature, span_start_row, \
                         span_end_row, span_start_column, span_end_column, span_start_byte, \
                         span_end_byte, scoped_name, overload_discriminator, depth, parent_id, \
                         metadata, parameters_json, return_type, doc_comment, modifiers_json, \
                         project_id, epoch, batch_id, rank) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                         $15, $16, $17, $18, $19, $20, $21, $22, $23) \
                         ON CONFLICT (project_id, epoch, file_id, scoped_name, kind, \
                         overload_discriminator) DO UPDATE SET name = EXCLUDED.name \
                         RETURNING id",
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
                            &entity.parent_id,
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
                    .get(0);
                ids.push(id);
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok(ids)
        })
        .await
    }

    pub(crate) async fn entities_with_files(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<EntityWithFile>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let columns = ENTITY_COLUMNS
                .split(',')
                .map(|column| format!("e.{}", column.trim()))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = client
                .query(
                    &format!(
                        "SELECT {columns}, f.path AS file_path FROM entities e \
                         JOIN files f ON f.id = e.file_id \
                         WHERE e.project_id = $1 AND e.epoch = $2 ORDER BY e.id"
                    ),
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| {
                    Ok(EntityWithFile {
                        entity: entity_from_row(row)?,
                        file_path: row.try_get("file_path").map_err(classify_pg)?,
                    })
                })
                .collect()
        })
        .await
    }

    pub(crate) async fn entities_by_ids(
        &self,
        project_id: i64,
        ids: &[i64],
    ) -> Result<Vec<EntityRecord>, StorageError> {
        let ids = ids.to_vec();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {ENTITY_COLUMNS} FROM entities \
                         WHERE project_id = $1 AND id = ANY($2) ORDER BY id"
                    ),
                    &[&project_id, &ids],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(entity_from_row).collect()
        })
        .await
    }

    pub(crate) async fn entities_by_file(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {ENTITY_COLUMNS} FROM entities \
                         WHERE project_id = $1 AND file_id = $2 ORDER BY id"
                    ),
                    &[&project_id, &file_id],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(entity_from_row).collect()
        })
        .await
    }

    pub(crate) async fn entities_delete_by_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<bool, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM entities WHERE id = $1 AND project_id = $2",
                    &[&entity_id, &project_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed > 0)
        })
        .await
    }

    pub(crate) async fn entities_delete_by_file(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM entities WHERE file_id = $1 AND epoch = $2",
                    &[&file_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn entities_set_parent(
        &self,
        project_id: i64,
        epoch: i64,
        child_id: i64,
        parent_id: i64,
    ) -> Result<(), StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            client
                .execute(
                    "UPDATE entities SET parent_id = $1 \
                     WHERE id = $2 AND project_id = $3 AND epoch = $4",
                    &[&parent_id, &child_id, &project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn entity_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<i64>, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT e.id FROM entities e JOIN files f ON f.id = e.file_id \
                     WHERE e.project_id = $1 AND e.epoch = $2 AND f.path = $3 ORDER BY e.id",
                    &[&project_id, &epoch, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows.into_iter().map(|row| row.get::<_, i64>(0)).collect())
        })
        .await
    }

    pub(crate) async fn chunks_by_file_epoch(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    &format!(
                        "SELECT {CHUNK_COLUMNS} FROM chunks \
                         WHERE project_id = $1 AND file_path = $2 AND epoch = $3 ORDER BY chunk_id"
                    ),
                    &[&project_id, &file_path, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(chunk_from_row).collect()
        })
        .await
    }

    pub(crate) async fn chunks_by_file(
        &self,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<Vec<String>, StorageError> {
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT chunk_id FROM chunks \
                     WHERE project_id = $1 AND file_path = $2 AND epoch = $3 ORDER BY chunk_id",
                    &[&project_id, &file_path, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows
                .into_iter()
                .map(|row| row.get::<_, String>(0))
                .collect())
        })
        .await
    }

    pub(crate) async fn chunks_content_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<ChunkContent>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT chunk_id, content, bm25_keywords FROM chunks \
                     WHERE project_id = $1 AND epoch = $2 ORDER BY file_path, chunk_id",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(chunk_content_from_row).collect()
        })
        .await
    }

    pub(crate) async fn chunks_stats(
        &self,
        project_id: i64,
        epoch: i64,
        channel: &str,
    ) -> Result<(i64, i64), StorageError> {
        let channel = channel.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_one(
                    "SELECT COUNT(*), COALESCE(SUM(truncated), 0) FROM chunks \
                     WHERE project_id = $1 AND epoch = $2 AND path = $3",
                    &[&project_id, &epoch, &channel],
                )
                .await
                .map_err(classify_pg)?;
            Ok((row.get(0), row.get(1)))
        })
        .await
    }

    pub(crate) async fn chunks_max_entity_id(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<u64>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT entity_ids FROM chunks WHERE project_id = $1 AND epoch = $2 \
                     AND entity_ids <> '' AND entity_ids <> '[]'",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            let mut max_id: Option<u64> = None;
            for row in rows {
                let raw: String = row.get(0);
                if let Ok(ids) = serde_json::from_str::<Vec<i64>>(&raw) {
                    for id in ids {
                        if id >= 0 {
                            max_id = Some(max_id.map_or(id as u64, |value| value.max(id as u64)));
                        }
                    }
                }
            }
            Ok(max_id)
        })
        .await
    }

    pub(crate) async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        path: &str,
    ) -> Result<usize, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND file_path = $2",
                    &[&project_id, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn chunks_and_mappings_upsert(
        &self,
        chunks: &[ChunkRecord],
        mappings: &[EntityDetailMapping],
    ) -> Result<(usize, usize), StorageError> {
        let chunks = chunks.to_vec();
        let mappings = mappings.to_vec();
        if chunks.is_empty() && mappings.is_empty() {
            return Ok((0, 0));
        }
        self.run(async move {
            let mut client = self.pooled().await?;
            let tx = client.transaction().await.map_err(classify_pg)?;
            for chunk in &chunks {
                insert_chunk_in(&tx, chunk).await?;
            }
            for mapping in &mappings {
                insert_mapping_in(&tx, mapping).await?;
            }
            tx.commit().await.map_err(classify_pg)?;
            Ok((chunks.len(), mappings.len()))
        })
        .await
    }

    pub(crate) async fn chunks_count_by_path(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<i64, StorageError> {
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let count: i64 = client
                .query_one(
                    "SELECT COUNT(*) FROM chunks WHERE project_id = $1 AND file_path = $2",
                    &[&project_id, &file_path],
                )
                .await
                .map_err(classify_pg)?
                .get(0);
            Ok(count)
        })
        .await
    }

    pub(crate) async fn mappings_by_entity(
        &self,
        project_id: i64,
        entity_id: i64,
        epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    &format!(
                        "SELECT {DETAIL_MAPPING_COLUMNS} FROM entity_detail_mappings \
                         WHERE project_id = $1 AND entity_id = $2 AND epoch = $3 LIMIT 1"
                    ),
                    &[&project_id, &entity_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| detail_mapping_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn mappings_point_ids_by_file_epoch(
        &self,
        project_id: i64,
        epoch: i64,
        path: &str,
    ) -> Result<Vec<(i64, String)>, StorageError> {
        let path = path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT m.entity_id, m.qdrant_point_ids FROM entity_detail_mappings m \
                     JOIN entities e ON e.id = m.entity_id \
                     JOIN files f ON f.id = e.file_id \
                     WHERE m.project_id = $1 AND m.epoch = $2 AND f.path = $3 \
                     ORDER BY m.entity_id",
                    &[&project_id, &epoch, &path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(rows
                .into_iter()
                .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)))
                .collect())
        })
        .await
    }

    pub(crate) async fn mappings_delete_by_file_id(
        &self,
        project_id: i64,
        file_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM entity_detail_mappings WHERE project_id = $1 \
                     AND entity_id IN (SELECT id FROM entities WHERE file_id = $2)",
                    &[&project_id, &file_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn mappings_delete_by_entity_id(
        &self,
        project_id: i64,
        entity_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let changed = client
                .execute(
                    "DELETE FROM entity_detail_mappings \
                     WHERE project_id = $1 AND entity_id = $2",
                    &[&project_id, &entity_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(changed as usize)
        })
        .await
    }

    pub(crate) async fn checkpoint_cached_parsed_data(
        &self,
        project_id: i64,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let file_path = file_path.to_string();
        let content_hash = content_hash.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT cf.parsed_data FROM checkpoint_file cf \
                     JOIN checkpoint c ON cf.operation_id = c.operation_id \
                       AND c.project_id = cf.project_id \
                     WHERE cf.project_id = $1 AND cf.file_path = $2 \
                       AND cf.content_hash = $3 AND cf.parsed_data IS NOT NULL \
                       AND c.status = 'in_progress' \
                     ORDER BY cf.id DESC LIMIT 1",
                    &[&project_id, &file_path, &content_hash],
                )
                .await
                .map_err(classify_pg)?;
            Ok(row.map(|row| row.get::<_, Vec<u8>>(0)))
        })
        .await
    }
}

async fn load_file_hashes(
    client: &deadpool_postgres::Client,
    project_id: i64,
    epoch: i64,
    out: &mut std::collections::HashMap<String, Option<String>>,
) -> Result<(), StorageError> {
    let rows = client
        .query(
            "SELECT path, content_hash FROM files WHERE project_id = $1 AND epoch = $2",
            &[&project_id, &epoch],
        )
        .await
        .map_err(classify_pg)?;
    for row in rows {
        let path: String = row.get(0);
        out.insert(path, row.get(1));
    }
    Ok(())
}

async fn insert_chunk_in(
    tx: &tokio_postgres::Transaction<'_>,
    chunk: &ChunkRecord,
) -> Result<(), StorageError> {
    let project_id: Option<i64> = chunk.project_id;
    tx.execute(
        "INSERT INTO chunks (chunk_id, file_path, content, start_line, end_line, entity_ids, \
         entity_names, entity_kinds, group_title, chunk_type, test_status, test_source, \
         created_at, updated_at, project_id, epoch, batch_id, path, bm25_keywords, segment_id, \
         truncated) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
         $18, $19, $20, $21) \
         ON CONFLICT (project_id, epoch, file_path, chunk_id) DO UPDATE SET \
            content = EXCLUDED.content, updated_at = EXCLUDED.updated_at",
        &[
            &chunk.chunk_id,
            &chunk.file_path,
            &chunk.content,
            &chunk.start_line,
            &chunk.end_line,
            &chunk.entity_ids,
            &chunk.entity_names,
            &chunk.entity_kinds,
            &chunk.group_title,
            &chunk.chunk_type,
            &(chunk.test_status as i64),
            &(chunk.test_source as i64),
            &chunk.created_at,
            &chunk.updated_at,
            &project_id,
            &chunk.epoch,
            &chunk.batch_id,
            &chunk.path,
            &chunk.bm25_keywords,
            &chunk.segment_id,
            &(chunk.truncated as i64),
        ],
    )
    .await
    .map_err(classify_pg)?;
    Ok(())
}

async fn insert_mapping_in(
    tx: &tokio_postgres::Transaction<'_>,
    mapping: &EntityDetailMapping,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO entity_detail_mappings (entity_id, project_id, epoch, qdrant_point_ids, \
         bm25_doc_ids, chunk_count, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (project_id, epoch, entity_id) DO UPDATE SET \
            qdrant_point_ids = EXCLUDED.qdrant_point_ids, \
            bm25_doc_ids = EXCLUDED.bm25_doc_ids, \
            chunk_count = EXCLUDED.chunk_count, updated_at = EXCLUDED.updated_at",
        &[
            &mapping.entity_id,
            &mapping.project_id,
            &mapping.epoch,
            &mapping.qdrant_point_ids,
            &mapping.bm25_doc_ids,
            &mapping.chunk_count,
            &mapping.created_at,
            &mapping.updated_at,
        ],
    )
    .await
    .map_err(classify_pg)?;
    Ok(())
}

/// Sanitize an FTS5 query string into plain substring tokens.
///
/// Drops quoting, wildcards and boolean keywords; caps the token count so a
/// hostile query cannot fan out into an unbounded conjunction.
fn sanitize_fts_query(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|char| char == '"' || char == '\'' || char == '(' || char == ')')
                .trim_end_matches('*')
                .replace(['\\', '%', '_'], "")
        })
        .filter(|token| {
            !token.is_empty()
                && !matches!(
                    token.to_ascii_uppercase().as_str(),
                    "AND" | "OR" | "NOT" | "NEAR"
                )
        })
        .take(MAX_FTS_TOKENS)
        .collect()
}
