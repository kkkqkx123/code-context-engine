//! SQLite-side compaction: materializing inherited generations into full ones.
//!
//! Also hosts the shared entity-row copying primitive used by both full
//! materialization and single-file cross-generation copies.

use std::collections::HashMap;

use crate::error::OrchestratorError;

use super::super::mapping::{replace_epoch_in_id, replace_epoch_in_id_list};
use super::StorageCoordinator;

/// One entity row staged for insertion into a target generation.
///
/// `old_file_id`/`old_parent_id` refer to source-generation rows; callers
/// remap them through the already-copied target rows during insertion.
pub(super) struct EntityCopyRow {
    old_id: i64,
    old_file_id: i64,
    name: String,
    kind: String,
    signature: Option<String>,
    span_start_row: Option<i64>,
    span_end_row: Option<i64>,
    span_start_column: Option<i64>,
    span_end_column: Option<i64>,
    span_start_byte: Option<i64>,
    span_end_byte: Option<i64>,
    scoped_name: Option<String>,
    depth: Option<i64>,
    old_parent_id: Option<i64>,
    metadata: Option<String>,
    parameters_json: Option<String>,
    return_type: Option<String>,
    doc_comment: Option<String>,
    modifiers_json: Option<String>,
    batch_id: i64,
}

impl EntityCopyRow {
    /// Column order contract:
    /// `id, name, kind, file_id, signature, span_start_row, span_end_row,
    /// span_start_column, span_end_column, span_start_byte, span_end_byte,
    /// scoped_name, depth, parent_id, metadata, parameters_json, return_type,
    /// doc_comment, modifiers_json, batch_id`.
    pub(super) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            old_id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            old_file_id: row.get(3)?,
            signature: row.get(4)?,
            span_start_row: row.get(5)?,
            span_end_row: row.get(6)?,
            span_start_column: row.get(7)?,
            span_end_column: row.get(8)?,
            span_start_byte: row.get(9)?,
            span_end_byte: row.get(10)?,
            scoped_name: row.get(11)?,
            depth: row.get(12)?,
            old_parent_id: row.get(13)?,
            metadata: row.get(14)?,
            parameters_json: row.get(15)?,
            return_type: row.get(16)?,
            doc_comment: row.get(17)?,
            modifiers_json: row.get(18)?,
            batch_id: row.get(19)?,
        })
    }
}

