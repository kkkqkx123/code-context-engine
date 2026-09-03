//! External-store compaction: materializing inherited Qdrant and BM25
//! generations.
//!
//! Because tantivy `content`/`keywords` fields are index-only, the BM25 clone
//! reconstructs document text from SQLite, which remains the single source of
//! truth.

use std::collections::HashMap;

use cce_parser::summary::FileSummary;
use cce_storage_bm25::Bm25Document;
use cce_types::PointKind;

use crate::error::OrchestratorError;

use super::StorageCoordinator;

/// Content needed to reconstruct BM25 documents from SQLite when a
/// generation is materialized (compaction).
///
/// The tantivy `content` and `keywords` fields are index-only (not stored), so
/// the active generation's chunk text and keywords must be read back from
/// SQLite when cloning into a candidate epoch. Chunk docs resolve by
/// `chunk_id`; summary docs by file path.
struct Bm25CloneContent {
    /// `chunk_id -> chunk text` (from the `chunks` table).
    chunk_by_id: HashMap<String, String>,
    /// `chunk_id -> space-joined BM25 keywords` (from the `chunks` table).
    keywords_by_id: HashMap<String, String>,
    /// `file_path -> summary BM25 text` (rebuilt from `file_summaries`).
    summary_by_path: HashMap<String, String>,
}

