//! File repository for SQLite operations
//!
//! # Persistence Strategy
//!
//! This repository manages file metadata persistence to SQLite. Key design principles:
//!
//! 1. **Primary Source of Truth**: The in-memory `RelationIndex.file_index` is the primary
//!    source during runtime. SQLite provides persistence across restarts.
//!
//! 2. **Batch Operations**: Files are persisted in batches during scanning,
//!    not individually, to minimize database round-trips.
//!
//! 3. **Project-Scoped Queries**: All query methods should include `project_id`
//!    to leverage composite indexes and avoid full table scans.
//!
//! 4. **Unique Paths**: File paths are unique within a project. Use `get_by_path_and_project()`
//!    for efficient lookups.

use rusqlite::{Connection, params};

use crate::helpers::{
    FromRow, execute_count, execute_insert, execute_insert_batch, execute_query,
    execute_query_optional, execute_update,
};
use crate::types::FileRecord;
use cce_types::StorageError;

/// File repository for CRUD operations
pub struct FileRepository;

impl FileRepository {
    /// Insert a file
    pub fn insert(tx: &rusqlite::Transaction, file: &FileRecord) -> Result<i64, StorageError> {
        execute_insert(
            tx,
            "INSERT INTO files (path, language, category, last_modified, created_at, project_id, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                file.path,
                file.language,
                file.category,
                file.last_modified,
                file.created_at,
                file.project_id,
                file.content_hash
            ],
            "file",
        )
    }

    /// Insert a file, silently ignoring if a row with the same
    /// `(project_id, epoch, path)` exists.
    /// Returns the new row id, or 0 if the row already existed.
    pub fn insert_or_ignore(
        tx: &rusqlite::Transaction,
        file: &FileRecord,
    ) -> Result<i64, StorageError> {
        let inserted = tx
            .execute(
                "INSERT OR IGNORE INTO files (path, language, category, last_modified, created_at, project_id, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    file.path,
                    file.language,
                    file.category,
                    file.last_modified,
                    file.created_at,
                    file.project_id,
                    file.content_hash
                ],
            )
            .map_err(|e| StorageError::insert(format!("Failed to insert file: {}", e)))?;

        Ok(if inserted == 0 {
            0
        } else {
            tx.last_insert_rowid()
        })
    }

    /// Insert multiple files
    pub fn insert_batch(
        tx: &rusqlite::Transaction,
        files: &[FileRecord],
    ) -> Result<Vec<i64>, StorageError> {
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let param_list: Vec<Vec<&dyn rusqlite::ToSql>> = files
            .iter()
            .map(|f| {
                vec![
                    &f.path as &dyn rusqlite::ToSql,
                    &f.language,
                    &f.category,
                    &f.last_modified,
                    &f.created_at,
                    &f.project_id,
                    &f.content_hash,
                ]
            })
            .collect();

        execute_insert_batch(
            tx,
            "INSERT INTO files (path, language, category, last_modified, created_at, project_id, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &param_list,
            "file",
        )
    }

    /// Get a file by ID
    pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<FileRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files WHERE id = ?1",
            params![id],
            FileRecord::from_row,
        )
    }

    /// Get the latest file by path and project_id (any epoch).
    /// For epoch-scoped lookups, use get_by_path_and_project_at_epoch.
    ///
    /// # Performance Note
    /// Uses composite index on (path, project_id) for efficient lookup.
    pub fn get_by_path_and_project(
        conn: &Connection,
        path: &str,
        project_id: i64,
    ) -> Result<Option<FileRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash
             FROM files
             WHERE path = ?1 AND project_id = ?2
             ORDER BY epoch DESC, id DESC LIMIT 1",
            params![path, project_id],
            FileRecord::from_row,
        )
    }

    /// Get all files for a project
    pub fn get_by_project(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files
             WHERE project_id = ?1 ORDER BY id",
            params![project_id],
            FileRecord::from_row,
        )
    }

    /// Get files by project ID with pagination
    ///
    /// # Performance Note
    /// Uses index on project_id column for efficient lookup.
    /// Use this method instead of loading all files at once.
    pub fn get_by_project_id_paged(
        conn: &Connection,
        project_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files
             WHERE project_id = ?1 ORDER BY id LIMIT ?2 OFFSET ?3",
            params![project_id, limit, offset],
            FileRecord::from_row,
        )
    }

    /// Get the files of one exact generation with pagination.
    ///
    /// Regeneration sweeps use this to enumerate unchanged files of the
    /// active generation without mixing in candidate-epoch override rows.
    pub fn get_by_project_and_epoch_paged(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FileRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files
             WHERE project_id = ?1 AND epoch = ?2 ORDER BY id LIMIT ?3 OFFSET ?4",
            params![project_id, epoch, limit, offset],
            FileRecord::from_row,
        )
    }

    /// Update a file's last modified timestamp
    pub fn update_modified(
        tx: &rusqlite::Transaction,
        id: i64,
        last_modified: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "UPDATE files SET last_modified = ?1 WHERE id = ?2",
            params![last_modified, id],
            "update file",
        )
    }

    /// Update a file's content hash (project-scoped)
    pub fn update_content_hash(
        tx: &rusqlite::Transaction,
        path: &str,
        content_hash: &str,
        project_id: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "UPDATE files SET content_hash = ?1 WHERE path = ?2 AND project_id = ?3",
            params![content_hash, path, project_id],
            "update file content hash",
        )
    }

    /// Insert file hash for a specific epoch, creating a new row.
    /// Unlike upsert_or_update_hash, this always creates a new row for the given epoch
    /// instead of overwriting the existing row, preserving old epoch data.
    pub fn insert_hash_for_epoch(
        tx: &rusqlite::Transaction,
        path: &std::path::Path,
        content_hash: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        let path_str = path.to_string_lossy();
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO files (path, language, last_modified, created_at, project_id, content_hash, epoch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(project_id, epoch, path) DO UPDATE SET
                content_hash = excluded.content_hash,
                last_modified = excluded.last_modified",
            params![
                path_str.as_ref(),
                "unknown",
                now,
                now,
                project_id,
                content_hash,
                epoch,
            ],
        )
        .map_err(|e| StorageError::insert(format!("Failed to insert file record for epoch: {}", e)))?;

        Ok(())
    }

    /// Upsert or update file hash (insert if not exists, update if exists)
    /// Deprecated: use insert_hash_for_epoch for epoch-versioned writes
    pub fn upsert_or_update_hash(
        tx: &rusqlite::Transaction,
        path: &std::path::Path,
        content_hash: &str,
        project_id: i64,
    ) -> Result<(), StorageError> {
        let path_str = path.to_string_lossy();

        let updated = tx
            .execute(
                "UPDATE files SET content_hash = ?1 WHERE path = ?2 AND project_id = ?3",
                params![content_hash, path_str.as_ref(), project_id],
            )
            .map_err(|e| StorageError::update(format!("Failed to update file hash: {}", e)))?;

        if updated == 0 {
            let now = chrono::Utc::now().timestamp();
            tx.execute(
                "INSERT INTO files (path, language, last_modified, created_at, project_id, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    path_str.as_ref(),
                    "unknown",
                    now,
                    now,
                    project_id,
                    content_hash,
                ],
            ).map_err(|e| StorageError::insert(format!("Failed to insert file record: {}", e)))?;
        }

        Ok(())
    }

    /// Get a file by path and project_id for a specific epoch
    pub fn get_by_path_and_project_at_epoch(
        conn: &Connection,
        path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<FileRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, path, language, category, last_modified, created_at, project_id, content_hash FROM files
             WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
            params![path, project_id, epoch],
            FileRecord::from_row,
        )
    }

    /// Get file content hash by path and epoch (project-scoped)
    pub fn get_content_hash_by_path_at_epoch(
        conn: &Connection,
        path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<String>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT content_hash FROM files WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
            params![path, project_id, epoch],
            |row| row.get(0),
        )
    }

    /// Get file content hash by path (project-scoped)
    pub fn get_content_hash_by_path(
        conn: &Connection,
        path: &str,
        project_id: i64,
    ) -> Result<Option<String>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT content_hash FROM files
             WHERE path = ?1 AND project_id = ?2
             ORDER BY epoch DESC, id DESC LIMIT 1",
            params![path, project_id],
            |row| row.get(0),
        )
    }

    /// Delete a file by ID
    pub fn delete(tx: &rusqlite::Transaction, id: i64) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM files WHERE id = ?1",
            params![id],
            "delete file",
        )
    }

    /// Delete a file by path (project-scoped)
    pub fn delete_by_path(
        tx: &rusqlite::Transaction,
        path: &str,
        project_id: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM files WHERE path = ?1 AND project_id = ?2",
            params![path, project_id],
            "delete file",
        )
    }

    /// Delete a file by path at a specific epoch (project-scoped)
    pub fn delete_by_path_at_epoch(
        tx: &rusqlite::Transaction,
        path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM files WHERE path = ?1 AND project_id = ?2 AND epoch = ?3",
            params![path, project_id, epoch],
            "delete file at epoch",
        )
    }

    /// Count all files
    pub fn count(conn: &Connection) -> Result<i64, StorageError> {
        execute_count(conn, "SELECT COUNT(*) FROM files", params![], "files")
    }

    /// Count files belonging to a specific project
    pub fn count_by_project(conn: &Connection, project_id: i64) -> Result<i64, StorageError> {
        execute_count(
            conn,
            "SELECT COUNT(*) FROM files WHERE project_id = ?1",
            params![project_id],
            "files by project",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("Failed to open database");

        conn.execute(
            "CREATE TABLE files (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                language TEXT NOT NULL,
                category INTEGER NOT NULL DEFAULT 4,
                last_modified INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                project_id INTEGER NOT NULL,
                content_hash TEXT,
                epoch INTEGER NOT NULL DEFAULT 0,
                batch_id INTEGER NOT NULL DEFAULT 0,
                UNIQUE(project_id, epoch, path)
            )",
            [],
        )
        .expect("Failed to create table");

        conn
    }

    #[test]
    fn test_insert_file() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let file = FileRecord {
            id: 0,
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 1,
            content_hash: None,
        };

        let id = FileRepository::insert(&tx, &file).expect("Failed to insert");
        assert_eq!(id, 1);

        tx.commit().expect("Failed to commit");
    }

    #[test]
    fn test_get_by_path() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let file = FileRecord {
            id: 0,
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 1,
            content_hash: None,
        };

        FileRepository::insert(&tx, &file).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let result = FileRepository::get_by_path_and_project(&conn, "src/main.rs", 1)
            .expect("Failed to get by path and project");
        assert!(result.is_some());
        assert_eq!(result.expect("Expected Some value").path, "src/main.rs");
    }

    #[test]
    fn test_get_by_path_returns_latest_epoch() {
        let conn = setup_test_db();
        conn.execute(
            "INSERT INTO files
                (path, language, last_modified, created_at, project_id, content_hash, epoch)
             VALUES ('src/main.rs', 'rust', 1, 1, 1, 'old', 1),
                    ('src/main.rs', 'rust', 2, 2, 1, 'new', 2)",
            [],
        )
        .expect("Failed to insert file generations");

        let file = FileRepository::get_by_path_and_project(&conn, "src/main.rs", 1)
            .expect("Failed to get latest file")
            .expect("Expected latest file");
        assert_eq!(file.content_hash.as_deref(), Some("new"));
    }

    #[test]
    fn test_insert_or_ignore_returns_zero_for_existing_file() {
        let conn = setup_test_db();
        let file = FileRecord {
            id: 0,
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1,
            created_at: 1,
            project_id: 1,
            content_hash: Some("hash".to_string()),
        };

        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");
        let first = FileRepository::insert_or_ignore(&tx, &file).expect("Failed to insert file");
        let second = FileRepository::insert_or_ignore(&tx, &file).expect("Failed to ignore file");
        tx.commit().expect("Failed to commit transaction");

        assert!(first > 0);
        assert_eq!(second, 0);
    }

    #[test]
    fn test_count() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let file = FileRecord {
            id: 0,
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 1,
            content_hash: None,
        };

        FileRepository::insert(&tx, &file).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let count = FileRepository::count(&conn).expect("Failed to count");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_by_project_id_paged() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let file1 = FileRecord {
            id: 0,
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 1,
            content_hash: None,
        };

        let file2 = FileRecord {
            id: 0,
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 1,
            content_hash: None,
        };

        let file3 = FileRecord {
            id: 0,
            path: "src/other.rs".to_string(),
            language: "rust".to_string(),
            category: cce_types::FileCategory::Code.as_u8(),
            last_modified: 1000,
            created_at: 1000,
            project_id: 2,
            content_hash: None,
        };

        FileRepository::insert(&tx, &file1).expect("Failed to insert");
        FileRepository::insert(&tx, &file2).expect("Failed to insert");
        FileRepository::insert(&tx, &file3).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let files = FileRepository::get_by_project_id_paged(&conn, 1, 10, 0)
            .expect("Failed to get by project_id");
        assert_eq!(files.len(), 2);
    }
}
