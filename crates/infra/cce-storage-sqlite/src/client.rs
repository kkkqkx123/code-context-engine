//! SQLite client for metadata storage.

use parking_lot::Mutex;
use rusqlite::{Connection, Transaction};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::metrics::SqliteMetrics;
use crate::repo::ProjectRepository;
use crate::schema;
use cce_config::validation::Validate;
use cce_types::StorageError;

pub use crate::config::SqliteConfig;

#[derive(Debug, Clone)]
pub struct ClientStats {
    pub total_size_bytes: usize,
    pub item_count: usize,
}

#[derive(Clone)]
pub struct SqliteClient {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Option<Arc<Mutex<Connection>>>,
    config: SqliteConfig,
    metrics: Option<Arc<SqliteMetrics>>,
    project_clients: Arc<Mutex<HashMap<i64, Arc<SqliteClient>>>>,
    scoped_project_id: Option<i64>,
}

impl SqliteClient {
    pub fn new(config: SqliteConfig) -> Result<Self, StorageError> {
        debug!(path = %config.path, "Creating SQLite client");

        config.validate_structured().map_err(|error| {
            StorageError::Connection(format!("Invalid SQLite configuration: {error}"))
        })?;

        let write_conn = Self::open_connection(&config, false)?;
        let read_conn = if config.path == ":memory:" {
            None
        } else {
            Some(Self::open_connection(&config, true)?)
        };
        let client = Self {
            write_conn: Arc::new(Mutex::new(write_conn)),
            read_conn: read_conn.map(|conn| Arc::new(Mutex::new(conn))),
            config,
            metrics: None,
            project_clients: Arc::new(Mutex::new(HashMap::new())),
            scoped_project_id: None,
        };

        info!(path = %client.config.path, "SQLite client initialized");
        Ok(client)
    }

