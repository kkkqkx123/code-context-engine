//! Detail tables: entity_detail_mappings, chunks.

use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_tables(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entity_detail_mappings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_id INTEGER NOT NULL,
            project_id INTEGER NOT NULL,
            epoch INTEGER NOT NULL DEFAULT 0,
            qdrant_point_ids TEXT NOT NULL DEFAULT '[]',
            bm25_doc_ids TEXT NOT NULL DEFAULT '[]',
            chunk_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(project_id, epoch, entity_id),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
            FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| {
        StorageError::Table(format!(
            "Failed to create entity_detail_mappings table: {}",
            e
        ))
    })?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            content TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            entity_ids TEXT NOT NULL DEFAULT '[]',
            entity_names TEXT NOT NULL DEFAULT '[]',
            entity_kinds TEXT NOT NULL DEFAULT '[]',
            group_title TEXT NOT NULL DEFAULT '',
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
            truncated INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (project_id, epoch, file_path, chunk_id),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create chunks table: {}", e)))?;

    Ok(())
}
