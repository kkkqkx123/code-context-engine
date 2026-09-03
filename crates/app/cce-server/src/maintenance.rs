//! Project index maintenance service
//!
//! Coordinates Qdrant, BM25, SQLite, relation runtime, and cache cleanup
//! for project-level clear and delete operations. All methods are idempotent
//! and report per-backend results so partial failures are observable.

use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;

use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;

use crate::engine::CodeContextEngine;

/// Result of a single backend operation during maintenance
#[derive(Debug, Clone)]
pub struct BackendResult {
    pub backend: &'static str,
    pub ok: bool,
    pub detail: String,
}

/// Combined result from a maintenance operation across all backends
#[derive(Debug, Clone)]
pub struct MaintenanceResult {
    pub project_id: i64,
    pub backends: Vec<BackendResult>,
    pub success: bool,
}

impl MaintenanceResult {
    fn new(project_id: i64) -> Self {
        Self {
            project_id,
            backends: Vec::new(),
            success: true,
        }
    }

    fn push(&mut self, backend: &'static str, ok: bool, detail: String) {
        if !ok {
            self.success = false;
        }
        self.backends.push(BackendResult {
            backend,
            ok,
            detail,
        });
    }
}

/// Project index maintenance service
///
/// Handles clear and delete operations across all storage backends.
/// Each method is idempotent and returns detailed per-backend results.
pub struct ProjectIndexMaintenanceService {
    engine: Arc<CodeContextEngine>,
    qdrant: Option<Arc<QdrantClient>>,
    bm25: Option<Arc<Mutex<Bm25Client>>>,
    metadata_store: Option<Arc<SqliteClient>>,
}

impl ProjectIndexMaintenanceService {
    pub fn new(
        engine: Arc<CodeContextEngine>,
        qdrant: Option<Arc<QdrantClient>>,
        bm25: Option<Arc<Mutex<Bm25Client>>>,
        metadata_store: Option<Arc<SqliteClient>>,
    ) -> Self {
        Self {
            engine,
            qdrant,
            bm25,
            metadata_store,
        }
    }

    fn sqlite_client(&self) -> Option<SqliteClient> {
        self.metadata_store.as_ref().map(|db| db.as_ref().clone())
    }

    /// Clear a project's index data from all backends, leaving the project
    /// registration intact. Idempotent — safe to call multiple times.
    pub async fn clear_project_index(&self, project_id: i64) -> MaintenanceResult {
        let mut result = MaintenanceResult::new(project_id);

        // Resolve group ID from project registry
        let entry = match self.engine.project_registry().get_or_load(project_id).await {
            Ok(e) => e,
            Err(e) => {
                result.push("registry", false, format!("Project not found: {}", e));
                return result;
            }
        };
        let group_id =
            cce_storage_qdrant::generate_project_group_id(project_id, &entry.metadata.root_path);

        info!(project_id, group = %group_id, "Clearing project index");

        // 1. Qdrant: delete_by_group only
        if let Some(client) = &self.qdrant {
            match client.delete_by_group(&group_id).await {
                Ok(()) => result.push("qdrant", true, "Group points deleted".to_string()),
                Err(e) => result.push("qdrant", false, format!("Failed to delete group: {}", e)),
            }
        } else {
            result.push("qdrant", true, "Qdrant not configured, skipped".to_string());
        }

        // 2. BM25: delete_all_project_docs only
        if let Some(bm25) = &self.bm25 {
            let mut client = bm25.lock().await;
            if client.is_enabled() {
                let index_name = client.config().index_name.clone();
                match client
                    .delete_all_project_docs(&index_name, project_id)
                    .await
                {
                    Ok(count) => {
                        result.push("bm25", true, format!("Deleted {} project documents", count))
                    }
                    Err(e) => result.push(
                        "bm25",
                        false,
                        format!("Failed to delete project docs: {}", e),
                    ),
                }
            } else {
                result.push("bm25", true, "BM25 not enabled, skipped".to_string());
            }
        } else {
            result.push("bm25", true, "BM25 not configured, skipped".to_string());
        }

        // 3. SQLite: clean index artifacts in a single transaction
        if let Some(sqlite) = self.sqlite_client() {
            let cleanup_result = sqlite
                .for_project(project_id)
                .map_err(|e| e.to_string())
                .and_then(|project| self.clear_sqlite_index(&project, project_id));
            match cleanup_result {
                Ok(()) => result.push("sqlite", true, "Index artifacts deleted".to_string()),
                Err(e) => result.push("sqlite", false, format!("Failed to clean SQLite: {}", e)),
            }
        } else {
            result.push("sqlite", true, "SQLite not configured, skipped".to_string());
        }

        // 4. Publish an empty canonical graph even when the in-memory builder
        // was never loaded. SQLite and runtime then agree on the clear result.
        if let Err(e) = self
            .engine
            .publish_empty_relation_snapshot(project_id)
            .await
        {
            result.push(
                "relations",
                false,
                format!("Failed to publish relation snapshot: {}", e),
            );
        } else {
            result.push(
                "relations",
                true,
                "Empty relation snapshot published".to_string(),
            );
        }

        // 5. Clear only the target project's cache
        if let Err(e) = self.engine.reload_project_config(project_id).await {
            result.push("cache", false, format!("Failed to clear cache: {}", e));
        } else {
            result.push("cache", true, "Project cache cleared".to_string());
        }

        result
    }