    pub fn for_project(&self, project_id: i64) -> Result<Arc<SqliteClient>, StorageError> {
        if self.config.path == ":memory:" || self.scoped_project_id == Some(project_id) {
            return Ok(Arc::new(self.clone()));
        }
        if let Some(client) = self.project_clients.lock().get(&project_id) {
            return Ok(client.clone());
        }

        let path = project_db_path(&self.config.path, project_id)?;
        let mut config = self.config.clone();
        config.path = path.to_string_lossy().to_string();
        let mut client = Self::new(config)?;
        client.scoped_project_id = Some(project_id);
        let client = Arc::new(client);

        let registry_row = self.with_transaction(|tx| ProjectRepository::get_by_id(tx, project_id));
        let registry_row = match registry_row {
            Ok(Some(record)) => Some(record),
            Ok(None) => None,
            Err(e) => {
                warn!(error = %e, project_id, "Failed to read registry project row");
                None
            }
        };
        client.with_transaction(|tx| {
            use rusqlite::params;
            let now = crate::utils::current_timestamp();
            let (
                name,
                root_path,
                config_file_path,
                language,
                extensions,
                exclude_dirs,
                respect_gitignore,
                ignore_patterns,
            ) = match registry_row {
                Some(ref record) => (
                    record.name.clone(),
                    record.root_path.clone(),
                    record.config_file_path.clone(),
                    record.language.clone(),
                    record.extensions.clone(),
                    record.exclude_dirs.clone(),
                    record.respect_gitignore.map(|b| if b { 1 } else { 0 }),
                    record.ignore_patterns.clone(),
                ),
                None => (
                    format!("project-{project_id}"),
                    format!("/projects/{project_id}"),
                    ".cce/config.json".to_string(),
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            };
            tx.execute(
                "INSERT OR IGNORE INTO projects
                    (id, name, root_path, config_file_path, language, extensions,
                     exclude_dirs, respect_gitignore, ignore_patterns, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    project_id,
                    name,
                    root_path,
                    config_file_path,
                    language,
                    extensions,
                    exclude_dirs,
                    respect_gitignore,
                    ignore_patterns,
                    now,
                ],
            )
            .map_err(|e| StorageError::insert(format!("Failed to seed project row: {e}")))?;
            Ok(())
        })?;

        let mut clients = self.project_clients.lock();
        if let Some(existing) = clients.get(&project_id) {
            return Ok(existing.clone());
        }
        clients.insert(project_id, client.clone());
        Ok(client)
    }

    pub fn with_metrics(mut self, metrics: Arc<SqliteMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn metrics(&self) -> Option<&Arc<SqliteMetrics>> {
        self.metrics.as_ref()
    }

    pub fn in_memory() -> Result<Self, StorageError> {
        let config = SqliteConfig::new(":memory:".to_string());
        Self::new(config)
    }

    pub fn with_path(path: impl Into<String>) -> Result<Self, StorageError> {
        let config = SqliteConfig::new(path);
        Self::new(config)
    }

    pub fn new_in_temp() -> Result<Self, StorageError> {
        use uuid::Uuid;
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join(format!("cce_test_{}.db", Uuid::new_v4()));
        let config = SqliteConfig::new(temp_path.to_string_lossy().to_string());
        Self::new(config)
    }

    fn open_connection(config: &SqliteConfig, read_only: bool) -> Result<Connection, StorageError> {
        let path = Path::new(&config.path);

        let conn = Connection::open(path).map_err(|e| {
            error!(path = %config.path, error = %e, "Failed to open database");
            StorageError::Connection(format!("Failed to open database: {}", e))
        })?;

        if !read_only {
            if config.enable_wal {
                conn.execute_batch("PRAGMA journal_mode = WAL;")
                    .map_err(|e| {
                        error!(path = %config.path, error = %e, "Failed to enable WAL mode");
                        StorageError::Connection(format!("Failed to enable WAL: {}", e))
                    })?;
                debug!(path = %config.path, "WAL mode enabled");
            }

            let synchronous_pragma = format!("PRAGMA synchronous = {};", config.synchronous);
            conn.execute_batch(&synchronous_pragma).map_err(|e| {
                error!(path = %config.path, error = %e, "Failed to set synchronous mode");
                StorageError::Connection(format!("Failed to set synchronous mode: {}", e))
            })?;
        }

        let cache_pragma = format!("PRAGMA cache_size = {};", config.cache_size);
        conn.execute_batch(&cache_pragma).map_err(|e| {
            error!(path = %config.path, error = %e, "Failed to set cache size");
            StorageError::Connection(format!("Failed to set cache size: {}", e))
        })?;

        let busy_timeout_pragma = format!("PRAGMA busy_timeout = {};", config.busy_timeout_ms);
        conn.execute_batch(&busy_timeout_pragma).map_err(|e| {
            error!(path = %config.path, error = %e, "Failed to set busy timeout");
            StorageError::Connection(format!("Failed to set busy timeout: {}", e))
        })?;

        conn.execute_batch("PRAGMA temp_store = MEMORY;")
            .map_err(|e| {
                error!(path = %config.path, error = %e, "Failed to set temp store");
                StorageError::Connection(format!("Failed to set temp store: {}", e))
            })?;

        let mmap_pragma = format!("PRAGMA mmap_size = {};", config.mmap_size);
        conn.execute_batch(&mmap_pragma).map_err(|e| {
            error!(path = %config.path, error = %e, "Failed to set mmap size");
            StorageError::Connection(format!("Failed to set mmap size: {}", e))
        })?;

        if read_only {
            conn.execute_batch("PRAGMA query_only = ON;").map_err(|e| {
                error!(path = %config.path, error = %e, "Failed to enable query_only");
                StorageError::Connection(format!("Failed to enable query_only: {}", e))
            })?;
        } else {
            if config.enable_fk {
                conn.execute_batch("PRAGMA foreign_keys = ON;")
                    .map_err(|e| {
                        error!(path = %config.path, error = %e, "Failed to enable foreign keys");
                        StorageError::Connection(format!("Failed to enable foreign keys: {}", e))
                    })?;
            }

            schema::create_all(&conn)?;
            debug!(path = %config.path, "Schema initialized");
        }

        Ok(conn)
    }

    pub fn with_transaction<F, R>(&self, f: F) -> Result<R, StorageError>
    where
        F: FnOnce(&Transaction) -> Result<R, StorageError>,
    {
        let start = std::time::Instant::now();

        let mut conn = self.write_conn.lock();

        let tx = conn.transaction().map_err(|e| {
            error!(path = %self.config.path, error = %e, "Failed to start transaction");
            StorageError::Transaction(format!("Failed to start transaction: {}", e))
        })?;

        let result = match f(&tx) {
            Ok(result) => {
                tx.commit().map_err(|e| {
                    error!(path = %self.config.path, error = %e, "Failed to commit transaction");
                    StorageError::Transaction(format!("Failed to commit transaction: {}", e))
                })?;
                Ok(result)
            }
            Err(e) => {
                warn!(path = %self.config.path, error = %e, "Transaction failed, rolling back");
                tx.rollback().map_err(|err| {
                    error!(path = %self.config.path, error = %err, "Failed to rollback transaction");
                    StorageError::Transaction(format!("Failed to rollback transaction: {}", err))
                })?;
                Err(e)
            }
        };

        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_transaction(elapsed, true, result.is_ok());
        }

        result
    }

    pub fn config(&self) -> &SqliteConfig {
        &self.config
    }

    pub fn read_connection(&self) -> Result<parking_lot::MutexGuard<'_, Connection>, StorageError> {
        match &self.read_conn {
            Some(conn) => Ok(conn.lock()),
            None => Ok(self.write_conn.lock()),
        }
    }

