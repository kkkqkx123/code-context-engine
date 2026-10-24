use rusqlite::Connection;

use cce_types::StorageError;

/// The schema version represented by the table definitions in `schema/tables`.
pub(crate) const LATEST_SCHEMA_VERSION: i64 = 7;

/// Reject databases written by an incompatible schema state.
pub fn run(conn: &Connection) -> Result<(), StorageError> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| StorageError::Table(format!("Failed to read schema version: {error}")))?;
    if version == LATEST_SCHEMA_VERSION {
        return Ok(());
    }
    if version == 5 {
        conn.execute(
            "ALTER TABLE projects ADD COLUMN profile_json TEXT NOT NULL DEFAULT '{}'",
            [],
        )
        .map_err(|error| StorageError::Table(format!("Failed to migrate profile_json: {error}")))?;
        conn.execute(
            "ALTER TABLE projects ADD COLUMN profile_version INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|error| {
            StorageError::Table(format!("Failed to migrate profile_version: {error}"))
        })?;
    } else if version != 6 {
        let message = format!(
            "Incompatible database schema version {version} (expected {LATEST_SCHEMA_VERSION}); \
             run a full rebuild with force_reindex=true or delete the data directory"
        );
        tracing::error!(%message, "Refusing to open outdated index database");
        return Err(StorageError::Table(message));
    }
    migrate_drop_legacy_scan_columns(conn)?;
    conn.pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
        .map_err(|error| StorageError::Table(format!("Failed to set schema version: {error}")))?;
    Ok(())
}

/// Drop the scan-cache columns superseded by the profile document.
///
/// Each statement tolerates a missing column so the migration stays
/// idempotent and rerunnable against partially migrated databases.
fn migrate_drop_legacy_scan_columns(conn: &Connection) -> Result<(), StorageError> {
    for column in [
        "extensions",
        "exclude_dirs",
        "respect_gitignore",
        "ignore_patterns",
    ] {
        let _ = conn.execute(&format!("ALTER TABLE projects DROP COLUMN {column}"), []);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_column_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("PRAGMA table_info(projects)")
            .expect("table info must be readable");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("column scan must succeed")
            .map(|name| name.expect("column name must decode"))
            .collect()
    }

    #[test]
    fn drops_legacy_scan_columns_and_stays_rerunnable() {
        let conn = Connection::open_in_memory().expect("memory database must open");
        conn.execute_batch(
            "CREATE TABLE projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                root_path TEXT NOT NULL UNIQUE,
                config_file_path TEXT NOT NULL DEFAULT '.cce/config.json',
                language TEXT,
                extensions TEXT,
                exclude_dirs TEXT,
                respect_gitignore INTEGER,
                ignore_patterns TEXT,
                profile_json TEXT NOT NULL DEFAULT '{}',
                profile_version INTEGER NOT NULL DEFAULT 0,
                last_indexed TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            PRAGMA user_version = 6;",
        )
        .expect("legacy schema must apply");
        conn.execute(
            "INSERT INTO projects (name, root_path, created_at, updated_at)
             VALUES ('legacy', '/tmp/legacy', 1, 1)",
            [],
        )
        .expect("legacy row must insert");

        run(&conn).expect("migration must succeed");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version must be readable");
        assert_eq!(version, LATEST_SCHEMA_VERSION);
        let columns = legacy_column_names(&conn);
        for dropped in [
            "extensions",
            "exclude_dirs",
            "respect_gitignore",
            "ignore_patterns",
        ] {
            assert!(
                !columns.iter().any(|name| name == dropped),
                "legacy column {dropped} must be gone"
            );
        }
        for kept in [
            "id",
            "name",
            "root_path",
            "config_file_path",
            "language",
            "profile_json",
            "profile_version",
        ] {
            assert!(
                columns.iter().any(|name| name == kept),
                "live column {kept} must survive"
            );
        }

        run(&conn).expect("migration must be rerunnable");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .expect("rows must survive migration");
        assert_eq!(count, 1);
    }
}
