use cce_types::StorageError;

use cce_storage_common::relation::{ChunkRecord, EntityDetailMapping, EntityRecord, FileRecord};

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
                     span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, \
                     return_type, doc_comment, modifiers_json, project_id, epoch, batch_id, rank) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                     $16, $17, $18, $19, $20, $21, $22) \
                     ON CONFLICT (project_id, epoch, file_id, scoped_name, kind) DO UPDATE SET \
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
                     entity_ids, entity_names, chunk_type, test_status, test_source, created_at, \
                     updated_at, project_id, epoch, batch_id, path, bm25_keywords, segment_id, \
                     truncated) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, \
                     $16, $17, $18, $19) \
                     ON CONFLICT (project_id, epoch, chunk_id) DO UPDATE SET \
                        file_path = EXCLUDED.file_path, content = EXCLUDED.content, \
                        start_line = EXCLUDED.start_line, end_line = EXCLUDED.end_line, \
                        entity_ids = EXCLUDED.entity_ids, entity_names = EXCLUDED.entity_names, \
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

    pub(crate) async fn chunks_delete_by_file(
        &self,
        project_id: i64,
        file_path: &str,
    ) -> Result<usize, StorageError> {
        let file_path = file_path.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM chunks WHERE project_id = $1 AND file_path = $2",
                    &[&project_id, &file_path],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
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

    pub(crate) async fn summary_upsert(
        &self,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<(), StorageError> {
        let summary_json = summary_json.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().to_rfc3339();
            client
                .execute(
                    "INSERT INTO file_summaries (file_id, epoch, summary_json, created_at, updated_at) \
                     VALUES ($1, $2, $3, $4, $4) \
                     ON CONFLICT (file_id, epoch) DO UPDATE SET \
                        summary_json = EXCLUDED.summary_json, updated_at = EXCLUDED.updated_at",
                    &[&file_id, &epoch, &summary_json, &now],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn summary_at_epoch(
        &self,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT summary_json FROM file_summaries WHERE file_id = $1 AND epoch = $2",
                    &[&file_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| row.try_get("summary_json").map_err(classify_pg))
                .transpose()
        })
        .await
    }

    pub(crate) async fn summaries_by_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT f.path, s.summary_json, s.file_id FROM file_summaries s \
                     JOIN files f ON f.id = s.file_id \
                     WHERE f.project_id = $1 AND s.epoch = $2 ORDER BY f.path",
                    &[&project_id, &epoch],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter()
                .map(|row| {
                    Ok((
                        row.try_get("path").map_err(classify_pg)?,
                        row.try_get("summary_json").map_err(classify_pg)?,
                        row.try_get("file_id").map_err(classify_pg)?,
                    ))
                })
                .collect()
        })
        .await
    }

    pub(crate) async fn summaries_delete_by_project_epoch(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM file_summaries WHERE epoch = $1 AND file_id IN \
                     (SELECT id FROM files WHERE project_id = $2)",
                    &[&epoch, &project_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }

    pub(crate) async fn summaries_delete_by_project(
        &self,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let removed = client
                .execute(
                    "DELETE FROM file_summaries WHERE file_id IN \
                     (SELECT id FROM files WHERE project_id = $1)",
                    &[&project_id],
                )
                .await
                .map_err(classify_pg)?;
            Ok(removed as usize)
        })
        .await
    }
}
