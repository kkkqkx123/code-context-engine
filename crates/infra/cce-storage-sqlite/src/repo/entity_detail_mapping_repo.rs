//! Entity detail mapping repository for CRUD operations
//!
//! This module provides repository operations for EntityDetailMapping records,
//! which track fine-grained entity-level embeddings and BM25 documents.

use rusqlite::{Connection, Row, params};

use crate::helpers::{execute_count, execute_insert, execute_query, execute_query_optional};
use crate::types::EntityDetailMapping;
use cce_types::StorageError;

/// Entity detail mapping repository for CRUD operations
pub struct EntityDetailMappingRepository;

impl EntityDetailMappingRepository {
    /// Insert a single detail mapping
    pub fn insert(
        tx: &rusqlite::Transaction,
        mapping: &EntityDetailMapping,
    ) -> Result<i64, StorageError> {
        execute_insert(
            tx,
            "INSERT INTO entity_detail_mappings (
                entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids, chunk_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                mapping.entity_id,
                mapping.project_id,
                mapping.epoch,
                mapping.qdrant_point_ids,
                mapping.bm25_doc_ids,
                mapping.chunk_count,
                mapping.created_at,
                mapping.updated_at,
            ],
            "entity detail mapping",
        )
    }

    /// Upsert a detail mapping (insert or update with epoch scope)
    ///
    /// The stored row is merged, not replaced: the embedding and BM25 modules
    /// persist their own columns of the same `(project, epoch, entity)` row
    /// independently, so a replacing upsert would let the second writer erase
    /// the first writer's ids. Stale rows are cleared by the per-file
    /// preparation step before a file is rewritten inside one generation.
    pub fn upsert(
        tx: &rusqlite::Transaction,
        mapping: &EntityDetailMapping,
    ) -> Result<i64, StorageError> {
        let merged = match mapping
            .project_id
            .and_then(|project_id| {
                Self::get_by_entity_id_at_epoch(tx, mapping.entity_id, project_id, mapping.epoch)
                    .transpose()
            })
            .transpose()?
        {
            Some(existing) => {
                let mut point_ids = mapping.get_qdrant_point_ids();
                for point_id in existing.get_qdrant_point_ids() {
                    if !point_ids.contains(&point_id) {
                        point_ids.push(point_id);
                    }
                }
                let mut doc_ids = mapping.get_bm25_doc_ids();
                for doc_id in existing.get_bm25_doc_ids() {
                    if !doc_ids.contains(&doc_id) {
                        doc_ids.push(doc_id);
                    }
                }
                mapping
                    .clone()
                    .with_qdrant_point_ids(&point_ids)
                    .with_bm25_doc_ids(&doc_ids)
            }
            None => mapping.clone(),
        };
        tx.execute(
            "INSERT INTO entity_detail_mappings (
                entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids, chunk_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(project_id, epoch, entity_id) DO UPDATE SET
                qdrant_point_ids = excluded.qdrant_point_ids,
                bm25_doc_ids = excluded.bm25_doc_ids,
                chunk_count = excluded.chunk_count,
                updated_at = excluded.updated_at",
            params![
                merged.entity_id,
                merged.project_id,
                merged.epoch,
                merged.qdrant_point_ids,
                merged.bm25_doc_ids,
                merged.chunk_count,
                merged.created_at,
                merged.updated_at,
            ],
        )
        .map_err(|e| {
            StorageError::insert(format!("Failed to upsert entity detail mapping: {}", e))
        })?;
        tx.query_row(
            "SELECT id FROM entity_detail_mappings
             WHERE project_id = ?1 AND epoch = ?2 AND entity_id = ?3",
            params![mapping.project_id, mapping.epoch, mapping.entity_id],
            |row| row.get(0),
        )
        .map_err(|e| {
            StorageError::query(format!(
                "Failed to get upserted entity detail mapping ID: {}",
                e
            ))
        })
    }

    /// Get detail mapping by entity ID (project-scoped)
    pub fn get_by_entity_id(
        conn: &Connection,
        entity_id: i64,
        project_id: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids,
                    chunk_count, created_at, updated_at
             FROM entity_detail_mappings WHERE entity_id = ?1 AND project_id = ?2",
            params![entity_id, project_id],
            Self::from_row,
        )
    }

    /// Get a detail mapping for one entity in a specific data epoch.
    pub fn get_by_entity_id_at_epoch(
        conn: &Connection,
        entity_id: i64,
        project_id: i64,
        epoch: i64,
    ) -> Result<Option<EntityDetailMapping>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids,
                    chunk_count, created_at, updated_at
             FROM entity_detail_mappings
             WHERE entity_id = ?1 AND project_id = ?2 AND epoch = ?3",
            params![entity_id, project_id, epoch],
            Self::from_row,
        )
    }

    /// Update Qdrant point IDs for an entity at a specific epoch (project-scoped)
    pub fn update_qdrant_point_ids(
        tx: &rusqlite::Transaction,
        entity_id: i64,
        project_id: i64,
        epoch: i64,
        point_ids_json: &str,
        chunk_count: i64,
    ) -> Result<usize, StorageError> {
        use crate::utils::current_timestamp;

        tx.execute(
            "UPDATE entity_detail_mappings
             SET qdrant_point_ids = ?4, chunk_count = ?5, updated_at = ?6
             WHERE entity_id = ?1 AND project_id = ?2 AND epoch = ?3",
            params![
                entity_id,
                project_id,
                epoch,
                point_ids_json,
                chunk_count,
                current_timestamp()
            ],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update Qdrant point IDs for entity {}: {}",
                entity_id, e
            ))
        })
    }

    /// Update BM25 document IDs for an entity at a specific epoch (project-scoped)
    pub fn update_bm25_doc_ids(
        tx: &rusqlite::Transaction,
        entity_id: i64,
        project_id: i64,
        epoch: i64,
        doc_ids_json: &str,
    ) -> Result<usize, StorageError> {
        use crate::utils::current_timestamp;

        tx.execute(
            "UPDATE entity_detail_mappings
             SET bm25_doc_ids = ?4, updated_at = ?5
             WHERE entity_id = ?1 AND project_id = ?2 AND epoch = ?3",
            params![
                entity_id,
                project_id,
                epoch,
                doc_ids_json,
                current_timestamp()
            ],
        )
        .map_err(|e| {
            StorageError::update(format!(
                "Failed to update BM25 doc IDs for entity {}: {}",
                entity_id, e
            ))
        })
    }

    /// Delete detail mapping by entity ID (project-scoped, all epochs)
    pub fn delete_by_entity_id(
        tx: &rusqlite::Transaction,
        entity_id: i64,
        project_id: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM entity_detail_mappings WHERE entity_id = ?1 AND project_id = ?2",
            params![entity_id, project_id],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete entity detail mapping for entity {}: {}",
                entity_id, e
            ))
        })
    }

    /// Delete detail mapping by entity ID at a specific epoch (project-scoped)
    pub fn delete_by_entity_id_at_epoch(
        tx: &rusqlite::Transaction,
        entity_id: i64,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM entity_detail_mappings WHERE entity_id = ?1 AND project_id = ?2 AND epoch = ?3",
            params![entity_id, project_id, epoch],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete entity detail mapping for entity {} at epoch {}: {}",
                entity_id, epoch, e
            ))
        })
    }

    /// Delete detail mappings for all entities in a file at a specific epoch
    /// Uses JOIN with entities table to find entities by file_id
    pub fn delete_by_file_id_at_epoch(
        tx: &rusqlite::Transaction,
        file_id: i64,
        epoch: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM entity_detail_mappings
             WHERE epoch = ?2 AND entity_id IN (SELECT id FROM entities WHERE file_id = ?1 AND epoch = ?2)",
            params![file_id, epoch],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete entity detail mappings for file {} at epoch {}: {}",
                file_id, epoch, e
            ))
        })
    }

    /// Delete detail mappings for all entities in a file
    /// Uses JOIN with entities table to find entities by file_id
    pub fn delete_by_file_id(
        tx: &rusqlite::Transaction,
        file_id: i64,
    ) -> Result<usize, StorageError> {
        tx.execute(
            "DELETE FROM entity_detail_mappings
             WHERE entity_id IN (SELECT id FROM entities WHERE file_id = ?1)",
            params![file_id],
        )
        .map_err(|e| {
            StorageError::delete(format!(
                "Failed to delete entity detail mappings for file {}: {}",
                file_id, e
            ))
        })
    }

    /// Get all detail mappings
    pub fn get_all(conn: &Connection) -> Result<Vec<EntityDetailMapping>, StorageError> {
        execute_query(
            conn,
            "SELECT id, entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids,
                    chunk_count, created_at, updated_at
             FROM entity_detail_mappings",
            params![],
            Self::from_row,
        )
    }

    /// Count total detail mappings
    pub fn count(conn: &Connection) -> Result<i64, StorageError> {
        execute_count(
            conn,
            "SELECT COUNT(*) FROM entity_detail_mappings",
            params![],
            "entity detail mappings",
        )
    }

    /// From row helper
    fn from_row(row: &Row) -> Result<EntityDetailMapping, rusqlite::Error> {
        Ok(EntityDetailMapping {
            id: row.get(0)?,
            entity_id: row.get(1)?,
            project_id: row.get(2)?,
            epoch: row.get(3)?,
            qdrant_point_ids: row.get(4)?,
            bm25_doc_ids: row.get(5)?,
            chunk_count: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_detail_mapping_creation() {
        let mapping = EntityDetailMapping::new(1);

        assert_eq!(mapping.entity_id, 1);
        assert_eq!(mapping.qdrant_point_ids, "[]");
        assert_eq!(mapping.bm25_doc_ids, "[]");
        assert_eq!(mapping.chunk_count, 0);
    }

    #[test]
    fn test_entity_detail_mapping_builders() {
        let mapping = EntityDetailMapping::new(1)
            .with_qdrant_point_ids(&["point_1".to_string(), "point_2".to_string()])
            .with_bm25_doc_ids(&["doc_1".to_string()]);

        let point_ids = mapping.get_qdrant_point_ids();
        assert_eq!(point_ids.len(), 2);
        assert!(point_ids.contains(&"point_1".to_string()));
        assert!(point_ids.contains(&"point_2".to_string()));
        assert_eq!(mapping.chunk_count, 2);

        let doc_ids = mapping.get_bm25_doc_ids();
        assert_eq!(doc_ids.len(), 1);
        assert!(doc_ids.contains(&"doc_1".to_string()));
    }
}
