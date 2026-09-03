//! Core domain tables: projects, files, entities, project_meta.

use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_tables(conn: &Connection) -> Result<(), StorageError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            root_path TEXT NOT NULL UNIQUE,
            config_file_path TEXT NOT NULL DEFAULT '.cce/config.json',
            language TEXT,
            extensions TEXT,
            exclude_dirs TEXT,
            respect_gitignore INTEGER,
            ignore_patterns TEXT,
            last_indexed TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create projects table: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL,
            language TEXT NOT NULL,
            category INTEGER NOT NULL DEFAULT 4,
            last_modified INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            project_id INTEGER NOT NULL,
            content_hash TEXT,
            epoch INTEGER NOT NULL DEFAULT 0,
            batch_id INTEGER NOT NULL DEFAULT 0,
            UNIQUE(project_id, epoch, path),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create files table: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS entities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
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
            batch_id INTEGER NOT NULL DEFAULT 0,
            UNIQUE(project_id, epoch, file_id, scoped_name, kind),
            FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
            FOREIGN KEY (parent_id) REFERENCES entities(id) ON DELETE SET NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create entities table: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_meta (
            project_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (project_id, key),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create project_meta table: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_index_manifests (
            project_id INTEGER NOT NULL,
            publication_epoch INTEGER NOT NULL,
            data_epoch INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('building', 'active', 'failed')),
            input_fingerprint TEXT,
            created_at INTEGER NOT NULL,
            activated_at INTEGER,
            failure_reason TEXT,
            candidate_ready INTEGER NOT NULL DEFAULT 0,
            parent_data_epoch INTEGER,
            PRIMARY KEY (project_id, publication_epoch),
            UNIQUE (project_id, operation_id),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create project index manifests: {}", e)))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_project_index_manifest_active
         ON project_index_manifests(project_id, state, publication_epoch DESC)",
        [],
    )
    .map_err(|e| {
        StorageError::Table(format!(
            "Failed to create project index manifest index: {}",
            e
        ))
    })?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS generation_overrides (
            project_id INTEGER NOT NULL,
            epoch INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            disposition TEXT NOT NULL CHECK(disposition IN ('replaced', 'deleted')),
            PRIMARY KEY (project_id, epoch, file_path),
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| StorageError::Table(format!("Failed to create generation overrides: {}", e)))?;

    Ok(())
}