/// Copy staged entity rows into `target_epoch` inside the current transaction.
///
/// Rows whose source file has no target counterpart (per `target_file_id`)
/// are skipped. Parent references are remapped to the freshly inserted rows;
/// parents that were not copied keep `NULL`. Returns the
/// `source entity id -> target entity id` map for downstream detail-mapping
/// copies.
pub(super) fn insert_entity_copies_tx(
    tx: &rusqlite::Transaction<'_>,
    project_id: i64,
    target_epoch: i64,
    entities: Vec<EntityCopyRow>,
    target_file_id: impl Fn(i64) -> Option<i64>,
) -> Result<HashMap<i64, i64>, cce_types::StorageError> {
    let mut entity_ids = HashMap::new();
    let mut parent_updates = Vec::new();
    for entity in entities {
        let Some(new_file_id) = target_file_id(entity.old_file_id) else {
            continue;
        };
        tx.execute(
            "INSERT INTO entities
                (name, kind, file_id, signature, span_start_row, span_end_row,
                 span_start_column, span_end_column, span_start_byte, span_end_byte,
                 scoped_name, depth, parent_id, metadata, parameters_json, return_type,
                 doc_comment, modifiers_json, project_id, epoch, batch_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL,
                     ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            rusqlite::params![
                entity.name,
                entity.kind,
                new_file_id,
                entity.signature,
                entity.span_start_row,
                entity.span_end_row,
                entity.span_start_column,
                entity.span_end_column,
                entity.span_start_byte,
                entity.span_end_byte,
                entity.scoped_name,
                entity.depth,
                entity.metadata,
                entity.parameters_json,
                entity.return_type,
                entity.doc_comment,
                entity.modifiers_json,
                project_id,
                target_epoch,
                entity.batch_id,
            ],
        )
        .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
        let new_id = tx.last_insert_rowid();
        entity_ids.insert(entity.old_id, new_id);
        parent_updates.push((new_id, entity.old_parent_id));
    }

    for (new_id, old_parent_id) in parent_updates {
        if let Some(old_parent_id) = old_parent_id
            && let Some(parent_new_id) = entity_ids.get(&old_parent_id)
        {
            tx.execute(
                "UPDATE entities SET parent_id = ?1 WHERE id = ?2",
                rusqlite::params![parent_new_id, new_id],
            )
            .map_err(|error| cce_types::StorageError::update(error.to_string()))?;
        }
    }
    Ok(entity_ids)
}

impl StorageCoordinator {
    /// Delete the target-generation rows of an explicit path set, children
    /// before parents so foreign keys stay satisfied.
    ///
    /// Scoped to exactly these paths: rows of other files in the target
    /// generation (its own changed/new files) are never touched. Used by
    /// [`Self::materialize_sqlite_generation`] to keep compaction restartable.
    fn clear_target_rows_for_paths_tx(
        tx: &rusqlite::Transaction<'_>,
        project_id: i64,
        target_epoch: i64,
        paths: &[String],
    ) -> Result<(), cce_types::StorageError> {
        if paths.is_empty() {
            return Ok(());
        }
        let placeholders = paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let scope = format!(" AND path IN ({placeholders})");
        // `chunks.file_path` carries the project-relative file path, while
        // `chunks.path` is the point-type marker ('emb'/'bm25'/'nl').
        let chunk_scope = format!(" AND file_path IN ({placeholders})");
        for sql in [
            format!(
                "DELETE FROM entity_detail_mappings WHERE project_id = ?1 AND epoch = ?2 AND entity_id IN
                 (SELECT id FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                  (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2{scope}))"
            ),
            format!("DELETE FROM chunks WHERE project_id = ?1 AND epoch = ?2{chunk_scope}"),
            format!(
                "DELETE FROM file_summaries WHERE epoch = ?2 AND file_id IN
                 (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2{scope})"
            ),
            format!(
                "DELETE FROM entities WHERE project_id = ?1 AND epoch = ?2 AND file_id IN
                 (SELECT id FROM files WHERE project_id = ?1 AND epoch = ?2{scope})"
            ),
            format!("DELETE FROM files WHERE project_id = ?1 AND epoch = ?2{scope}"),
        ] {
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &target_epoch];
            for path in paths {
                params.push(path);
            }
            tx.execute(&sql, params.as_slice()).map_err(|error| {
                cce_types::StorageError::delete(format!(
                    "failed to clear materialization residue: {error}"
                ))
            })?;
        }
        Ok(())
    }

    /// Materialize an inherited generation into a complete one (compaction).
    ///
    /// Copies every parent-generation row that is not hidden by an override
    /// into `target_epoch`, merging with the rows the target already owns.
    /// Afterwards the target no longer depends on its parent and can be
    /// published as a full generation. Used only by compaction — candidate
    /// creation registers inheritance instead of copying.
    ///
    /// Idempotent under crash retry: rows of the copied source paths are
    /// deleted from the target inside the same transaction before the copy,
    /// so an interrupted attempt never leaves unique-constraint residue that
    /// would wedge every later compaction.
    ///
    /// `excluded_paths` lists the overridden files of the target; their
    /// parent rows must not be copied (replaced files already own newer
    /// rows, deleted files must stay invisible).
    pub(crate) fn materialize_sqlite_generation(
        &self,
        source_epoch: i64,
        target_epoch: i64,
        excluded_paths: &[String],
    ) -> Result<(), OrchestratorError> {
        let Some(client) = self.metadata_store.as_ref().map(|store| store.as_ref()) else {
            return Ok(());
        };
        let exclusion_sql = if excluded_paths.is_empty() {
            String::new()
        } else {
            let placeholders = excluded_paths
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(" AND path NOT IN ({placeholders})")
        };

        client
            .with_transaction(|tx| {
                // Crash-retry guard: drop any target rows left behind by an
                // interrupted earlier attempt for exactly the source paths
                // being copied. Target-owned rows of other files (changed or
                // newly added, all covered by overrides) are untouched.
                let source_paths: Vec<String> = {
                    let sql = format!(
                        "SELECT path FROM files
                         WHERE project_id = ?1 AND epoch = ?2{exclusion_sql}"
                    );
                    let mut statement = tx
                        .prepare(&sql)
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let mut params: Vec<&dyn rusqlite::ToSql> =
                        vec![&self.project_id, &source_epoch];
                    for path in excluded_paths {
                        params.push(path);
                    }
                    let rows = statement
                        .query_map(params.as_slice(), |row| row.get::<_, String>(0))
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?
                };
                Self::clear_target_rows_for_paths_tx(
                    tx,
                    self.project_id,
                    target_epoch,
                    &source_paths,
                )?;

                // Files (excluding overridden paths).
                let files_sql = format!(
                    "SELECT id, path, language, category, last_modified, created_at, content_hash, batch_id
                     FROM files WHERE project_id = ?1 AND epoch = ?2{exclusion_sql}"
                );
                let files = {
                    let mut statement = tx
                        .prepare(&files_sql)
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let mut params: Vec<&dyn rusqlite::ToSql> =
                        vec![&self.project_id, &source_epoch];
                    for path in excluded_paths {
                        params.push(path);
                    }
                    let rows = statement
                        .query_map(params.as_slice(), |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, u8>(3)?,
                                row.get::<_, i64>(4)?,
                                row.get::<_, i64>(5)?,
                                row.get::<_, Option<String>>(6)?,
                                row.get::<_, i64>(7)?,
                            ))
                        })
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?
                };
                let mut file_ids = HashMap::new();
                for (old_id, path, language, category, last_modified, created_at, _hash, batch_id) in files {
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
                            _hash,
                            target_epoch,
                            batch_id,
                        ],
                    )
                    .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                    file_ids.insert(old_id, tx.last_insert_rowid());
                }

                // Entities whose file was not copied are skipped implicitly;
                // the same applies to their detail mappings below.
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
                             WHERE e.project_id = ?1 AND e.epoch = ?2",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = statement
                        .query_map(rusqlite::params![self.project_id, source_epoch], |row| {
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
                    |old_file_id| file_ids.get(&old_file_id).copied(),
                )?;

                let mappings = {
                    let mut statement = tx
                        .prepare(
                            "SELECT entity_id, qdrant_point_ids, bm25_doc_ids, chunk_count,
                                    created_at, updated_at
                             FROM entity_detail_mappings
                             WHERE project_id = ?1 AND epoch = ?2",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = statement
                        .query_map(rusqlite::params![self.project_id, source_epoch], |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, i64>(3)?,
                                row.get::<_, i64>(4)?,
                                row.get::<_, i64>(5)?,
                            ))
                        })
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?
                };
                for (
                    old_entity_id,
                    qdrant_point_ids,
                    bm25_doc_ids,
                    chunk_count,
                    created_at,
                    updated_at,
                ) in mappings
                {
                    let Some(new_entity_id) = entity_ids.get(&old_entity_id) else {
                        continue;
                    };
                    tx.execute(
                        "INSERT INTO entity_detail_mappings
                            (entity_id, project_id, epoch, qdrant_point_ids, bm25_doc_ids,
                             chunk_count, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        rusqlite::params![
                            new_entity_id,
                            self.project_id,
                            target_epoch,
                            replace_epoch_in_id_list(&qdrant_point_ids, source_epoch, target_epoch),
                            replace_epoch_in_id_list(&bm25_doc_ids, source_epoch, target_epoch),
                            chunk_count,
                            created_at,
                            updated_at,
                        ],
                    )
                    .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                }

                // Chunks of non-overridden files only: overridden files own
                // newer candidate rows under the same chunk IDs.
                let chunks_exclusion_sql = if excluded_paths.is_empty() {
                    " AND epoch = ?3".to_string()
                } else {
                    let placeholders = excluded_paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    format!(
                        " AND epoch = ?3 AND file_path NOT IN ({placeholders})"
                    )
                };
                let chunks_sql = format!(
                    "INSERT INTO chunks
                        (chunk_id, file_path, content, start_line, end_line,
                         entity_ids, entity_names, chunk_type, created_at, updated_at,
                         project_id, epoch, batch_id)
                     SELECT chunk_id, file_path, content, start_line, end_line,
                            entity_ids, entity_names, chunk_type, created_at, updated_at,
                            project_id, ?2, batch_id
                     FROM chunks WHERE project_id = ?1{chunks_exclusion_sql}"
                );
                {
                    let mut statement = tx
                        .prepare(&chunks_sql)
                        .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                    let mut params: Vec<&dyn rusqlite::ToSql> =
                        vec![&self.project_id, &target_epoch, &source_epoch];
                    for path in excluded_paths {
                        params.push(path);
                    }
                    statement
                        .execute(params.as_slice())
                        .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                }

                let summaries = {
                    let mut statement = tx
                        .prepare(
                            "SELECT new_file.id, summary.summary_json,
                                    summary.qdrant_point_id, summary.bm25_doc_id,
                                    summary.created_at, summary.updated_at
                             FROM file_summaries summary
                             JOIN files old_file ON old_file.id = summary.file_id
                             JOIN files new_file ON new_file.path = old_file.path
                                AND new_file.project_id = old_file.project_id
                                AND new_file.epoch = ?3
                             WHERE old_file.project_id = ?1 AND old_file.epoch = ?2
                                AND summary.epoch = ?2",
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    let rows = statement
                        .query_map(
                            rusqlite::params![self.project_id, source_epoch, target_epoch],
                            |row| {
                                Ok((
                                    row.get::<_, i64>(0)?,
                                    row.get::<_, Option<String>>(1)?,
                                    row.get::<_, Option<String>>(2)?,
                                    row.get::<_, Option<String>>(3)?,
                                    row.get::<_, String>(4)?,
                                    row.get::<_, String>(5)?,
                                ))
                            },
                        )
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?;
                    rows.collect::<Result<Vec<_>, _>>()
                        .map_err(|error| cce_types::StorageError::query(error.to_string()))?
                };
                for (
                    new_file_id,
                    summary_json,
                    qdrant_point_id,
                    bm25_doc_id,
                    created_at,
                    updated_at,
                ) in summaries
                {
                    // Copy the canonical JSON blob verbatim; the structured
                    // columns are generated from it and must never be written.
                    tx.execute(
                        "INSERT INTO file_summaries
                            (file_id, epoch, summary_json, qdrant_point_id, bm25_doc_id,
                             created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        rusqlite::params![
                            new_file_id,
                            target_epoch,
                            summary_json,
                            qdrant_point_id
                                .map(|id| replace_epoch_in_id(&id, source_epoch, target_epoch)),
                            bm25_doc_id
                                .map(|id| replace_epoch_in_id(&id, source_epoch, target_epoch)),
                            created_at,
                            updated_at,
                        ],
                    )
                    .map_err(|error| cce_types::StorageError::insert(error.to_string()))?;
                }
                Ok(())
            })
            .map_err(OrchestratorError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cce_storage_sqlite::{NewProjectRecord, ProjectRepository, SqliteClient};

    use super::super::StorageCoordinator;

    fn insert_file_row(tx: &rusqlite::Transaction<'_>, path: &str, epoch: i64, hash: &str) {
        tx.execute(
            "INSERT INTO files
                (path, language, last_modified, created_at, project_id, content_hash, epoch, batch_id)
             VALUES (?1, 'Rust', 1, 1, 1, ?2, ?3, 0)",
            rusqlite::params![path, hash, epoch],
        )
        .expect("file row should insert");
    }

    fn insert_chunk_row(tx: &rusqlite::Transaction<'_>, path: &str, epoch: i64, content: &str) {
        tx.execute(
            "INSERT INTO chunks
                (chunk_id, file_path, content, start_line, end_line, chunk_type,
                 created_at, updated_at, project_id, epoch)
             VALUES (?1, ?2, ?3, 0, 5, 'function', 1, 1, 1, ?4)",
            rusqlite::params![format!("{path}::{epoch}"), path, content, epoch],
        )
        .expect("chunk row should insert");
    }

    /// A crashed earlier compaction attempt may have committed part of the
    /// copy. Re-running must succeed (no unique-constraint wedging), replace
    /// the residue with the source rows, and leave target-owned rows of
    /// overridden files untouched.
    #[test]
    fn materialize_is_restartable_after_partial_copy() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let client = database.as_ref().clone();
        client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new("test".to_string(), "/tmp/test".to_string()),
                )
                .map(|_| ())?;
                // Source generation 1: two files.
                insert_file_row(tx, "src/keep.rs", 1, "hash-keep");
                insert_file_row(tx, "src/replaced.rs", 1, "hash-old");
                insert_chunk_row(tx, "src/keep.rs", 1, "keep-source-content");
                insert_chunk_row(tx, "src/replaced.rs", 1, "replaced-parent-content");
                // Target generation 2 owns a newer row for the replaced file…
                insert_file_row(tx, "src/replaced.rs", 2, "hash-new");
                insert_chunk_row(tx, "src/replaced.rs", 2, "replaced-own-newer");
                // …plus crash residue from an interrupted compaction.
                insert_file_row(tx, "src/keep.rs", 2, "hash-stale-residue");
                insert_chunk_row(tx, "src/keep.rs", 2, "stale-partial-copy");
                Ok(())
            })
            .expect("seed should succeed");

        let storage = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(database);
        storage
            .materialize_sqlite_generation(1, 2, &["src/replaced.rs".to_string()])
            .expect("materialization over residue must succeed");

        let conn = client.read_connection().expect("connection should open");
        let chunk_content = |path: &str| -> String {
            conn.query_row(
                "SELECT content FROM chunks WHERE project_id = 1 AND epoch = 2 AND file_path = ?1",
                rusqlite::params![path],
                |row| row.get(0),
            )
            .expect("target chunk should exist")
        };
        assert_eq!(
            chunk_content("src/keep.rs"),
            "keep-source-content",
            "residue must be replaced by the source rows"
        );
        assert_eq!(
            chunk_content("src/replaced.rs"),
            "replaced-own-newer",
            "overridden files' own newer rows must survive"
        );
        let file_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE project_id = 1 AND epoch = 2",
                [],
                |row| row.get(0),
            )
            .expect("count target files");
        assert_eq!(file_count, 2, "no duplicate path rows may accumulate");

        drop(conn);
        // Re-running the whole compaction stays idempotent.
        storage
            .materialize_sqlite_generation(1, 2, &["src/replaced.rs".to_string()])
            .expect("materialization must be restartable");
    }
}
