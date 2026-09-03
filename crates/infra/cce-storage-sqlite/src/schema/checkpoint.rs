use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_all(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoint (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            operation_type TEXT NOT NULL,

            root_dir TEXT NOT NULL,
            total_files INTEGER NOT NULL,
            batch_size INTEGER NOT NULL,

            current_batch_index INTEGER NOT NULL DEFAULT 0,
            current_phase TEXT NOT NULL DEFAULT 'Scanning',

            file_list_hash TEXT,

            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_error TEXT,
            failure_count INTEGER DEFAULT 0,

            status TEXT DEFAULT 'in_progress',
            operation_mode TEXT,

            active_flag INTEGER DEFAULT 0,
            priority INTEGER NOT NULL DEFAULT 0,
            last_heartbeat TEXT,

            failed_at TEXT,

            UNIQUE(project_id, operation_id),
            CHECK (current_batch_index >= 0),
            CHECK (batch_size > 0),
            CHECK (active_flag IN (0, 1)),
            CHECK (priority IN (0, 1, 2, 3)),
            CHECK (status IN ('in_progress', 'completed', 'failed'))
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint table: {}", e)))?;

    if !table_has_column(conn, "checkpoint", "failed_at")? {
        conn.execute("ALTER TABLE checkpoint ADD COLUMN failed_at TEXT", [])
            .map_err(|e| {
                StorageError::Table(format!("Failed to add checkpoint.failed_at column: {}", e))
            })?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_operation ON checkpoint(operation_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_project_id ON checkpoint(project_id)",
        [],
    )
    .map_err(|e| {
        StorageError::Table(format!(
            "Failed to create checkpoint project_id index: {}",
            e
        ))
    })?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_project_operation ON checkpoint(project_id, operation_id)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint project_operation index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_status ON checkpoint(status)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint status index: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoint_batch (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            batch_index INTEGER NOT NULL,

            first_file TEXT NOT NULL,
            last_file TEXT NOT NULL,
            file_count INTEGER NOT NULL,

            processed_files INTEGER DEFAULT 0,
            failed_files INTEGER DEFAULT 0,

            entities_extracted INTEGER DEFAULT 0,
            relations_found INTEGER DEFAULT 0,
            chunks_generated INTEGER DEFAULT 0,
            vectors_stored INTEGER DEFAULT 0,

            start_time TEXT NOT NULL,
            end_time TEXT,
            duration_ms INTEGER,

            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,

            UNIQUE(project_id, operation_id, batch_index),
            FOREIGN KEY(project_id, operation_id) REFERENCES checkpoint(project_id, operation_id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint_batch table: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_batch_range ON checkpoint_batch(operation_id, first_file)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint_batch index: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoint_file (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            batch_index INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_id INTEGER,

            language TEXT,
            file_size INTEGER,
            content_hash TEXT,

            parsed_data TEXT,
            parse_error TEXT,
            summary_data TEXT,

            embedding_count INTEGER DEFAULT 0,
            bm25_doc_id TEXT,
            export_path TEXT,
            render_fingerprint TEXT,
            module_progress TEXT,

            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,

            UNIQUE(project_id, operation_id, file_path),
            FOREIGN KEY(project_id, operation_id, batch_index)
                REFERENCES checkpoint_batch(project_id, operation_id, batch_index) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint_file table: {}", e)))?;

    if !table_has_column(conn, "checkpoint_file", "summary_data")? {
        conn.execute(
            "ALTER TABLE checkpoint_file ADD COLUMN summary_data TEXT",
            [],
        )
        .map_err(|e| {
            StorageError::Table(format!(
                "Failed to add checkpoint_file.summary_data column: {}",
                e
            ))
        })?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_file_batch ON checkpoint_file(operation_id, batch_index)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint_file batch index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_active ON checkpoint(project_id, active_flag) WHERE active_flag = 1",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint active index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_heartbeat ON checkpoint(last_heartbeat) WHERE active_flag = 1",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint heartbeat index: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_checkpoint_ttl ON checkpoint(status, updated_at)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create checkpoint TTL index: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS work_unit_checkpoint (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            stage TEXT NOT NULL,
            target_epoch INTEGER NOT NULL,
            work_unit_hash TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            item_count INTEGER DEFAULT 0,

            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,

            UNIQUE(project_id, operation_id, stage, work_unit_hash),
            FOREIGN KEY(project_id, operation_id) REFERENCES checkpoint(project_id, operation_id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create work_unit_checkpoint table: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_work_unit_op_stage ON work_unit_checkpoint(project_id, operation_id, stage)",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create work_unit_checkpoint index: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS index_state_projection (
            project_id INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            version INTEGER NOT NULL,
            state_json TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY(project_id, operation_id, file_path),
            FOREIGN KEY(project_id, operation_id)
                REFERENCES checkpoint(project_id, operation_id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create index state projection: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_index_state_projection_file
         ON index_state_projection(project_id, file_path, version DESC)",
        [],
    )
    .map_err(|e| {
        StorageError::Table(format!(
            "Failed to create index state projection index: {}",
            e
        ))
    })?;

    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, StorageError> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| StorageError::Table(error.to_string()))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| StorageError::Table(error.to_string()))?;
    for result in columns {
        if result.map_err(|error| StorageError::Table(error.to_string()))? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
