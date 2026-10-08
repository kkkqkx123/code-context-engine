use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_all(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_project ON files(project_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_files_project_path ON files(project_id, path)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project ON entities(project_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project_name ON entities(project_id, name)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project_kind ON entities(project_id, kind)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project_file ON entities(project_id, file_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project_file_kind ON entities(project_id, file_id, kind)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_file ON entities(file_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_file_epoch ON entities(file_id, epoch)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_relation_snapshot_entities_file
         ON relation_snapshot_entities(project_id, relation_epoch, file_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_relation_snapshot_entities_entity_id
         ON relation_snapshot_entities(project_id, relation_epoch, entity_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_project_parent ON entities(project_id, parent_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_detail_mappings_project_epoch_entity ON entity_detail_mappings(project_id, epoch, entity_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "DROP INDEX IF EXISTS idx_detail_mappings_project_entity",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to drop old index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_file_summaries_file_epoch ON file_summaries(file_id, epoch)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project ON chunks(project_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_file ON chunks(project_id, file_path)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_file_epoch
         ON chunks(project_id, file_path, epoch)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_chunk_id ON chunks(project_id, chunk_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_chunk_id_epoch
         ON chunks(project_id, chunk_id, epoch)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_type ON chunks(project_id, chunk_type)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chunks_project_file_lines ON chunks(project_id, file_path, start_line, end_line)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute("DROP INDEX IF EXISTS idx_entities_symbol_key", [])
        .map_err(|e| StorageError::Table(format!("Failed to migrate symbol index: {}", e)))?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_entities_symbol_key ON entities(project_id, epoch, file_id, scoped_name, kind)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_project_meta_project_key ON project_meta(project_id, key)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_time ON metrics_aggregated(timestamp)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_name_time ON metrics_aggregated(metric_name, timestamp)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_project_time ON metrics_aggregated(project_id, timestamp)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_operation_time ON metrics_aggregated(operation_type, timestamp)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_metrics_project_operation_time ON metrics_aggregated(project_id, operation_type, timestamp)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index: {}", e)))?;

    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(name, signature, content='entities', content_rowid='id')",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create FTS5 table: {}", e)))?;

    conn.execute_batch(
        "DROP TRIGGER IF EXISTS entities_ai;
        DROP TRIGGER IF EXISTS entities_ad;
        DROP TRIGGER IF EXISTS entities_au;
        CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
            INSERT INTO entities_fts(rowid, name, signature) VALUES (new.id, new.name, new.signature);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
            INSERT INTO entities_fts(entities_fts, rowid, name, signature) VALUES('delete', old.id, old.name, old.signature);
        END;
        CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities
            WHEN old.name IS NOT new.name OR old.signature IS NOT new.signature BEGIN
            INSERT INTO entities_fts(entities_fts, rowid, name, signature) VALUES('delete', old.id, old.name, old.signature);
            INSERT INTO entities_fts(rowid, name, signature) VALUES (new.id, new.name, new.signature);
        END;",
    )
    .map_err(|e| StorageError::Table(format!("Failed to create FTS5 triggers: {}", e)))?;

    conn.execute("DROP TABLE IF EXISTS write_retry", [])
        .map_err(|e| {
            StorageError::Table(format!("Failed to drop legacy write_retry table: {}", e))
        })?;

    Ok(())
}
