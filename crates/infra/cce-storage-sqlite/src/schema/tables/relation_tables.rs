//! Relation snapshot tables.

use rusqlite::Connection;

use cce_types::StorageError;

pub fn create_tables(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS relation_snapshot_manifest (
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            operation_id TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('building', 'ready', 'active', 'failed', 'delta')),
            schema_version INTEGER NOT NULL,
            parser_version INTEGER NOT NULL,
            resolver_version INTEGER NOT NULL,
            path_normalization_version INTEGER NOT NULL,
            config_fingerprint TEXT NOT NULL,
            input_fingerprint TEXT,
            snapshot_fingerprint TEXT,
            file_count INTEGER,
            entity_count INTEGER,
            relation_count INTEGER,
            dependency_count INTEGER,
            created_at INTEGER NOT NULL,
            validated_at INTEGER,
            activated_at INTEGER,
            failure_reason TEXT,
            symbol_key_conflict_count INTEGER NOT NULL DEFAULT 0,
            symbol_key_conflict_samples_json TEXT,
            PRIMARY KEY(project_id, relation_epoch),
            FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_relation_manifest_operation
            ON relation_snapshot_manifest(project_id, operation_id);
         CREATE TABLE IF NOT EXISTS relation_snapshot_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            path TEXT NOT NULL,
            language TEXT NOT NULL,
            input_hash TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            imports_json TEXT NOT NULL,
            UNIQUE(project_id, relation_epoch, path),
            FOREIGN KEY(project_id, relation_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS relation_snapshot_entities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            file_id INTEGER NOT NULL,
            scoped_name TEXT NOT NULL,
            kind_json TEXT NOT NULL,
            overload_discriminator TEXT NOT NULL,
            entity_id INTEGER,
            name TEXT NOT NULL,
            signature TEXT NOT NULL,
            parameters_json TEXT NOT NULL,
            return_type TEXT,
            span_json TEXT NOT NULL,
            depth INTEGER NOT NULL,
            parent_symbol_id INTEGER,
            doc_comment TEXT,
            modifiers_json TEXT NOT NULL,
            attributes_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            is_stdlib INTEGER NOT NULL,
            stdlib_category_json TEXT,
            subtype TEXT,
            UNIQUE(project_id, relation_epoch, file_id, scoped_name, kind_json, overload_discriminator),
            FOREIGN KEY(project_id, relation_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE,
            FOREIGN KEY(file_id) REFERENCES relation_snapshot_files(id) ON DELETE CASCADE,
            FOREIGN KEY(parent_symbol_id) REFERENCES relation_snapshot_entities(id)
         );
         CREATE INDEX IF NOT EXISTS idx_relation_snapshot_entities_scoped_name
            ON relation_snapshot_entities(project_id, relation_epoch, scoped_name);
         CREATE TABLE IF NOT EXISTS relation_snapshot_relations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            caller_symbol_id INTEGER NOT NULL,
            target_symbol_id INTEGER,
            target_state TEXT NOT NULL CHECK(target_state IN ('internal', 'external', 'unresolved')),
            raw_target TEXT NOT NULL,
            relation_type_json TEXT NOT NULL,
            span_json TEXT NOT NULL,
            external_type_json TEXT,
            unresolved_reason TEXT,
            stdlib_category_json TEXT,
            FOREIGN KEY(project_id, relation_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE,
            FOREIGN KEY(caller_symbol_id) REFERENCES relation_snapshot_entities(id),
            FOREIGN KEY(target_symbol_id) REFERENCES relation_snapshot_entities(id)
         );
         CREATE INDEX IF NOT EXISTS idx_relation_snapshot_relations_caller
            ON relation_snapshot_relations(project_id, relation_epoch, caller_symbol_id);
         CREATE INDEX IF NOT EXISTS idx_relation_snapshot_relations_target
            ON relation_snapshot_relations(project_id, relation_epoch, target_symbol_id);
         CREATE TABLE IF NOT EXISTS relation_snapshot_exports (
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            file_id INTEGER NOT NULL,
            symbol_id INTEGER NOT NULL,
            export_type TEXT NOT NULL,
            PRIMARY KEY(project_id, relation_epoch, file_id, symbol_id, export_type),
            FOREIGN KEY(project_id, relation_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE,
            FOREIGN KEY(file_id) REFERENCES relation_snapshot_files(id) ON DELETE CASCADE,
            FOREIGN KEY(symbol_id) REFERENCES relation_snapshot_entities(id)
         );
         CREATE TABLE IF NOT EXISTS relation_snapshot_dependencies (
            project_id INTEGER NOT NULL,
            relation_epoch INTEGER NOT NULL,
            source_file_id INTEGER NOT NULL,
            target_path TEXT NOT NULL,
            source TEXT NOT NULL,
            PRIMARY KEY(project_id, relation_epoch, source_file_id, target_path, source),
            FOREIGN KEY(project_id, relation_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE,
            FOREIGN KEY(source_file_id) REFERENCES relation_snapshot_files(id) ON DELETE CASCADE
         );
         CREATE TABLE IF NOT EXISTS relation_snapshot_deltas (
            project_id  INTEGER NOT NULL,
            base_epoch  INTEGER NOT NULL,
            delta_epoch INTEGER NOT NULL,
            delta_data  BLOB    NOT NULL,
            size_bytes  INTEGER NOT NULL,
            PRIMARY KEY (project_id, delta_epoch),
            FOREIGN KEY (project_id, base_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE,
            FOREIGN KEY (project_id, delta_epoch)
                REFERENCES relation_snapshot_manifest(project_id, relation_epoch)
                ON DELETE CASCADE
         );",
    )
    .map_err(|error| {
        StorageError::Table(format!("Failed to create relation snapshot tables: {error}"))
    })?;

    Ok(())
}
