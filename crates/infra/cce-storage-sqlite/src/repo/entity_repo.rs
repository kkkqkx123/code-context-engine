//! Entity repository for SQLite operations
//!
//! # Persistence Strategy
//!
//! This repository manages entity persistence to SQLite. Key design principles:
//!
//! 1. **Primary Source of Truth**: The in-memory `RelationIndex.function_index` is the
//!    primary source during runtime. SQLite provides persistence across restarts.
//!
//! 2. **Batch Operations**: Entities are persisted in batches during indexing,
//!    not individually, to minimize database round-trips.
//!
//! 3. **Project-Scoped Queries**: All query methods should include `project_id`
//!    to leverage composite indexes and avoid full table scans.
//!
//! 4. **Hot Update Support**: When files are updated, entities are deleted by file_id
//!    and re-inserted, ensuring consistency between memory and disk.

use rusqlite::{Connection, Row, params};

use crate::helpers::{
    execute_count, execute_insert, execute_insert_batch, execute_query, execute_query_optional,
    execute_update,
};
use crate::types::EntityRecord;
use cce_types::StorageError;

/// Entity repository for CRUD operations
pub struct EntityRepository;

impl EntityRepository {
    /// Insert a single entity
    pub fn insert(tx: &rusqlite::Transaction, entity: &EntityRecord) -> Result<i64, StorageError> {
        execute_insert(
            tx,
            "INSERT INTO entities (name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            params![
                entity.name,
                entity.kind,
                entity.file_id,
                entity.signature,
                entity.span_start_row,
                entity.span_end_row,
                entity.span_start_column,
                entity.span_end_column,
                entity.span_start_byte,
                entity.span_end_byte,
                entity.scoped_name,
                entity.depth,
                entity.parent_id,
                entity.metadata,
                entity.parameters_json,
                entity.return_type,
                entity.doc_comment,
                entity.modifiers_json,
                entity.project_id,
                entity.epoch,
                entity.batch_id
            ],
            "entity",
        )
    }

    /// Insert multiple entities
    pub fn insert_batch(
        tx: &rusqlite::Transaction,
        entities: &[EntityRecord],
    ) -> Result<Vec<i64>, StorageError> {
        if entities.is_empty() {
            return Ok(Vec::new());
        }

        let param_list: Vec<Vec<&dyn rusqlite::ToSql>> = entities
            .iter()
            .map(|entity| {
                vec![
                    &entity.name as &dyn rusqlite::ToSql,
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
                ]
            })
            .collect();

        execute_insert_batch(
            tx,
            "INSERT INTO entities (name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            &param_list,
            "entity",
        )
    }

    /// Get an entity by ID
    ///
    /// # Performance Note
    /// Uses SQLite's automatic PRIMARY KEY index for O(log n) lookup.
    /// This is the most efficient single-entity query method.
    pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<EntityRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE id = ?1",
            params![id],
            Self::from_row,
        )
    }

    /// Get entities by file ID
    ///
    /// # Performance Note
    /// Uses composite index `idx_entities_project_file` for efficient lookup.
    /// For better performance, consider adding project_id filter if available.
    pub fn get_by_file_id(
        conn: &Connection,
        file_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE file_id = ?1",
            params![file_id],
            Self::from_row,
        )
    }

    /// Get entities by file ID at a specific epoch.
    pub fn get_by_file_id_at_epoch(
        conn: &Connection,
        file_id: i64,
        epoch: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE file_id = ?1 AND epoch = ?2",
            params![file_id, epoch],
            Self::from_row,
        )
    }

    /// Get entities by file ID and project ID (optimized)
    ///
    /// # Performance Note
    /// Uses composite index `idx_entities_project_file` for O(log n) lookup.
    /// This is more efficient than get_by_file_id when project_id is known.
    pub fn get_by_file_and_project(
        conn: &Connection,
        file_id: i64,
        project_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE file_id = ?1 AND project_id = ?2",
            params![file_id, project_id],
            Self::from_row,
        )
    }

    /// Get entities by name within a project
    ///
    /// # Performance Note
    /// Uses composite index `idx_entities_project_name` for efficient lookup.
    /// Always requires project_id to avoid full table scan.
    pub fn get_by_name_in_project(
        conn: &Connection,
        name: &str,
        project_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE name = ?1 AND project_id = ?2",
            params![name, project_id],
            Self::from_row,
        )
    }

    /// Search entities using FTS5 (Full-Text Search)
    ///
    /// # Performance Note
    /// Uses SQLite's FTS5 virtual table for efficient text searching across
    /// entity names and signatures.
    pub fn search_fts(
        conn: &Connection,
        query: &str,
        project_id: i64,
        limit: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT e.id, e.name, e.kind, e.file_id, e.signature, e.span_start_row, e.span_end_row, e.span_start_column, e.span_end_column, e.span_start_byte, e.span_end_byte, e.scoped_name, e.depth, e.parent_id, e.metadata, e.parameters_json, e.return_type, e.doc_comment, e.modifiers_json, e.project_id, e.epoch, e.batch_id
             FROM entities e
             JOIN entities_fts f ON e.id = f.rowid
             WHERE entities_fts MATCH ?1 AND e.project_id = ?2
             ORDER BY rank LIMIT ?3",
            params![query, project_id, limit],
            Self::from_row,
        )
    }

    /// Search entities using FTS5, scoped to a specific epoch.
    pub fn search_fts_at_epoch(
        conn: &Connection,
        query: &str,
        project_id: i64,
        limit: i64,
        epoch: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT e.id, e.name, e.kind, e.file_id, e.signature, e.span_start_row, e.span_end_row, e.span_start_column, e.span_end_column, e.span_start_byte, e.span_end_byte, e.scoped_name, e.depth, e.parent_id, e.metadata, e.parameters_json, e.return_type, e.doc_comment, e.modifiers_json, e.project_id, e.epoch, e.batch_id
             FROM entities e
             JOIN entities_fts f ON e.id = f.rowid
             WHERE entities_fts MATCH ?1 AND e.project_id = ?2 AND e.epoch = ?4
             ORDER BY rank LIMIT ?3",
            params![query, project_id, limit, epoch],
            Self::from_row,
        )
    }

    /// Get entities by kind within a project
    ///
    /// # Performance Note
    /// Uses composite index `idx_entities_project_kind` for efficient lookup.
    pub fn get_by_kind_in_project(
        conn: &Connection,
        kind: &str,
        project_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE kind = ?1 AND project_id = ?2",
            params![kind, project_id],
            Self::from_row,
        )
    }

    /// Get entities by project ID
    ///
    /// # Performance Note
    /// Uses index `idx_entities_project`. For large projects, consider pagination.
    pub fn get_by_project_id(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE project_id = ?1",
            params![project_id],
            Self::from_row,
        )
    }

    /// Get entities by project ID with pagination
    ///
    /// # Performance Note
    /// Recommended for large projects to avoid loading all entities into memory.
    pub fn get_by_project_id_paged(
        conn: &Connection,
        project_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, kind, file_id, signature, span_start_row, span_end_row, span_start_column, span_end_column, span_start_byte, span_end_byte, scoped_name, depth, parent_id, metadata, parameters_json, return_type, doc_comment, modifiers_json, project_id, epoch, batch_id
             FROM entities WHERE project_id = ?1 ORDER BY id LIMIT ?2 OFFSET ?3",
            params![project_id, limit, offset],
            Self::from_row,
        )
    }

    /// Get all entities for a project with their associated file_path.
    /// Used by the relation snapshot loader and resolution pipeline to
    /// reconstruct the in-memory symbol table.
    pub fn get_by_project_with_file_path(
        conn: &Connection,
        project_id: i64,
    ) -> Result<Vec<(EntityRecord, String)>, StorageError> {
        execute_query(
            conn,
            "SELECT e.id, e.name, e.kind, e.file_id, e.signature, e.span_start_row, e.span_end_row, e.span_start_column, e.span_end_column, e.span_start_byte, e.span_end_byte, e.scoped_name, e.depth, e.parent_id, e.metadata, e.parameters_json, e.return_type, e.doc_comment, e.modifiers_json, e.project_id, e.epoch, e.batch_id, f.path
             FROM entities e JOIN files f ON e.file_id = f.id
             WHERE e.project_id = ?1",
            params![project_id],
            |row| {
                let entity = Self::parse_entity_from_row(row)?;
                let file_path: String = row.get(22)?;
                Ok((entity, file_path))
            },
        )
    }

    /// Get entities for a project at a given epoch with file_path.
    /// Used by RelationSnapshotLoader for cold-start reconstruction.
    pub fn get_by_project_and_epoch_with_file_path(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<(EntityRecord, String)>, StorageError> {
        execute_query(
            conn,
            "SELECT e.id, e.name, e.kind, e.file_id, e.signature, e.span_start_row, e.span_end_row, e.span_start_column, e.span_end_column, e.span_start_byte, e.span_end_byte, e.scoped_name, e.depth, e.parent_id, e.metadata, e.parameters_json, e.return_type, e.doc_comment, e.modifiers_json, e.project_id, e.epoch, e.batch_id, f.path
             FROM entities e JOIN files f ON e.file_id = f.id
             WHERE e.project_id = ?1 AND e.epoch = ?2",
            params![project_id, epoch],
            |row| {
                let entity = Self::parse_entity_from_row(row)?;
                let file_path: String = row.get(22)?;
                Ok((entity, file_path))
            },
        )
    }

    /// Delete an entity by ID
    pub fn delete(tx: &rusqlite::Transaction, id: i64) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE id = ?1",
            params![id],
            "delete entity",
        )
    }

    /// Delete entities by file ID (all epochs)
    /// Prefer delete_by_file_id_at_epoch for epoch-scoped deletion.
    pub fn delete_by_file_id(tx: &rusqlite::Transaction, file_id: i64) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE file_id = ?1",
            params![file_id],
            "delete entities by file",
        )
    }

    /// Delete entities by file ID at a specific epoch.
    /// Use this during hot-update to only remove the current epoch's entities
    /// without affecting data visible to other epochs.
    pub fn delete_by_file_id_at_epoch(
        tx: &rusqlite::Transaction,
        file_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE file_id = ?1 AND epoch = ?2",
            params![file_id, epoch],
            "delete entities by file at epoch",
        )
    }

    /// Delete all entities for a project.
    pub fn delete_by_project(
        tx: &rusqlite::Transaction,
        project_id: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE project_id = ?1",
            params![project_id],
            "delete entities by project",
        )
    }

    /// Delete entities belonging to one unpublished or obsolete snapshot.
    pub fn delete_by_project_and_epoch(
        tx: &rusqlite::Transaction,
        project_id: i64,
        epoch: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2",
            params![project_id, epoch],
            "delete entities by project and epoch",
        )
    }

    /// Delete entities for a project by file_path (requires JOIN with files).
    /// Caution: This deletes across ALL epochs. Use the epoch-scoped variant
    /// for hot-update operations.
    pub fn delete_by_file_path(
        tx: &rusqlite::Transaction,
        project_id: i64,
        file_path: &str,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE project_id = ?1 AND file_id IN (SELECT id FROM files WHERE path = ?2)",
            rusqlite::params![project_id, file_path],
            "delete entities by file path",
        )
    }

    /// Delete entities for a project by file_path and epoch.
    /// Used by hot-update to remove only the current epoch's file entities.
    pub fn delete_by_file_path_epoch(
        tx: &rusqlite::Transaction,
        project_id: i64,
        file_path: &str,
        epoch: i64,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN (SELECT id FROM files WHERE path = ?3)",
            rusqlite::params![project_id, epoch, file_path],
            "delete entities by file path and epoch",
        )
    }

    /// Update scoped_name and span byte offsets for an existing entity.
    /// Used by publish_symbols to enrich entities after scoped-name resolution.
    pub fn update_symbol_info(
        tx: &rusqlite::Transaction,
        entity_id: i64,
        scoped_name: &str,
        span_start_byte: Option<i64>,
        span_end_byte: Option<i64>,
    ) -> Result<(), StorageError> {
        execute_update(
            tx,
            "UPDATE entities SET scoped_name = ?1, span_start_byte = ?2, span_end_byte = ?3 WHERE id = ?4",
            params![scoped_name, span_start_byte, span_end_byte, entity_id],
            "update entity symbol info",
        )
    }

    /// Count all entities
    pub fn count(conn: &Connection) -> Result<i64, StorageError> {
        execute_count(conn, "SELECT COUNT(*) FROM entities", params![], "entities")
    }

    /// Count entities for a project at a given epoch.
    pub fn count_by_project_and_epoch(
        conn: &Connection,
        project_id: i64,
        epoch: i64,
    ) -> Result<i64, StorageError> {
        execute_count(
            conn,
            "SELECT COUNT(*) FROM entities WHERE project_id = ?1 AND epoch = ?2",
            params![project_id, epoch],
            "entities by project and epoch",
        )
    }

    fn parse_entity_from_row(row: &Row) -> Result<EntityRecord, rusqlite::Error> {
        Ok(EntityRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            file_id: row.get(3)?,
            signature: row.get(4)?,
            span_start_row: row.get(5)?,
            span_end_row: row.get(6)?,
            span_start_column: row.get(7)?,
            span_end_column: row.get(8)?,
            span_start_byte: row.get(9)?,
            span_end_byte: row.get(10)?,
            scoped_name: row.get(11)?,
            depth: row.get(12)?,
            parent_id: row.get(13)?,
            metadata: row.get(14)?,
            parameters_json: row.get(15)?,
            return_type: row.get(16)?,
            doc_comment: row.get(17)?,
            modifiers_json: row.get(18)?,
            project_id: row.get::<_, i64>(19)?,
            epoch: row.get(20)?,
            batch_id: row.get(21)?,
        })
    }

    /// Parse a row into EntityRecord
    fn from_row(row: &Row) -> Result<EntityRecord, rusqlite::Error> {
        Self::parse_entity_from_row(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("Failed to open database");

        conn.execute(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_id INTEGER NOT NULL,
                signature TEXT,
                span_start_row INTEGER,
                span_end_row INTEGER,
                span_start_column INTEGER,
                span_end_column INTEGER,
                span_start_byte INTEGER,
                span_end_byte INTEGER,
                scoped_name TEXT,
                depth INTEGER,
                parent_id INTEGER,
                metadata TEXT,
                parameters_json TEXT,
                return_type TEXT,
                doc_comment TEXT,
                modifiers_json TEXT,
                project_id INTEGER NOT NULL,
                epoch INTEGER NOT NULL DEFAULT 0,
                batch_id INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .expect("Failed to create table");

        conn
    }

    #[test]
    fn test_insert_entity() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        let id = EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        assert_eq!(id, 1);

        tx.commit().expect("Failed to commit");
    }

    #[test]
    fn test_get_by_id() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        let id = EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let result = EntityRepository::get_by_id(&conn, id).expect("Failed to get by id");
        assert!(result.is_some());
        assert_eq!(result.expect("Expected Some value").name, "test_function");
    }

    #[test]
    fn test_get_by_name() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let results = EntityRepository::get_by_name_in_project(&conn, "test_function", 1)
            .expect("Failed to get by name");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test_function");
    }

    #[test]
    fn test_get_by_file_id() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let results = EntityRepository::get_by_file_id(&conn, 1).expect("Failed to get by file_id");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_id, 1);
    }

    #[test]
    fn test_get_by_kind() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity1 = EntityRecord {
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            depth: Some(0),
            parent_id: None,
            metadata: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        let entity2 = EntityRecord {
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            id: 0,
            name: "TestClass".to_string(),
            kind: "Class".to_string(),
            file_id: 1,
            signature: Some("class TestClass".to_string()),
            span_start_row: Some(10),
            span_end_row: Some(20),
            depth: Some(0),
            parent_id: None,
            metadata: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        EntityRepository::insert(&tx, &entity1).expect("Failed to insert");
        EntityRepository::insert(&tx, &entity2).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let results = EntityRepository::get_by_kind_in_project(&conn, "Function", 1)
            .expect("Failed to get by kind");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, "Function");
    }

    #[test]
    fn test_count() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        tx.commit().expect("Failed to commit");

        let count = EntityRepository::count(&conn).expect("Failed to count");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_delete() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        let id = EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        EntityRepository::delete(&tx, id).expect("Failed to delete");
        tx.commit().expect("Failed to commit");

        let result = EntityRepository::get_by_id(&conn, id).expect("Failed to get by id");
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_by_file_id() {
        let conn = setup_test_db();
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        let entity = EntityRecord {
            id: 0,
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            file_id: 1,
            signature: Some("fn test_function()".to_string()),
            span_start_row: Some(0),
            span_end_row: Some(5),
            span_start_column: None,
            span_end_column: None,
            span_start_byte: None,
            span_end_byte: None,
            scoped_name: None,
            depth: Some(0),
            parent_id: None,
            metadata: None,
            parameters_json: None,
            return_type: None,
            doc_comment: None,
            modifiers_json: None,
            project_id: 1,
            epoch: 0,
            batch_id: 0,
        };

        EntityRepository::insert(&tx, &entity).expect("Failed to insert");
        EntityRepository::delete_by_file_id(&tx, 1).expect("Failed to delete by file_id");
        tx.commit().expect("Failed to commit");

        let results = EntityRepository::get_by_file_id(&conn, 1).expect("Failed to get by file_id");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_fts_basic() {
        let conn = Connection::open_in_memory().expect("Failed to open database");

        // Create entities table
        conn.execute(
            "CREATE TABLE entities (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_id INTEGER NOT NULL,
                signature TEXT,
                span_start_row INTEGER,
                span_end_row INTEGER,
                span_start_column INTEGER,
                span_end_column INTEGER,
                span_start_byte INTEGER,
                span_end_byte INTEGER,
                scoped_name TEXT,
                depth INTEGER,
                parent_id INTEGER,
                metadata TEXT,
                parameters_json TEXT,
                return_type TEXT,
                doc_comment TEXT,
                modifiers_json TEXT,
                project_id INTEGER NOT NULL,
                epoch INTEGER NOT NULL DEFAULT 0,
                batch_id INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .expect("Failed to create table");

        // Create FTS5 virtual table
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(name, signature, content='entities', content_rowid='id')",
            [],
        )
        .expect("Failed to create FTS5 table");

        // Create triggers
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
                INSERT INTO entities_fts(rowid, name, signature) VALUES (new.id, new.name, new.signature);
            END;
            CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
                INSERT INTO entities_fts(entities_fts, rowid, name, signature) VALUES('delete', old.id, old.name, old.signature);
            END;
            CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities BEGIN
                INSERT INTO entities_fts(entities_fts, rowid, name, signature) VALUES('delete', old.id, old.name, old.signature);
                INSERT INTO entities_fts(rowid, name, signature) VALUES (new.id, new.name, new.signature);
            END;"
        )
        .expect("Failed to create triggers");

        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        // Insert test entities
        let entities = vec![
            EntityRecord {
                span_start_column: None,
                span_end_column: None,
                span_start_byte: None,
                span_end_byte: None,
                scoped_name: None,
                parameters_json: None,
                return_type: None,
                doc_comment: None,
                modifiers_json: None,
                id: 0,
                name: "authenticate_user".to_string(),
                kind: "Function".to_string(),
                file_id: 1,
                signature: Some("fn authenticate_user(username: &str) -> bool".to_string()),
                span_start_row: Some(10),
                span_end_row: Some(20),
                depth: Some(0),
                parent_id: None,
                metadata: None,
                project_id: 1,
                epoch: 0,
                batch_id: 0,
            },
            EntityRecord {
                span_start_column: None,
                span_end_column: None,
                span_start_byte: None,
                span_end_byte: None,
                scoped_name: None,
                parameters_json: None,
                return_type: None,
                doc_comment: None,
                modifiers_json: None,
                id: 0,
                name: "authorization_check".to_string(),
                kind: "Function".to_string(),
                file_id: 1,
                signature: Some("fn authorization_check(user_id: u32) -> Result<()>".to_string()),
                span_start_row: Some(25),
                span_end_row: Some(35),
                depth: Some(0),
                parent_id: None,
                metadata: None,
                project_id: 1,
                epoch: 0,
                batch_id: 0,
            },
            EntityRecord {
                span_start_column: None,
                span_end_column: None,
                span_start_byte: None,
                span_end_byte: None,
                scoped_name: None,
                parameters_json: None,
                return_type: None,
                doc_comment: None,
                modifiers_json: None,
                id: 0,
                name: "test_helper".to_string(),
                kind: "Function".to_string(),
                file_id: 2,
                signature: Some("fn test_helper()".to_string()),
                span_start_row: Some(5),
                span_end_row: Some(8),
                depth: Some(0),
                parent_id: None,
                metadata: None,
                project_id: 1,
                epoch: 0,
                batch_id: 0,
            },
        ];

        EntityRepository::insert_batch(&tx, &entities).expect("Failed to insert batch");
        tx.commit().expect("Failed to commit");

        // Test prefix search
        let results =
            EntityRepository::search_fts(&conn, "auth*", 1, 10).expect("Failed to search FTS5");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|e| e.name == "authenticate_user"));
        assert!(results.iter().any(|e| e.name == "authorization_check"));

        // Test exact name search
        let results = EntityRepository::search_fts(&conn, "authenticate_user", 1, 10)
            .expect("Failed to search FTS5");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "authenticate_user");

        // Test signature search
        let results =
            EntityRepository::search_fts(&conn, "username", 1, 10).expect("Failed to search FTS5");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "authenticate_user");

        // Test project isolation
        let results =
            EntityRepository::search_fts(&conn, "auth*", 2, 10).expect("Failed to search FTS5");
        assert_eq!(results.len(), 0); // No entities in project 2
    }
}