    pub fn write_connection(
        &self,
    ) -> Result<parking_lot::MutexGuard<'_, Connection>, StorageError> {
        Ok(self.write_conn.lock())
    }

    pub fn db_size(&self) -> Result<u64, StorageError> {
        let path = Path::new(&self.config.path);

        let main_size = file_size_or_zero(path)?;
        let wal_size = file_size_or_zero(&sqlite_sidecar_path(path, "-wal"))?;
        let shm_size = file_size_or_zero(&sqlite_sidecar_path(path, "-shm"))?;

        let mut total = main_size
            .checked_add(wal_size)
            .and_then(|size| size.checked_add(shm_size))
            .ok_or_else(|| StorageError::Query("SQLite database size overflow".to_string()))?;

        for client in self.project_clients.lock().values() {
            total = total
                .checked_add(client.db_size()?)
                .ok_or_else(|| StorageError::Query("SQLite database size overflow".to_string()))?;
        }
        Ok(total)
    }

    pub fn project_meta_get_int(&self, project_id: i64, key: &str) -> Result<i64, StorageError> {
        let project = self.for_project(project_id)?;
        let conn = project.read_connection()?;
        ProjectRepository::meta_get_int(&conn, project_id, key)
    }

    pub fn project_meta_get_int_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<i64>, StorageError> {
        let project = self.for_project(project_id)?;
        let conn = project.read_connection()?;
        ProjectRepository::meta_get_int_optional(&conn, project_id, key)
    }

    pub fn project_meta_set_int(
        &self,
        project_id: i64,
        key: &str,
        value: i64,
    ) -> Result<(), StorageError> {
        let project = self.for_project(project_id)?;
        let conn = project.write_connection()?;
        ProjectRepository::meta_set_int(&conn, project_id, key, value)
    }

    pub fn project_meta_get_string_optional(
        &self,
        project_id: i64,
        key: &str,
    ) -> Result<Option<String>, StorageError> {
        let project = self.for_project(project_id)?;
        let conn = project.read_connection()?;
        ProjectRepository::meta_get_string_optional(&conn, project_id, key)
    }

    pub fn project_meta_set_string(
        &self,
        project_id: i64,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        let project = self.for_project(project_id)?;
        let conn = project.write_connection()?;
        ProjectRepository::meta_set_string(&conn, project_id, key, value)
    }

    pub fn get_stats(&self) -> Result<ClientStats, StorageError> {
        let total_size_bytes = usize::try_from(self.db_size()?)
            .map_err(|_| StorageError::Query("SQLite database is too large".to_string()))?;
        let conn = self.read_connection()?;
        let item_count: i64 = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM projects) +
                    (SELECT COUNT(*) FROM files) +
                    (SELECT COUNT(*) FROM entities) +
                    (SELECT COUNT(*) FROM chunks)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| {
                StorageError::Query(format!("Failed to count SQLite records: {error}"))
            })?;

        let mut total_items = usize::try_from(item_count)
            .map_err(|_| StorageError::Query("SQLite item count overflow".to_string()))?;
        for client in self.project_clients.lock().values() {
            let stats = client.get_stats()?;
            total_items = total_items
                .checked_add(stats.item_count)
                .ok_or_else(|| StorageError::Query("SQLite item count overflow".to_string()))?;
        }

        Ok(ClientStats {
            total_size_bytes,
            item_count: total_items,
        })
    }

    pub fn delete_project_db(&self, project_id: i64) -> Result<usize, StorageError> {
        self.project_clients.lock().remove(&project_id);

        let path = project_db_path(&self.config.path, project_id)?;
        let mut removed = 0;
        for candidate in [
            path.clone(),
            sqlite_sidecar_path(&path, "-wal"),
            sqlite_sidecar_path(&path, "-shm"),
        ] {
            match std::fs::remove_file(&candidate) {
                Ok(()) => removed += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(StorageError::Connection(format!(
                        "Failed to remove project database {}: {e}",
                        candidate.display()
                    )));
                }
            }
        }
        Ok(removed)
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    sidecar.into()
}