impl StorageCoordinator {
    /// Materialize the external generations (Qdrant + BM25) inherited by
    /// `target_epoch` from `source_epoch`. Overridden files are skipped so
    /// the target's own newer points/documents are never shadowed. Only used
    /// by compaction.
    pub(crate) async fn materialize_external_epochs(
        &self,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), OrchestratorError> {
        if source_epoch <= 0 {
            return Ok(());
        }
        let is_excluded = |path: Option<&str>| -> bool {
            match path {
                Some(path) => excluded_paths.iter().any(|excluded| excluded == path),
                None => false,
            }
        };
        if let Some(qdrant) = &self.qdrant {
            self.ensure_project_group_id()?;
            let points = qdrant.scroll_all_points().await?;
            let cloned: Vec<_> = points
                .into_iter()
                .filter(|point| {
                    point.payload.group_id.as_deref() == Some(self.project_group_id.as_str())
                        && point.payload.epoch == Some(source_epoch)
                        && !is_excluded(Some(point.payload.file_path.as_str()))
                })
                .map(|mut point| {
                    point.payload.epoch = Some(target_epoch);
                    point.id = if point.payload.r#type == Some(PointKind::Summary) {
                        format!(
                            "{}::{}::summary::{}",
                            self.project_group_id, target_epoch, point.payload.file_path
                        )
                    } else {
                        format!(
                            "{}::{}::{}",
                            self.project_group_id, target_epoch, point.payload.source_id
                        )
                    };
                    point
                })
                .collect();
            if !cloned.is_empty() {
                qdrant.upsert_points(&cloned).await?;
            }
        }
        if let Some(bm25) = &self.bm25 {
            let mut client = bm25.lock().await;
            let documents = client
                .snapshot_documents(self.project_id, source_epoch)
                .await?;
            if documents.is_empty() && client.document_count_by_project(self.project_id).await? > 0
            {
                return Err(OrchestratorError::index(
                    "generation_compaction",
                    "BM25 inherited generation cannot be materialized; a full BM25 rebuild is required",
                ));
            }
            // content and keywords are index-only BM25 fields, so the snapshot
            // above carries neither. Reconstruct them from SQLite, which
            // remains the single source of truth: chunk docs from
            // `chunks.content` / `chunks.bm25_keywords`, and summary docs by
            // rebuilding `to_bm25_text()` from `file_summaries`.
            let clone_content = self.load_bm25_clone_content(source_epoch)?;
            let cloned: Vec<_> = documents
                .into_iter()
                .filter(|document| {
                    !is_excluded(document.fields.get("file_path").map(String::as_str))
                })
                .map(|document| {
                    let mut document = document;
                    if !document.fields.contains_key("content") {
                        let content = if document.fields.contains_key("chunk_id") {
                            document
                                .fields
                                .get("chunk_id")
                                .and_then(|id| clone_content.chunk_by_id.get(id).cloned())
                        } else {
                            document
                                .fields
                                .get("file_path")
                                .and_then(|path| clone_content.summary_by_path.get(path).cloned())
                        };
                        if let Some(content) = content {
                            document.fields.insert("content".to_string(), content);
                        }
                    }
                    if !document.fields.contains_key("keywords") {
                        if let Some(keywords) = document
                            .fields
                            .get("chunk_id")
                            .and_then(|id| clone_content.keywords_by_id.get(id).cloned())
                        {
                            document.fields.insert("keywords".to_string(), keywords);
                        }
                    }
                    let file_path = document.fields.get("file_path").cloned();
                    let chunk_id = document.fields.get("chunk_id").cloned();
                    let document_id = if let Some(chunk_id) = chunk_id {
                        format!("{}::{}::{}", self.project_id, target_epoch, chunk_id)
                    } else if let Some(file_path) = file_path {
                        format!(
                            "{}::{}::summary::{}",
                            self.project_id, target_epoch, file_path
                        )
                    } else {
                        document.document_id
                    };
                    let mut fields = document.fields;
                    fields.insert("project_id".to_string(), self.project_id.to_string());
                    fields.insert("epoch".to_string(), target_epoch.to_string());
                    Bm25Document {
                        document_id,
                        fields,
                    }
                })
                .collect();
            if !cloned.is_empty() {
                client.batch_index("default", &cloned).await?;
            }
        }
        Ok(())
    }

    /// Load the content needed to clone the active BM25 generation into a
    /// candidate epoch. Because content and keywords are index-only fields in
    /// tantivy, the snapshot cannot copy them; both chunk text/keywords and
    /// summary BM25 text are reconstructed from SQLite.
    fn load_bm25_clone_content(&self, epoch: i64) -> Result<Bm25CloneContent, OrchestratorError> {
        let mut chunk_content: HashMap<String, String> = HashMap::new();
        let mut chunk_keywords: HashMap<String, String> = HashMap::new();
        let mut summary_content: HashMap<String, String> = HashMap::new();
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(Bm25CloneContent {
                chunk_by_id: chunk_content,
                keywords_by_id: chunk_keywords,
                summary_by_path: summary_content,
            });
        };
        client
            .with_transaction(|tx| {
                {
                    let mut stmt = tx
                        .prepare(
                            "SELECT chunk_id, content, bm25_keywords FROM chunks
                             WHERE project_id = ?1 AND epoch = ?2",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = stmt
                        .query_map(rusqlite::params![self.project_id, epoch], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        })
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    for row in rows {
                        let (chunk_id, content, keywords) =
                            row.map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                        if !content.is_empty() {
                            chunk_content.insert(chunk_id.clone(), content);
                        }
                        if !keywords.is_empty() {
                            chunk_keywords.insert(chunk_id, keywords);
                        }
                    }
                }
                {
                    let mut stmt = tx
                        .prepare(
                            "SELECT files.path, fs.summary_json
                             FROM file_summaries fs
                             JOIN files ON files.id = fs.file_id
                             WHERE files.project_id = ?1 AND fs.epoch = ?2
                                AND fs.summary_json IS NOT NULL",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = stmt
                        .query_map(rusqlite::params![self.project_id, epoch], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        })
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    for row in rows {
                        let (file_path, summary_json) =
                            row.map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                        // The persisted blob is the canonical FileSummary;
                        // deserialize it instead of reassembling from columns.
                        match serde_json::from_str::<FileSummary>(&summary_json) {
                            Ok(summary) => {
                                let text = summary.to_bm25_text();
                                if !text.is_empty() {
                                    summary_content.insert(file_path, text);
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    path = %file_path,
                                    %error,
                                    "Skipping unreadable persisted summary during BM25 clone"
                                );
                            }
                        }
                    }
                }
                Ok(())
            })
            .map_err(OrchestratorError::Storage)?;
        Ok(Bm25CloneContent {
            chunk_by_id: chunk_content,
            keywords_by_id: chunk_keywords,
            summary_by_path: summary_content,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cce_storage_sqlite::{
        ChunkRecord, ChunkRepository, FileRepository, FileSummaryRepository, NewProjectRecord,
        ProjectRepository, SqliteClient,
    };

    use super::super::StorageCoordinator;

    #[test]
    fn load_bm25_clone_content_reconstructs_chunk_and_summary_text() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())
            })
            .expect("project should be inserted");

        client
            .with_transaction(|tx| {
                FileRepository::insert(
                    tx,
                    &cce_storage_sqlite::FileRecord {
                        id: 0,
                        path: "src/lib.rs".to_string(),
                        language: "rust".to_string(),
                        category: cce_types::FileCategory::Code.as_u8(),
                        last_modified: 1,
                        created_at: 1,
                        project_id: 1,
                        content_hash: None,
                    },
                )
                .map(|_| ())
            })
            .expect("file should be inserted");

        let chunk = ChunkRecord::new(
            "group_1_bm25_0".to_string(),
            "src/lib.rs".to_string(),
            "bm25 chunk text".to_string(),
            1,
            5,
        )
        .with_entity_ids(&[10, 20])
        .with_epoch(3)
        .with_project_id(1);
        client
            .with_transaction(|tx| ChunkRepository::insert_batch(tx, &[chunk]))
            .expect("chunk should be inserted");

        let summary = cce_parser::summary::FileSummary::new("src/lib.rs")
            .with_language("rust")
            .with_summary("handles file parsing")
            .with_entities(vec!["Parser".to_string()]);
        let summary_json = serde_json::to_string(&summary).expect("summary should serialize");
        client
            .with_transaction(|tx| {
                FileSummaryRepository::upsert_with_epoch(tx, 1, 3, &summary_json)
            })
            .expect("summary should be inserted");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        let clone_content = storage
            .load_bm25_clone_content(3)
            .expect("content should load from SQLite");

        assert_eq!(
            clone_content
                .chunk_by_id
                .get("group_1_bm25_0")
                .map(String::as_str),
            Some("bm25 chunk text")
        );
        let summary_text = clone_content
            .summary_by_path
            .get("src/lib.rs")
            .expect("summary bm25 text should be reconstructed");
        assert!(summary_text.contains("Summary: handles file parsing"));
        assert!(summary_text.contains("Entities: Parser"));
        assert!(summary_text.contains("File: src/lib.rs"));
    }
}
