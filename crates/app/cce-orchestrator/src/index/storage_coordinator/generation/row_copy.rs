//! Cross-generation copying of single-file rows.
//!
//! Supports chunking-drift sweeps: a re-chunked unchanged file receives own
//! target-generation rows so the vector-store flow can regenerate its detail
//! mappings against fresh point ids.

use crate::error::OrchestratorError;

use super::StorageCoordinator;
use super::sqlite_compaction::{EntityCopyRow, insert_entity_copies_tx};

impl StorageCoordinator {
    /// Copy one file's `files` and `entities` rows from an ancestor
    /// generation into the target generation.
    ///
    /// Used by chunking-drift sweeps: a re-chunked unchanged file gets own
    /// target-epoch entity rows so the vector-store flow can write fresh
    /// `entity_detail_mappings` pointing at the new point ids. Without the
    /// copy, its mappings stay behind in the ancestor generation and are lost
    /// once compaction retires that ancestor.
    ///
    /// No-op when the file already owns a target-generation row (it is being
    /// handled by the normal change flow) or has no source row. Parent
    /// references are remapped to the copied rows. Existing detail mappings
    /// and chunks are deliberately not copied: mappings regenerate against
    /// the freshly stored chunks, and the sweep writes the chunks itself.
    ///
    /// Returns the number of copied entity rows (0 = no-op).
    pub(crate) fn copy_file_rows_between_epochs(
        &self,
        source_epoch: i64,
        target_epoch: i64,
        path: &str,
    ) -> Result<usize, OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(0);
        };
        client
            .with_transaction(|tx| {
                // A target-generation row means the normal change flow owns
                // this file; never shadow it with ancestor copies.
                let target_exists: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM files
                         WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
                        rusqlite::params![self.project_id, target_epoch, path],
                        |row| row.get(0),
                    )
                    .ok();
                if target_exists.is_some() {
                    return Ok(0);
                }

                let source_file = tx
                    .query_row(
                        "SELECT language, category, last_modified, created_at, content_hash, batch_id
                         FROM files
                         WHERE project_id = ?1 AND epoch = ?2 AND path = ?3",
                        rusqlite::params![self.project_id, source_epoch, path],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, u8>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, i64>(5)?,
                            ))
                        },
                    )
                    .ok();
                let Some((language, category, last_modified, created_at, content_hash, batch_id)) =
                    source_file
                else {
                    return Ok(0);
                };
                tx.execute(
                    "INSERT INTO files
                        (path, language, category, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        path,
                        language,
                        category,
                        last_modified,
                        created_at,
                        self.project_id,
                        content_hash,
                        target_epoch,
                        batch_id,
                    ],
                )
                .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                let new_file_id = tx.last_insert_rowid();

                let entities = {
                    let mut statement = tx
                        .prepare(
                            "SELECT e.id, e.name, e.kind, e.file_id, e.signature,
                                    e.span_start_row, e.span_end_row, e.span_start_column,
                                    e.span_end_column, e.span_start_byte, e.span_end_byte,
                                    e.scoped_name, e.depth, e.parent_id, e.metadata,
                                    e.parameters_json, e.return_type, e.doc_comment,
                                    e.modifiers_json, e.batch_id
                             FROM entities e
                             JOIN files f ON f.id = e.file_id
                             WHERE e.project_id = ?1 AND e.epoch = ?2 AND f.path = ?3
                                AND f.epoch = ?2 AND f.project_id = e.project_id",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = statement
                        .query_map(rusqlite::params![self.project_id, source_epoch, path], |row| {
                            EntityCopyRow::from_row(row)
                        })
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?
                };
                let entity_ids = insert_entity_copies_tx(
                    tx,
                    self.project_id,
                    target_epoch,
                    entities,
                    |_| Some(new_file_id),
                )?;

                Ok(entity_ids.len())
            })
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cce_storage_sqlite::{NewProjectRecord, ProjectRepository, SqliteClient};

    use super::super::StorageCoordinator;

    fn seed_sweep_source_file(client: &SqliteClient) {
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO files (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
                     VALUES ('src/lib.rs', 'rust', 1, 1, 1, 'hash-1', 5, 2)",
                    [],
                )
                .map_err(|e| cce_types::StorageError::insert(e.to_string()))?;
                let file_id: i64 = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO entities (name, kind, file_id, signature, scoped_name, metadata, project_id, epoch, batch_id)
                     VALUES ('alpha', 'function', ?1, 'fn alpha()', 'alpha',
                             '{\"__source_entity_id\":\"100\"}', 1, 5, 2)",
                    rusqlite::params![file_id],
                )
                .map_err(|e| cce_types::StorageError::insert(e.to_string()))?;
                let parent_db_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO entities (name, kind, file_id, signature, scoped_name, parent_id, metadata, project_id, epoch, batch_id)
                     VALUES ('nested', 'function', ?1, 'fn nested()', 'alpha::nested', ?2,
                             '{\"__source_entity_id\":\"101\"}', 1, 5, 2)",
                    rusqlite::params![file_id, parent_db_id],
                )
                .map_err(|e| cce_types::StorageError::insert(e.to_string()))?;
                Ok(())
            })
            .expect("seed source generation");
    }

    #[test]
    fn copy_file_rows_between_epochs_materializes_entities() {
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
        seed_sweep_source_file(&client);

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);

        let copied = storage
            .copy_file_rows_between_epochs(5, 6, "src/lib.rs")
            .expect("copy must succeed");
        assert_eq!(copied, 2, "both entities must be copied");

        let conn = client.read_connection().expect("read connection");
        let content_hash: String = conn
            .query_row(
                "SELECT content_hash FROM files WHERE project_id = 1 AND epoch = 6 AND path = 'src/lib.rs'",
                [],
                |row| row.get(0),
            )
            .expect("target file row");
        assert_eq!(content_hash, "hash-1");

        let (new_parent_id, new_child_id, child_parent_id): (i64, i64, i64) = conn
            .query_row(
                "SELECT parent.id, child.id, child.parent_id FROM entities parent
                 JOIN entities child ON child.scoped_name = 'alpha::nested'
                    AND child.project_id = 1 AND child.epoch = 6
                 WHERE parent.scoped_name = 'alpha' AND parent.project_id = 1 AND parent.epoch = 6",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("copied entities");
        assert_ne!(new_parent_id, new_child_id);
        assert_eq!(
            child_parent_id, new_parent_id,
            "parent references must be remapped to the copied rows"
        );

        let source_entity_id: String = conn
            .query_row(
                "SELECT metadata FROM entities WHERE id = ?1",
                [new_parent_id],
                |row| row.get(0),
            )
            .expect("copied metadata");
        assert!(
            source_entity_id.contains("__source_entity_id"),
            "source entity ids must survive the copy so detail mappings regenerate"
        );
        drop(conn);

        // A second copy is a no-op: the target already owns the file row.
        let again = storage
            .copy_file_rows_between_epochs(5, 6, "src/lib.rs")
            .expect("second copy must succeed");
        assert_eq!(again, 0, "existing target rows must not be duplicated");
    }

    #[test]
    fn copy_file_rows_is_a_noop_for_unknown_paths() {
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

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        let copied = storage
            .copy_file_rows_between_epochs(5, 6, "missing.rs")
            .expect("unknown path is a legitimate no-op");
        assert_eq!(copied, 0);
    }
}