fn project_db_path(main_path: &str, project_id: i64) -> Result<PathBuf, StorageError> {
    if project_id <= 0 {
        return Err(StorageError::query(format!(
            "Invalid project id for per-project database: {project_id}"
        )));
    }
    let main = Path::new(main_path);
    let dir = main.with_extension("projects");
    std::fs::create_dir_all(&dir).map_err(|e| {
        StorageError::Connection(format!(
            "Failed to create per-project database directory {}: {e}",
            dir.display()
        ))
    })?;
    Ok(dir.join(format!("project_{project_id}.db")))
}

fn file_size_or_zero(path: &Path) -> Result<u64, StorageError> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(StorageError::from(error)),
    }
}

impl std::fmt::Debug for SqliteClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteClient")
            .field("config", &self.config)
            .field("has_connection", &true)
            .finish()
    }
}

impl cce_storage_common::SqliteStore for SqliteClient {
    type Error = StorageError;

    fn execute_write(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<usize, StorageError> {
        let conn = self.write_connection()?;
        conn.execute(sql, params)
            .map_err(|e| StorageError::Sqlite(e.to_string()))
    }

    fn query_rows(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: &mut dyn FnMut(
            &rusqlite::Row<'_>,
        ) -> rusqlite::Result<cce_storage_common::AggregatedMetric>,
    ) -> Result<Vec<cce_storage_common::AggregatedMetric>, StorageError> {
        let conn = self.read_connection()?;
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        let rows = stmt
            .query_map(params, f)
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::Sqlite(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;
    use tempfile::NamedTempFile;

    #[test]
    fn test_sqlite_client_new() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let config = SqliteConfig::new(path);
        let client = SqliteClient::new(config).expect("Failed to create client");
        assert_eq!(
            client.config.path,
            temp.path().to_string_lossy().to_string()
        );
        let conn = client.write_connection().expect("Failed to get connection");
        let schema_version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("Failed to read schema version");
        assert_eq!(schema_version, migration::LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn test_sqlite_client_transaction() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let config = SqliteConfig::new(path);
        let client = SqliteClient::new(config).expect("Failed to create client");

        let result = client.with_transaction(|tx| {
            tx.execute(
                "CREATE TABLE IF NOT EXISTS test_table (id INTEGER PRIMARY KEY)",
                [],
            )
            .map_err(|e| StorageError::Query(e.to_string()))?;
            Ok(42)
        });

        assert_eq!(result.expect("Transaction failed"), 42);
    }

    #[test]
    fn test_sqlite_client_get_stats() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let config = SqliteConfig::new(path);
        let client = SqliteClient::new(config).expect("Failed to create client");

        let stats = client.get_stats().expect("Failed to get stats");
        assert!(stats.total_size_bytes > 0);
        assert_eq!(stats.item_count, 0);
    }

    #[test]
    fn test_sqlite_client_counts_domain_records() {
        let client = SqliteClient::in_memory().expect("Failed to create client");
        client
            .with_transaction(|tx| {
                tx.execute(
                    "INSERT INTO projects (name, root_path, created_at, updated_at)
                     VALUES ('test', '/tmp/test', 1, 1)",
                    [],
                )
                .map_err(|error| StorageError::Insert(error.to_string()))?;
                Ok(())
            })
            .expect("Failed to insert test project");

        let stats = client.get_stats().expect("Failed to get stats");
        assert_eq!(stats.item_count, 1);
    }

    #[test]
    fn for_project_keeps_registry_and_isolates_project_metadata() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let client = SqliteClient::with_path(path).expect("Failed to create client");
        let p1 = client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &crate::types::NewProjectRecord::new(
                        "alpha".to_string(),
                        "/tmp/alpha".to_string(),
                    ),
                )
            })
            .expect("insert project");

        client
            .project_meta_set_int(p1, "active_epoch", 7)
            .expect("write project meta");

        let project = client.for_project(p1).expect("open project db");
        assert_eq!(
            project
                .project_meta_get_int(p1, "active_epoch")
                .expect("read project meta"),
            7
        );

        let registry_count: i64 = client
            .read_connection()
            .expect("registry connection")
            .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
            .expect("count registry rows");
        assert_eq!(registry_count, 1);

        let stats = client.get_stats().expect("aggregated stats");
        assert!(stats.total_size_bytes > 0);
    }

    #[test]
    fn test_sqlite_sidecar_path_preserves_database_extension() {
        let path = Path::new("metadata.sqlite");
        assert_eq!(
            sqlite_sidecar_path(path, "-wal"),
            Path::new("metadata.sqlite-wal")
        );
    }

    #[test]
    fn test_sqlite_client_rejects_invalid_configuration() {
        let config = SqliteConfig::new(":memory:").cache_size(-1_048_577);
        let result = SqliteClient::new(config);
        assert!(matches!(result, Err(StorageError::Connection(_))));
    }

    #[test]
    fn for_project_creates_isolated_databases() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let client = SqliteClient::with_path(path).expect("Failed to create client");

        let p1 = client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &crate::types::NewProjectRecord::new(
                        "alpha".to_string(),
                        "/tmp/alpha".to_string(),
                    ),
                )
            })
            .expect("insert project 1");
        let p2 = client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &crate::types::NewProjectRecord::new(
                        "beta".to_string(),
                        "/tmp/beta".to_string(),
                    ),
                )
            })
            .expect("insert project 2");

        let db1 = client.for_project(p1).expect("open project 1 db");
        let db2 = client.for_project(p2).expect("open project 2 db");
        assert_ne!(db1.config().path, db2.config().path);
        assert_ne!(db1.config().path, client.config().path);

        db1.with_transaction(|tx| {
            use crate::repo::ChunkRepository;
            use crate::types::ChunkRecord;
            let chunk = ChunkRecord::new(
                "c1".to_string(),
                "src/a.rs".to_string(),
                "nl".to_string(),
                1,
                2,
            )
            .with_project_id(p1);
            ChunkRepository::insert(tx, &chunk)
        })
        .expect("write chunk to project 1");

        let conn1 = db1.read_connection().expect("read project 1");
        let conn2 = db2.read_connection().expect("read project 2");
        let count1: i64 = conn1
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .expect("count chunks project 1");
        let count2: i64 = conn2
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .expect("count chunks project 2");
        assert_eq!(count1, 1);
        assert_eq!(count2, 0, "project databases must be isolated");

        let db1_again = client.for_project(p1).expect("re-open project 1 db");
        assert!(Arc::ptr_eq(&db1, &db1_again));

        let memory = SqliteClient::in_memory().expect("in-memory client");
        let memory_project = memory.for_project(p1).expect("memory project");
        assert_eq!(
            memory_project.config().path,
            memory.config().path,
            "in-memory clients must not open separate project databases"
        );
    }

    #[test]
    fn delete_project_db_removes_project_file() {
        let temp = NamedTempFile::new().expect("Failed to create temp file");
        let path = temp.path().to_string_lossy().to_string();
        let client = SqliteClient::with_path(path).expect("Failed to create client");
        let p1 = client
            .with_transaction(|tx| {
                ProjectRepository::insert(
                    tx,
                    &crate::types::NewProjectRecord::new(
                        "alpha".to_string(),
                        "/tmp/alpha".to_string(),
                    ),
                )
            })
            .expect("insert project");

        let project = client.for_project(p1).expect("open project db");
        let project_path = project.config().path.clone();
        assert!(std::path::Path::new(&project_path).exists());

        let removed = client
            .delete_project_db(p1)
            .expect("delete project database");
        assert!(removed >= 1);
        assert!(
            !std::path::Path::new(&project_path).exists(),
            "project database file must be removed"
        );
    }
}
