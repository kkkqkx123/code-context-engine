//! Project repository for SQLite operations

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::helpers::{execute_count, execute_query, execute_query_optional, execute_update};
use crate::types::{NewProjectRecord, ProjectRecord, ProjectUpdateRecord};
use crate::utils::current_timestamp;
use cce_types::StorageError;

/// Project repository for CRUD operations
pub struct ProjectRepository;
impl ProjectRepository {
    /// Insert a new project and return the ID
    pub fn insert(
        tx: &rusqlite::Transaction,
        project: &NewProjectRecord,
    ) -> Result<i64, StorageError> {
        let now = current_timestamp();
        let config_file_path = project
            .config_file_path
            .as_deref()
            .unwrap_or(".cce/config.json");

        tx.execute(
            "INSERT INTO projects (name, root_path, config_file_path, language, extensions, exclude_dirs, respect_gitignore, ignore_patterns, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                project.name,
                project.root_path,
                config_file_path,
                project.language,
                project.extensions,
                project.exclude_dirs,
                project.respect_gitignore.map(|b| if b { 1 } else { 0 }),
                project.ignore_patterns,
                now,
                now,
            ],
        )
        .map_err(|e| StorageError::insert(format!("Failed to insert project: {}", e)))?;

        Ok(tx.last_insert_rowid())
    }

    /// Get a project by ID
    pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<ProjectRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, name, root_path, config_file_path, language, extensions, exclude_dirs, respect_gitignore, ignore_patterns, last_indexed, created_at, updated_at
             FROM projects WHERE id = ?1",
            params![id],
            Self::from_row,
        )
    }

    /// Get all projects
    pub fn get_all(conn: &Connection) -> Result<Vec<ProjectRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, root_path, config_file_path, language, extensions, exclude_dirs, respect_gitignore, ignore_patterns, last_indexed, created_at, updated_at
             FROM projects ORDER BY created_at DESC",
            params![],
            Self::from_row,
        )
    }

    /// Get projects by name
    pub fn get_by_name(conn: &Connection, name: &str) -> Result<Vec<ProjectRecord>, StorageError> {
        execute_query(
            conn,
            "SELECT id, name, root_path, config_file_path, language, extensions, exclude_dirs, respect_gitignore, ignore_patterns, last_indexed, created_at, updated_at
             FROM projects WHERE name = ?1",
            params![name],
            Self::from_row,
        )
    }

    /// Delete a project by ID with cascade cleanup
    pub fn delete(tx: &rusqlite::Transaction, id: i64) -> Result<(), StorageError> {
        execute_update(
            tx,
            "DELETE FROM projects WHERE id = ?1",
            params![id],
            "delete project",
        )
    }

    /// Delete a project and all its related data across tables.
    ///
    /// This explicitly cleans up tables that lack ON DELETE CASCADE
    /// foreign key constraints, in addition to the CASCADE-enabled tables.
    pub fn delete_with_cascade(tx: &rusqlite::Transaction, id: i64) -> Result<(), StorageError> {
        // These tables have project_id but no ON DELETE CASCADE to projects
        for table in &["checkpoint_file", "checkpoint_batch", "checkpoint"] {
            execute_update(
                tx,
                &format!("DELETE FROM {} WHERE project_id = ?1", table),
                params![id],
                &format!("delete {} by project", table),
            )?;
        }

        Self::delete(tx, id)
    }

    /// Find a project by root path
    pub fn find_by_root_path(
        conn: &Connection,
        root_path: &str,
    ) -> Result<Option<ProjectRecord>, StorageError> {
        execute_query_optional(
            conn,
            "SELECT id, name, root_path, config_file_path, language, extensions, exclude_dirs, respect_gitignore, ignore_patterns, last_indexed, created_at, updated_at
             FROM projects WHERE root_path = ?1",
            params![root_path],
            Self::from_row,
        )
    }

    /// Count all projects
    pub fn count(conn: &Connection) -> Result<i64, StorageError> {
        execute_count(conn, "SELECT COUNT(*) FROM projects", params![], "projects")
    }

    /// Check if a project with the given root path exists
    pub fn path_exists(conn: &Connection, root_path: &str) -> Result<bool, StorageError> {
        let count = execute_count(
            conn,
            "SELECT COUNT(*) FROM projects WHERE root_path = ?1",
            params![root_path],
            "projects",
        )?;
        Ok(count > 0)
    }

    /// Update a project
    pub fn update(
        tx: &rusqlite::Transaction,
        id: i64,
        updates: &ProjectUpdateRecord,
    ) -> Result<(), StorageError> {
        // Build dynamic update query
        let mut sets = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref name) = updates.name {
            sets.push("name = ?");
            params_vec.push(Box::new(name.clone()));
        }
        if let Some(ref root_path) = updates.root_path {
            sets.push("root_path = ?");
            params_vec.push(Box::new(root_path.clone()));
        }
        if let Some(ref config_file_path) = updates.config_file_path {
            sets.push("config_file_path = ?");
            params_vec.push(Box::new(config_file_path.clone()));
        }
        if let Some(ref language) = updates.language {
            sets.push("language = ?");
            params_vec.push(Box::new(language.clone()));
        }
        if let Some(ref extensions) = updates.extensions {
            sets.push("extensions = ?");
            params_vec.push(Box::new(extensions.clone()));
        }
        if let Some(ref exclude_dirs) = updates.exclude_dirs {
            sets.push("exclude_dirs = ?");
            params_vec.push(Box::new(exclude_dirs.clone()));
        }
        if let Some(ref respect_gitignore) = updates.respect_gitignore {
            sets.push("respect_gitignore = ?");
            params_vec.push(Box::new(if *respect_gitignore { 1i32 } else { 0i32 }));
        }
        if let Some(ref ignore_patterns) = updates.ignore_patterns {
            sets.push("ignore_patterns = ?");
            params_vec.push(Box::new(ignore_patterns.clone()));
        }
        if let Some(ref last_indexed) = updates.last_indexed {
            sets.push("last_indexed = ?");
            params_vec.push(Box::new(last_indexed.clone()));
        }

        if sets.is_empty() {
            return Ok(()); // Nothing to update
        }

        sets.push("updated_at = ?");
        let now = current_timestamp();
        params_vec.push(Box::new(now));

        let sql = format!("UPDATE projects SET {} WHERE id = ?", sets.join(", "));

        params_vec.push(Box::new(id));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        tx.execute(&sql, params_refs.as_slice())
            .map_err(|e| StorageError::query(format!("Failed to update project: {}", e)))?;

        Ok(())
    }

    /// Parse a row into ProjectRecord
    fn from_row(row: &Row) -> Result<ProjectRecord, rusqlite::Error> {
        Ok(ProjectRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            root_path: row.get(2)?,
            config_file_path: row.get(3)?,
            language: row.get(4)?,
            extensions: row.get(5)?,
            exclude_dirs: row.get(6)?,
            respect_gitignore: row.get::<_, Option<i32>>(7)?.map(|v| v != 0),
            ignore_patterns: row.get(8)?,
            last_indexed: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }

    /// Get a project metadata value as integer
    pub fn meta_get_int(
        conn: &Connection,
        project_id: i64,
        key: &str,
    ) -> Result<i64, StorageError> {
        let value: String = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = ?2",
                params![project_id, key],
                |row| row.get(0),
            )
            .map_err(|e| {
                StorageError::Query(format!("Failed to get project_meta {}: {}", key, e))
            })?;

        value
            .parse::<i64>()
            .map_err(|e| StorageError::Query(format!("Failed to parse project_meta as int: {}", e)))
    }

    /// Read a project metadata value as an integer, distinguishing an absent
    /// row (the key was never written) from real failures.
    ///
    /// Returns `Ok(None)` when the row does not exist, `Ok(Some(value))` when
    /// the stored value parses, and `Err` for unparseable stored values or
    /// database failures.
    pub fn meta_get_int_optional(
        conn: &Connection,
        project_id: i64,
        key: &str,
    ) -> Result<Option<i64>, StorageError> {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = ?2",
                params![project_id, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| {
                StorageError::Query(format!("Failed to get project_meta {}: {}", key, e))
            })?;
        value
            .map(|value| {
                value.parse::<i64>().map_err(|e| {
                    StorageError::Query(format!(
                        "Failed to parse project_meta {} as int: {}",
                        key, e
                    ))
                })
            })
            .transpose()
    }

    /// Set a project metadata value as integer
    pub fn meta_set_int(
        conn: &Connection,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        let now = current_timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, ?2, ?3,
                     COALESCE((SELECT created_at FROM project_meta WHERE project_id = ?1 AND key = ?2), ?4),
                     ?4)",
            params![project_id, key, value.to_string(), now],
        )
        .map_err(|e| StorageError::Query(format!("Failed to set project_meta: {}", e)))?;
        Ok(())
    }

    /// Read a project metadata string value, distinguishing an absent row
    /// (the key was never written) from real failures.
    pub fn meta_get_string_optional(
        conn: &Connection,
        project_id: i64,
        key: &str,
    ) -> Result<Option<String>, StorageError> {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM project_meta WHERE project_id = ?1 AND key = ?2",
                params![project_id, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| {
                StorageError::Query(format!("Failed to get project_meta {}: {}", key, e))
            })?;
        Ok(value)
    }

    /// Set a project metadata string value.
    pub fn meta_set_string(
        conn: &Connection,
        project_id: i64,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        let now = current_timestamp();
        conn.execute(
            "INSERT OR REPLACE INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (?1, ?2, ?3,
                     COALESCE((SELECT created_at FROM project_meta WHERE project_id = ?1 AND key = ?2), ?4),
                     ?4)",
            params![project_id, key, value, now],
        )
        .map_err(|e| StorageError::Query(format!("Failed to set project_meta: {}", e)))?;
        Ok(())
    }
}

