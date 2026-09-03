//! Chunk repository for SQLite storage
//!
//! This module provides repository operations for Chunk records,
//! which store the actual content of code chunks separately from
//! Qdrant vector storage to minimize payload size.
//!
//! # Persistence Strategy
//!
//! By storing chunk content in SQLite instead of Qdrant:
//! - Reduces vector storage size by ~60-70%
//! - Allows content to be updated without re-embedding
//! - Enables efficient full-text search via BM25
//!
//! # Design Principles
//!
//! 1. **Primary Source of Truth**: The in-memory chunk data is the primary source
//!    during runtime. SQLite provides persistence across restarts.
//!
//! 2. **Batch Operations**: Chunks are persisted in batches during indexing,
//!    not individually, to minimize database round-trips.
//!
//! 3. **Project-Scoped Queries**: All query methods should include `project_id`
//!    to leverage composite indexes and avoid full table scans.
//!
//! 4. **Hot Update Support**: When files are updated, chunks are deleted by file_path
//!    and re-inserted, ensuring consistency between memory and disk.

use rusqlite::{Connection, Row, params};

use crate::helpers::{execute_count, execute_query, execute_query_optional, execute_update};
use crate::types::ChunkRecord;
use cce_types::StorageError;

/// Chunk repository for CRUD operations
pub struct ChunkRepository;

impl ChunkRepository {
    /// Insert a single chunk record
    pub fn insert(tx: &rusqlite::Transaction, chunk: &ChunkRecord) -> Result<(), StorageError> {
        execute_update(
            tx,
            "INSERT INTO chunks (
                chunk_id, file_path, content,
                start_line, end_line, entity_ids, entity_names,
                chunk_type, test_status, test_source,
                created_at, updated_at, project_id, epoch, batch_id, path,
                bm25_keywords,
                segment_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(project_id, epoch, chunk_id) DO UPDATE SET
                file_path = excluded.file_path,
                content = excluded.content,
                start_line = excluded.start_line,
                end_line = excluded.end_line,
                entity_ids = excluded.entity_ids,
                entity_names = excluded.entity_names,
                chunk_type = excluded.chunk_type,
                test_status = excluded.test_status,
                test_source = excluded.test_source,
                updated_at = excluded.updated_at,
                project_id = excluded.project_id,
                epoch = excluded.epoch,
                batch_id = excluded.batch_id,
                path = excluded.path,
                bm25_keywords = excluded.bm25_keywords,
                segment_id = excluded.segment_id",
            params![
                chunk.chunk_id,
                chunk.file_path,
                chunk.content,
                chunk.start_line,
                chunk.end_line,
                chunk.entity_ids,
                chunk.entity_names,
                chunk.chunk_type,
                chunk.test_status,
                chunk.test_source,
                chunk.created_at,
                chunk.updated_at,
                chunk.project_id,
                chunk.epoch,
                chunk.batch_id,
                chunk.path,
                chunk.bm25_keywords,
                chunk.segment_id,
            ],
            "insert chunk",
        )
    }

    /// Insert multiple chunk records
    pub fn insert_batch(
        tx: &rusqlite::Transaction,
        chunks: &[ChunkRecord],
    ) -> Result<(), StorageError> {
        if chunks.is_empty() {
            return Ok(());
        }

        let mut stmt = tx
            .prepare(
                "INSERT INTO chunks (
                chunk_id, file_path, content,
                start_line, end_line, entity_ids, entity_names,
                chunk_type, test_status, test_source,
                created_at, updated_at, project_id, epoch, batch_id, path,
                bm25_keywords,
                segment_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(project_id, epoch, chunk_id) DO UPDATE SET
                file_path = excluded.file_path,
                content = excluded.content,
                start_line = excluded.start_line,
                end_line = excluded.end_line,
                entity_ids = excluded.entity_ids,
                entity_names = excluded.entity_names,
                chunk_type = excluded.chunk_type,
                test_status = excluded.test_status,
                test_source = excluded.test_source,
                updated_at = excluded.updated_at,
                project_id = excluded.project_id,
                epoch = excluded.epoch,
                batch_id = excluded.batch_id,
                path = excluded.path,
                bm25_keywords = excluded.bm25_keywords,
                segment_id = excluded.segment_id",
            )
            .map_err(|e| StorageError::insert(format!("Failed to prepare statement: {}", e)))?;

        for chunk in chunks {
            stmt.execute(params![
                chunk.chunk_id,
                chunk.file_path,
                chunk.content,
                chunk.start_line,
                chunk.end_line,
                chunk.entity_ids,
                chunk.entity_names,
                chunk.chunk_type,
                chunk.test_status,
                chunk.test_source,
                chunk.created_at,
                chunk.updated_at,
                chunk.project_id,
                chunk.epoch,
                chunk.batch_id,
                chunk.path,
                chunk.bm25_keywords,
                chunk.segment_id,
            ])
            .map_err(|e| StorageError::insert(format!("Failed to insert chunk: {}", e)))?;
        }

        Ok(())
    }