    fn clear_sqlite_index(&self, sqlite: &SqliteClient, project_id: i64) -> Result<(), String> {
        use cce_types::StorageError;

        sqlite
            .with_transaction(|tx| {
                use rusqlite::params;

                tx.execute(
                    "DELETE FROM files WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM entities WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM chunks WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM relations WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM entity_detail_mappings WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM project_meta WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                tx.execute(
                    "DELETE FROM checkpoint WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

                Ok(())
            })
            .map_err(|e: StorageError| e.to_string())
    }

    /// Delete a project's data from all backends AND remove the project record.
    /// If Qdrant or BM25 cleanup fails, the project record is preserved to allow retry.
    pub async fn delete_project(&self, project_id: i64) -> MaintenanceResult {
        let mut result = MaintenanceResult::new(project_id);

        // Step A: Clear index data (Qdrant, BM25, SQLite, relations, cache)
        let clear_result = self.clear_project_index(project_id).await;

        // Merge clear results
        for br in &clear_result.backends {
            if br.backend == "registry" {
                result.push(br.backend, br.ok, br.detail.clone());
                return result;
            }
        }

        // If Qdrant or BM25 failed, preserve project record for retry
        let qdrant_ok = clear_result
            .backends
            .iter()
            .any(|b| b.backend == "qdrant" && !b.ok);
        let bm25_ok = clear_result
            .backends
            .iter()
            .any(|b| b.backend == "bm25" && !b.ok);

        if qdrant_ok || bm25_ok {
            result.push(
                "project_record",
                false,
                "Qdrant or BM25 cleanup failed, project record preserved for retry".to_string(),
            );
            // Merge other backends results
            for br in clear_result.backends {
                result.push(br.backend, br.ok, br.detail);
            }
            return result;
        }

        // Step B: Delete SQLite project record and project database
        if let Some(sqlite) = self.sqlite_client() {
            let registry_result = sqlite.with_transaction(|tx| {
                cce_storage_sqlite::ProjectRepository::delete_with_cascade(tx, project_id)
            });
            match registry_result {
                Ok(()) => {
                    result.push("project_record", true, "Project record deleted".to_string());
                    // Remove the project's database file (its data is gone
                    // with the record; the file may still hold residual state)
                    match sqlite.delete_project_db(project_id) {
                        Ok(removed) => {
                            result.push(
                                "project_db",
                                true,
                                format!("Project database removed ({removed} files)"),
                            );
                        }
                        Err(e) => {
                            result.push(
                                "project_db",
                                false,
                                format!("Failed to remove project database: {e}"),
                            );
                        }
                    }
                }
                Err(e) => {
                    result.push(
                        "project_record",
                        false,
                        format!("Failed to delete project record: {}", e),
                    );
                }
            }
        } else {
            result.push(
                "project_record",
                false,
                "SQLite not configured, cannot delete project record".to_string(),
            );
        }

        result
    }
}
