//! File summaries repository for CRUD operations
//!
//! This module provides repository operations for file summaries,
//! including persistent storage of FileSummary objects as JSON.

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::Value;

use cce_types::StorageError;

/// File summary data (JSON-based, decoupled from cce_parser::FileSummary)
#[derive(Debug, Clone)]
pub struct FileSummaryData {
    pub file_id: i64,
    pub summary_text: String,
    pub language: String,
    pub main_entities: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub tags: Vec<String>,
}

/// File summaries repository
pub struct FileSummaryRepository;

impl FileSummaryRepository {
    /// Upsert a file summary (insert or update)
    pub fn upsert(tx: &Transaction, file_id: i64, summary_json: &str) -> Result<i64, StorageError> {
        Self::upsert_with_epoch(tx, file_id, 0, summary_json)
    }

    /// Upsert a file summary at a specific epoch (insert or update)
    ///
    /// The JSON blob is the single source of truth; the structured columns
    /// are generated from it at read time. The payload is validated here so
    /// a malformed blob can never poison the row's generated columns.
    pub fn upsert_with_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
        summary_json: &str,
    ) -> Result<i64, StorageError> {
        let now = Utc::now().to_rfc3339();

        serde_json::from_str::<Value>(summary_json)
            .map_err(|e| StorageError::sqlite(format!("Invalid summary JSON: {}", e)))?;

        tx.execute(
            "INSERT INTO file_summaries (file_id, epoch, summary_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(file_id, epoch) DO UPDATE SET
                summary_json = excluded.summary_json,
                updated_at = excluded.updated_at",
            params![file_id, epoch, summary_json, now, now],
        )
        .map_err(|e| StorageError::insert(format!("Failed to upsert file summary: {}", e)))?;

        tx.query_row(
            "SELECT id FROM file_summaries WHERE file_id = ?1 AND epoch = ?2",
            params![file_id, epoch],
            |row| row.get(0),
        )
        .map_err(|e| StorageError::query(format!("Failed to get upserted file summary ID: {}", e)))
    }

    /// Get summary by file ID and epoch as JSON
    pub fn get_by_file_id(
        conn: &rusqlite::Connection,
        file_id: i64,
    ) -> Result<Option<String>, StorageError> {
        Self::get_by_file_id_at_epoch(conn, file_id, 0)
    }

    /// List the full persisted summary JSON of every file at one epoch.
    ///
    /// Returns `(file_path, summary_json, epoch)` triples; used by
    /// regeneration sweeps that must re-embed summaries in place without
    /// touching their SQLite rows.
    pub fn list_json_by_epoch(
        conn: &rusqlite::Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(String, String, i64)>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, s.summary_json, s.epoch
                 FROM file_summaries s
                 JOIN files f ON f.id = s.file_id
                 WHERE f.project_id = ?1 AND s.epoch = ?2 AND s.summary_json IS NOT NULL",
            )
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {e}")))?;
        let rows = stmt
            .query_map(params![project_id, epoch], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| StorageError::query(format!("Failed to query summaries: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("Failed to collect summaries: {e}")))?;
        Ok(rows)
    }

    /// Get summary by file ID and epoch as the persisted canonical JSON.
    ///
    /// Returns the exact blob written by [`Self::upsert_with_epoch`]; unlike
    /// the previous column-reconstruction path this preserves every field
    /// (real file path, category, test markers).
    pub fn get_by_file_id_at_epoch(
        conn: &rusqlite::Connection,
        file_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT summary_json FROM file_summaries
                 WHERE file_id = ? AND epoch = ? AND summary_json IS NOT NULL",
            )
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {}", e)))?;

        let result = stmt
            .query_row(params![file_id, epoch], |row| row.get::<_, String>(0))
            .optional()
            .map_err(|e| StorageError::query(format!("Failed to query summary: {}", e)))?;

        Ok(result)
    }

    /// Check if file has a summary (at any epoch)
    pub fn exists(conn: &rusqlite::Connection, file_id: i64) -> Result<bool, StorageError> {
        let mut stmt = conn
            .prepare("SELECT 1 FROM file_summaries WHERE file_id = ? LIMIT 1")
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {}", e)))?;

        let exists = stmt
            .exists(params![file_id])
            .map_err(|e| StorageError::query(format!("Failed to check existence: {}", e)))?;

        Ok(exists)
    }

    /// Delete summary by file ID (all epochs)
    pub fn delete_by_file_id(tx: &Transaction, file_id: i64) -> Result<(), StorageError> {
        tx.execute(
            "DELETE FROM file_summaries WHERE file_id = ?",
            params![file_id],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete file summary for file {}: {}",
                file_id, e
            ))
        })?;

        Ok(())
    }

    /// Delete summary by file ID at a specific epoch
    pub fn delete_by_file_id_at_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        tx.execute(
            "DELETE FROM file_summaries WHERE file_id = ? AND epoch = ?",
            params![file_id, epoch],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete file summary for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })?;

        Ok(())
    }

    /// Update Qdrant point ID for a file at a specific epoch
    pub fn update_qdrant_point_id(
        tx: &Transaction,
        file_id: i64,
        point_id: Option<String>,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET qdrant_point_id = ?, updated_at = ?
             WHERE file_id = ?",
            params![point_id, now, file_id],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update Qdrant point ID for file {}: {}",
                file_id, e
            ))
        })?;
        Ok(())
    }

    /// Update Qdrant point ID for a file at a specific epoch
    pub fn update_qdrant_point_id_at_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
        point_id: Option<String>,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET qdrant_point_id = ?, updated_at = ?
             WHERE file_id = ? AND epoch = ?",
            params![point_id, now, file_id, epoch],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update Qdrant point ID for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })?;
        Ok(())
    }

    /// Update BM25 document ID for a file
    pub fn update_bm25_doc_id(
        tx: &Transaction,
        file_id: i64,
        doc_id: Option<String>,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET bm25_doc_id = ?, updated_at = ?
             WHERE file_id = ?",
            params![doc_id, now, file_id],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update BM25 doc ID for file {}: {}",
                file_id, e
            ))
        })?;
        Ok(())
    }

    /// Update BM25 document ID for a file at a specific epoch
    pub fn update_bm25_doc_id_at_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
        doc_id: Option<String>,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET bm25_doc_id = ?, updated_at = ?
             WHERE file_id = ? AND epoch = ?",
            params![doc_id, now, file_id, epoch],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update BM25 doc ID for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })?;
        Ok(())
    }

    /// Clear Qdrant point ID for a file
    pub fn clear_qdrant_point_id(tx: &Transaction, file_id: i64) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET qdrant_point_id = NULL, updated_at = ?
             WHERE file_id = ?",
            params![now, file_id],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to clear Qdrant point ID for file {}: {}",
                file_id, e
            ))
        })?;
        Ok(())
    }

    /// Clear Qdrant point ID for a file at a specific epoch
    pub fn clear_qdrant_point_id_at_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET qdrant_point_id = NULL, updated_at = ?
             WHERE file_id = ? AND epoch = ?",
            params![now, file_id, epoch],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to clear Qdrant point ID for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })?;
        Ok(())
    }

    /// Clear BM25 document ID for a file
    pub fn clear_bm25_doc_id(tx: &Transaction, file_id: i64) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET bm25_doc_id = NULL, updated_at = ?
             WHERE file_id = ?",
            params![now, file_id],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to clear BM25 doc ID for file {}: {}",
                file_id, e
            ))
        })?;
        Ok(())
    }

    /// Clear BM25 document ID for a file at a specific epoch
    pub fn clear_bm25_doc_id_at_epoch(
        tx: &Transaction,
        file_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE file_summaries
             SET bm25_doc_id = NULL, updated_at = ?
             WHERE file_id = ? AND epoch = ?",
            params![now, file_id, epoch],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to clear BM25 doc ID for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })?;
        Ok(())
    }

    /// Count total summaries
    pub fn count(conn: &rusqlite::Connection) -> Result<i64, StorageError> {
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM file_summaries")
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {}", e)))?;

        let count: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| StorageError::query(format!("Failed to count summaries: {}", e)))?;

        Ok(count)
    }

    /// Get summaries by language, as their persisted canonical JSON.
    ///
    /// `language` is a generated column extracted from the JSON payload, so
    /// this filter works without any duplicated storage.
    pub fn get_by_language(
        conn: &rusqlite::Connection,
        language: &str,
        limit: usize,
    ) -> Result<Vec<String>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT summary_json FROM file_summaries
                 WHERE language = ? AND summary_json IS NOT NULL LIMIT ?",
            )
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {}", e)))?;

        let summaries = stmt
            .query_map(params![language, limit as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| StorageError::query(format!("Failed to execute query: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("Failed to collect results: {}", e)))?;

        Ok(summaries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_valid_json() {
        let json = serde_json::json!({
            "summary_text": "Test summary",
            "language": "rust",
            "main_entities": ["fn main"],
            "imports": ["std"],
            "exports": [],
            "tags": ["entry_point"],
            "entity_count": 1,
            "line_count": 100
        })
        .to_string();

        // Just verify the JSON is valid
        assert!(serde_json::from_str::<Value>(&json).is_ok());
    }

    /// Seed a project and one file; returns the file row id.
    fn seeded_file(client: &crate::SqliteClient, path: &str) -> i64 {
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (name, root_path, created_at, updated_at) \
                     VALUES ('probe', '/probe', 0, 0)",
                    [],
                )
                .map_err(|e| StorageError::insert(e.to_string()))?;
                tx.execute(
                    "INSERT INTO files (path, language, last_modified, created_at, project_id, \
                     content_hash, epoch, batch_id) VALUES (?1, 'rust', 1, 1, 1, 'hash', 3, 0)",
                    rusqlite::params![path],
                )
                .map_err(|e| StorageError::insert(e.to_string()))?;
                Ok(tx.last_insert_rowid())
            })
            .expect("seed project + file")
    }

    #[test]
    fn upsert_then_get_roundtrips_canonical_json() {
        use crate::SqliteClient;

        let client = SqliteClient::in_memory().expect("db");
        let file_id = seeded_file(&client, "src/lib.rs");

        let canonical = serde_json::json!({
            "file_path": "src/lib.rs",
            "language": "rust",
            "summary_text": "handles parsing",
            "main_entities": ["Parser"],
            "imports": [],
            "exports": [],
            "tags": [],
            "entity_count": 1,
            "line_count": 10,
            "file_doc_comment": null,
            "importance_level": "Medium",
            "test_info": { "is_test": false, "source": "none" }
        })
        .to_string();

        client
            .with_transaction(|tx| {
                FileSummaryRepository::upsert_with_epoch(tx, file_id, 3, &canonical)
            })
            .expect("upsert");

        let conn = client.read_connection().expect("conn");
        let got = FileSummaryRepository::get_by_file_id_at_epoch(&conn, file_id, 3).expect("get");
        assert_eq!(got.as_deref(), Some(canonical.as_str()));

        // The generated columns expose the payload for filtering without
        // duplicating it in storage.
        assert_eq!(
            FileSummaryRepository::get_by_language(&conn, "rust", 10).expect("by language"),
            vec![canonical.clone()]
        );
        assert!(
            FileSummaryRepository::list_json_by_epoch(&conn, 1, 3)
                .expect("list")
                .iter()
                .any(|(path, json, epoch)| path == "src/lib.rs"
                    && json == &canonical
                    && *epoch == 3)
        );
    }

    #[test]
    fn upsert_rejects_malformed_json() {
        use crate::SqliteClient;

        let client = SqliteClient::in_memory().expect("db");
        let file_id = seeded_file(&client, "src/broken.rs");

        let failed = client
            .with_transaction(|tx| {
                FileSummaryRepository::upsert_with_epoch(tx, file_id, 0, "{not json")
            })
            .unwrap_err();
        assert!(failed.to_string().contains("Invalid summary JSON"));
    }

    #[test]
    fn second_upsert_updates_in_place() {
        use crate::SqliteClient;

        let client = SqliteClient::in_memory().expect("db");
        let file_id = seeded_file(&client, "src/twice.rs");

        let first = serde_json::json!({"summary_text": "before"}).to_string();
        let second = serde_json::json!({"summary_text": "after"}).to_string();
        client
            .with_transaction(|tx| FileSummaryRepository::upsert_with_epoch(tx, file_id, 0, &first))
            .expect("first upsert");
        client
            .with_transaction(|tx| {
                FileSummaryRepository::upsert_with_epoch(tx, file_id, 0, &second)
            })
            .expect("second upsert");

        let count: i64 = client
            .read_connection()
            .expect("conn")
            .query_row(
                "SELECT COUNT(*) FROM file_summaries WHERE file_id = ?1",
                [file_id],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(
            count, 1,
            "the same (file_id, epoch) must not duplicate rows"
        );

        let got = FileSummaryRepository::get_by_file_id_at_epoch(
            &client.read_connection().expect("conn"),
            file_id,
            0,
        )
        .expect("get");
        assert_eq!(got.as_deref(), Some(second.as_str()));
    }
}