    /// Get a chunk by its ID (project-scoped)
    ///
    /// # Performance Note
    /// Uses composite index `idx_chunks_project_chunk_id` for efficient lookup.
    /// Always provide project_id to avoid cross-project scans.
    pub fn get_by_id(
        conn: &Connection,
        chunk_id: &str,
        project_id: i64,
    ) -> Result<Option<ChunkRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT chunk_id, file_path, content,
                    start_line, end_line, entity_ids, entity_names,
                    chunk_type, test_status, test_source,
                    created_at, updated_at, project_id, epoch, batch_id, path,
                    bm25_keywords, segment_id
             FROM chunks
             WHERE chunk_id = ?1 AND project_id = ?2
               AND epoch = (
                   SELECT MAX(epoch) FROM chunks
                   WHERE chunk_id = ?1 AND project_id = ?2
               )",
            params![chunk_id, project_id],
            Self::map_row,
        )
    }

    /// Get chunks by file path (project-scoped)
    ///
    /// # Performance Note
    /// Uses composite index `idx_chunks_project_file` for O(log n) lookup.
    /// Always provide project_id to avoid cross-project scans.
    pub fn get_by_file_and_project(
        conn: &Connection,
        file_path: &str,
        project_id: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT chunk_id, file_path, content,
                    start_line, end_line, entity_ids, entity_names,
                    chunk_type, test_status, test_source,
                    created_at, updated_at, project_id, epoch, batch_id, path,
                    bm25_keywords, segment_id
             FROM chunks
             WHERE file_path = ?1 AND project_id = ?2 AND path = 'emb'
               AND epoch = (
                   SELECT MAX(epoch) FROM chunks
                   WHERE file_path = ?1 AND project_id = ?2
               )
              ORDER BY start_line",
            params![file_path, project_id],
            Self::map_row,
        )
    }

    /// Get chunks by project ID with pagination
    ///
    /// # Performance Note
    /// Uses index `idx_chunks_project` for efficient lookup.
    /// Use this method instead of loading all chunks at once.
    pub fn get_by_project_id_paged(
        conn: &Connection,
        project_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT chunk_id, file_path, content,
                    start_line, end_line, entity_ids, entity_names,
                    chunk_type, test_status, test_source,
                    created_at, updated_at, project_id, epoch, batch_id, path,
                    bm25_keywords, segment_id
             FROM chunks WHERE project_id = ?1 AND path = 'emb'
              ORDER BY file_path, start_line LIMIT ?2 OFFSET ?3",
            params![project_id, limit, offset],
            Self::map_row,
        )
    }

    /// Get embedding-path chunks of one exact generation with pagination,
    /// joined with their file-level category.
    ///
    /// Unlike [`Self::get_by_project_id_paged`] this filters on a specific
    /// epoch so regeneration sweeps never mix parent and candidate rows. The
    /// category is a `files`-table attribute; the join resolves it per chunk
    /// through `(project_id, epoch, path)` instead of duplicating it on
    /// chunk rows.
    pub fn get_by_project_and_epoch_with_category_paged(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(ChunkRecord, u8)>, StorageError> {
        execute_query(
            conn,
            "SELECT c.chunk_id, c.file_path, c.content,
                    c.start_line, c.end_line, c.entity_ids, c.entity_names,
                    c.chunk_type, c.test_status, c.test_source,
                    c.created_at, c.updated_at, c.project_id, c.epoch,
                    c.batch_id, c.path, c.bm25_keywords, c.segment_id,
                    COALESCE(f.category, 4)
             FROM chunks c
             LEFT JOIN files f
               ON f.project_id = c.project_id AND f.epoch = c.epoch
              AND f.path = c.file_path
             WHERE c.project_id = ?1 AND c.path = 'emb' AND c.epoch = ?2
              ORDER BY c.file_path, c.start_line LIMIT ?3 OFFSET ?4",
            params![project_id, epoch, limit, offset],
            |row| {
                let record = Self::map_row(row)?;
                let category = row.get(18)?;
                Ok((record, category))
            },
        )
    }

    /// Get chunks by chunk IDs (project-scoped), optionally filtered by epoch
    ///
    /// # Performance Note
    /// Uses composite index `idx_chunks_project_chunk_id` for efficient lookup.
    /// Always provide project_id to avoid cross-project scans.
    /// Pass `epoch = Some(n)` for defense-in-depth version isolation
    /// (primary epoch filtering is done at the retrieval layer).
    pub fn get_by_chunk_ids(
        conn: &Connection,
        chunk_ids: &[String],
        project_id: i64,
        epoch: Option<i64>,
    ) -> Result<Vec<ChunkRecord>, StorageError> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let epoch_clause = epoch
            .map(|_| " AND epoch = ?".to_string())
            .unwrap_or_default();
        let query = format!(
            "SELECT chunk_id, file_path, content,
                    start_line, end_line, entity_ids, entity_names,
                    chunk_type, test_status, test_source,
                    created_at, updated_at, project_id, epoch, batch_id, path,
                    bm25_keywords, segment_id
             FROM chunks WHERE project_id = ?1 AND chunk_id IN ({}){}",
            placeholders, epoch_clause
        );

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id];
        for id in chunk_ids {
            params.push(id as &dyn rusqlite::ToSql);
        }
        if let Some(ref e) = epoch {
            params.push(e as &dyn rusqlite::ToSql);
        }

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {}", e)))?;

        let results = stmt
            .query_map(&params[..], Self::map_row)
            .map_err(|e| StorageError::query(format!("Failed to query chunks: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::query(format!("Failed to collect chunks: {}", e)))?;
        drop(stmt);
        Ok(results)
    }

    /// Delete chunks by file path (project-scoped)
    pub fn delete_by_file_path(
        tx: &rusqlite::Transaction,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM chunks WHERE file_path = ?1 AND project_id = ?2",
            params![file_path, project_id],
        )
        .map_err(|e| StorageError::delete(format!("Failed to delete chunks: {}", e)))
    }

    /// Delete a chunk by its ID (project-scoped)
    pub fn delete_by_id(
        tx: &rusqlite::Transaction,
        chunk_id: &str,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM chunks WHERE chunk_id = ?1 AND project_id = ?2",
            params![chunk_id, project_id],
        )
        .map_err(|e| StorageError::delete(format!("Failed to delete chunk: {}", e)))
    }

    /// Get chunk count by file path and project ID
    ///
    /// # Performance Note
    /// Uses composite index `idx_chunks_project_file` for efficient counting.
    pub fn count_by_file_and_project(
        conn: &Connection,
        file_path: &str,
        project_id: i64,
    ) -> Result<i64, StorageError> {
        execute_count(
            conn,
            "SELECT COUNT(*) FROM chunks WHERE file_path = ?1 AND project_id = ?2",
            params![file_path, project_id],
            "chunks",
        )
    }

    /// Compute the maximum raw entity ID referenced by any chunk in an epoch.
    ///
    /// Hot-update parses reuse the raw `EntityId` space of the previously
    /// indexed epoch. Seeding the parser counter one above this maximum keeps
    /// freshly parsed entities from colliding with unchanged ones that were
    /// cloned into the candidate epoch.
    pub fn max_entity_id_for_epoch(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<u64>, StorageError> {
        let mut stmt = conn
            .prepare("SELECT entity_ids FROM chunks WHERE project_id = ?1 AND epoch = ?2")
            .map_err(|e| StorageError::query(format!("Failed to prepare query: {e}")))?;
        let rows = stmt
            .query_map(rusqlite::params![project_id, epoch], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| StorageError::query(format!("Failed to query chunks: {e}")))?;

        let mut max_id: Option<u64> = None;
        for row in rows {
            let entity_ids_json =
                row.map_err(|e| StorageError::query(format!("Failed to read chunk row: {e}")))?;
            if let Ok(entity_ids) = serde_json::from_str::<Vec<i64>>(&entity_ids_json) {
                for id in entity_ids {
                    if id >= 0 {
                        let id = id as u64;
                        max_id = Some(max_id.map_or(id, |m: u64| m.max(id)));
                    }
                }
            }
        }
        drop(stmt);
        Ok(max_id)
    }

    /// Map a database row to a ChunkRecord
    fn map_row(row: &Row) -> Result<ChunkRecord, rusqlite::Error> {
        Ok(ChunkRecord {
            chunk_id: row.get(0)?,
            file_path: row.get(1)?,
            content: row.get(2)?,
            start_line: row.get(3)?,
            end_line: row.get(4)?,
            entity_ids: row.get(5)?,
            entity_names: row.get(6)?,
            chunk_type: row.get(7)?,
            test_status: row.get::<_, u8>(8)?,
            test_source: row.get::<_, u8>(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            project_id: row.get(12)?,
            epoch: row.get(13)?,
            batch_id: row.get(14)?,
            path: row.get(15)?,
            bm25_keywords: row.get(16)?,
            segment_id: row.get(17)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("Failed to open database");
        conn.execute(
            "CREATE TABLE chunks (
                chunk_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                content TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                entity_ids TEXT NOT NULL,
                entity_names TEXT NOT NULL,
                chunk_type TEXT NOT NULL,
                test_status INTEGER NOT NULL DEFAULT 0,
                test_source INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                project_id INTEGER NOT NULL,
                epoch INTEGER NOT NULL DEFAULT 0,
                batch_id INTEGER NOT NULL DEFAULT 0,
                path TEXT NOT NULL DEFAULT 'emb',
                bm25_keywords TEXT NOT NULL DEFAULT '',
                segment_id TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (project_id, epoch, chunk_id)
            )",
            [],
        )
        .expect("Failed to create table");
        conn
    }

    #[test]
    fn test_max_entity_id_for_epoch() {
        let conn = create_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let chunk = |id: &str, entity_ids: &[i64], epoch: i64| {
            ChunkRecord::new(
                id.to_string(),
                "src/main.rs".to_string(),
                "code".to_string(),
                1,
                2,
            )
            .with_entity_ids(entity_ids)
            .with_project_id(1)
            .with_epoch(epoch)
        };

        for c in [
            chunk("a", &[3, 7], 0),
            chunk("b", &[], 0),
            chunk("c", &[42, 5], 0),
            chunk("d", &[100], 1),
        ] {
            ChunkRepository::insert(&tx, &c).expect("Failed to insert chunk");
        }
        tx.commit().expect("Failed to commit transaction");

        let max_epoch0 =
            ChunkRepository::max_entity_id_for_epoch(&conn, 1, 0).expect("query epoch 0");
        assert_eq!(max_epoch0, Some(42));

        let max_epoch1 =
            ChunkRepository::max_entity_id_for_epoch(&conn, 1, 1).expect("query epoch 1");
        assert_eq!(max_epoch1, Some(100));

        let max_other_project =
            ChunkRepository::max_entity_id_for_epoch(&conn, 2, 0).expect("query other project");
        assert_eq!(max_other_project, None);
    }

    #[test]
    fn test_insert_and_get() {
        let conn = create_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let chunk = ChunkRecord::new(
            "chunk-001".to_string(),
            "src/main.rs".to_string(),
            "fn main() {}".to_string(),
            1,
            2,
        )
        .with_chunk_type("function".to_string())
        .with_entity_ids(&[1, 2])
        .with_entity_names(&["main".to_string()])
        .with_project_id(1);

        ChunkRepository::insert(&tx, &chunk).expect("Failed to insert chunk");
        tx.commit().expect("Failed to commit transaction");

        let retrieved =
            ChunkRepository::get_by_id(&conn, "chunk-001", 1).expect("Failed to get chunk by id");
        assert!(retrieved.is_some());
        let retrieved = retrieved.expect("Expected Some value");
        assert_eq!(retrieved.chunk_id, "chunk-001");
        assert_eq!(retrieved.content, "fn main() {}");
        assert_eq!(retrieved.chunk_type, "function");
    }

    #[test]
    fn test_get_by_file_path() {
        let conn = create_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let chunk1 = ChunkRecord::new(
            "chunk-001".to_string(),
            "src/main.rs".to_string(),
            "fn main() {}".to_string(),
            1,
            2,
        )
        .with_project_id(1);

        let chunk2 = ChunkRecord::new(
            "chunk-002".to_string(),
            "src/main.rs".to_string(),
            "fn helper() {}".to_string(),
            5,
            6,
        )
        .with_project_id(1);

        ChunkRepository::insert(&tx, &chunk1).expect("Failed to insert chunk1");
        ChunkRepository::insert(&tx, &chunk2).expect("Failed to insert chunk2");
        tx.commit().expect("Failed to commit transaction");

        let chunks = ChunkRepository::get_by_file_and_project(&conn, "src/main.rs", 1)
            .expect("Failed to get chunks by file path");
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_get_by_file_path_excludes_bm25_records() {
        let conn = create_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let emb = ChunkRecord::new(
            "group_1_emb_0".to_string(),
            "src/main.rs".to_string(),
            "embedded content".to_string(),
            1,
            2,
        )
        .with_project_id(1)
        .with_path("emb");
        let bm25 = ChunkRecord::new(
            "group_1_bm25_0".to_string(),
            "src/main.rs".to_string(),
            "bm25 content".to_string(),
            1,
            2,
        )
        .with_project_id(1)
        .with_path("bm25");

        ChunkRepository::insert(&tx, &emb).expect("Failed to insert embedding chunk");
        ChunkRepository::insert(&tx, &bm25).expect("Failed to insert BM25 chunk");
        tx.commit().expect("Failed to commit transaction");

        let chunks = ChunkRepository::get_by_file_and_project(&conn, "src/main.rs", 1)
            .expect("Failed to get chunks by file path");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_id, "group_1_emb_0");
    }

    #[test]
    fn stores_identical_chunk_ids_in_distinct_epochs() {
        let conn = create_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");
        let first = ChunkRecord::new(
            "chunk-001".to_string(),
            "src/main.rs".to_string(),
            "old".to_string(),
            1,
            1,
        )
        .with_project_id(1)
        .with_epoch(1);
        let second = ChunkRecord::new(
            "chunk-001".to_string(),
            "src/main.rs".to_string(),
            "new".to_string(),
            1,
            1,
        )
        .with_project_id(1)
        .with_epoch(2);
        ChunkRepository::insert(&tx, &first).expect("Failed to insert first generation");
        ChunkRepository::insert(&tx, &second).expect("Failed to insert second generation");
        tx.commit().expect("Failed to commit transaction");

        let ids = vec!["chunk-001".to_string()];
        let old = ChunkRepository::get_by_chunk_ids(&conn, &ids, 1, Some(1))
            .expect("Failed to query old generation");
        let new = ChunkRepository::get_by_chunk_ids(&conn, &ids, 1, Some(2))
            .expect("Failed to query new generation");
        assert_eq!(old[0].content, "old");
        assert_eq!(new[0].content, "new");

        let latest = ChunkRepository::get_by_id(&conn, "chunk-001", 1)
            .expect("Failed to query latest generation")
            .expect("Expected latest chunk");
        assert_eq!(latest.content, "new");
    }
}