/// Generate a project name from root path
///
/// # Conversion Rules
/// - Windows: D:\projects\my-app → D-projects-my-app
/// - Linux: /home/user/projects → home-user-projects
pub fn generate_project_name(root_path: &str) -> String {
    // Convert path separators to '-', preserve original hyphens
    let converted: String = root_path
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' => '-',
            _ => c,
        })
        .collect();

    // Remove consecutive '-' characters
    let mut result = String::new();
    let mut prev_dash = false;
    for c in converted.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
                prev_dash = true;
            }
        } else {
            result.push(c);
            prev_dash = false;
        }
    }

    // Clean up and add prefix
    let cleaned = result
        .trim_start_matches('-')
        .trim_end_matches('-')
        .to_string();

    format!("proj-{}", cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_project_name_windows() {
        let name = generate_project_name("D:\\projects\\my-app");
        assert_eq!(name, "proj-D-projects-my-app");
    }

    #[test]
    fn test_generate_project_name_linux() {
        let name = generate_project_name("/home/user/projects");
        assert_eq!(name, "proj-home-user-projects");
    }

    #[test]
    fn meta_get_int_optional_distinguishes_absent_rows_from_failures() {
        let client = crate::SqliteClient::in_memory().expect("in-memory database should open");
        let conn = client.write_connection().expect("connection should open");
        let now = current_timestamp();
        conn.execute(
            "INSERT INTO projects (id, name, root_path, config_file_path, created_at, updated_at)
             VALUES (1, 'test', '/tmp/test', '.cce/config.json', ?1, ?1)",
            [now],
        )
        .expect("project should be inserted");

        // Missing row → Ok(None)
        let value = ProjectRepository::meta_get_int_optional(&conn, 1, "active_epoch")
            .expect("missing row must not be a failure");
        assert_eq!(value, None);

        // Parseable value → Ok(Some(value))
        conn.execute(
            "INSERT INTO project_meta (project_id, key, value, created_at, updated_at)
             VALUES (1, 'active_epoch', '5', ?1, ?1)",
            [now],
        )
        .expect("meta row should be inserted");
        let value = ProjectRepository::meta_get_int_optional(&conn, 1, "active_epoch")
            .expect("parseable value should be returned");
        assert_eq!(value, Some(5));

        // Unparseable stored value → Err (not silently downgraded)
        conn.execute(
            "UPDATE project_meta SET value = 'corrupt' WHERE project_id = 1 AND key = 'active_epoch'",
            [],
        )
        .expect("meta value should be corrupted");
        let err = ProjectRepository::meta_get_int_optional(&conn, 1, "active_epoch")
            .expect_err("corrupt value must be a failure");
        assert!(matches!(err, StorageError::Query(_)));
    }
}
