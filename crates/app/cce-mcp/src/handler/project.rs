//! Project management and indexing tools for the MCP surface.
//!
//! Listing, metadata lookup, full indexing, and incremental re-indexing
//! delegate to the shared engine's metadata store, project registry, and hot
//! update coordinator.

use cce_orchestrator::IndexOptions;
use cce_server_shared::context::SharedContext;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

use super::support::is_valid_deleted_shape;

/// Arguments for full indexing.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexProjectArgs {
    /// Project ID to index (required).
    pub project_id: i64,
    /// Optional root directory override. When omitted the project's registered
    /// root path is used.
    #[serde(default)]
    pub root_dir: Option<String>,
    /// File extensions to include (e.g. ["rs", "py"]). Empty uses project config.
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    /// Directories to exclude. Empty uses project config.
    #[serde(default)]
    pub exclude_dirs: Option<Vec<String>>,
}

/// Arguments for incremental indexing of explicit file changes.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IncrementalIndexArgs {
    /// Project ID to update (required).
    pub project_id: i64,
    /// Changed files, each path relative to the project root.
    pub files: Vec<String>,
    /// Whether each file was deleted. Empty (all false), a single boolean
    /// applied to all files, or one boolean per file.
    #[serde(default)]
    pub deleted: Option<Vec<bool>>,
}

/// Arguments that only need a project ID.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectArgs {
    /// Project ID (required).
    pub project_id: i64,
}

/// Run the list projects tool call.
pub async fn list_projects(state: &Arc<SharedContext>) -> Result<CallToolResult, rmcp::ErrorData> {
    let Some(store) = state.engine.metadata_store() else {
        return Ok(CallToolResult::error(vec![ContentBlock::text(
            "metadata store is not configured; cannot list projects",
        )]));
    };
    match store
        .as_ref()
        .with_transaction(|tx| cce_storage_metadb_sqlite::ProjectRepository::get_all(tx))
    {
        Ok(records) => {
            let items: Vec<Value> = records
                .iter()
                .map(|record| {
                    json!({
                        "id": record.id,
                        "name": record.name,
                        "root_path": record.root_path,
                    })
                })
                .collect();
            Ok(CallToolResult::success(vec![ContentBlock::text(
                json!({ "projects": items }).to_string(),
            )]))
        }
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "list_projects failed: {error}"
        ))])),
    }
}

/// Run the get project tool call.
pub async fn get_project(
    state: &Arc<SharedContext>,
    args: ProjectArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match state
        .engine
        .project_registry()
        .get_or_load(args.project_id)
        .await
    {
        Ok(entry) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "id": entry.metadata.id,
                "name": entry.metadata.name,
                "root_path": entry.metadata.root_path,
                "version": entry.version,
            })
            .to_string(),
        )])),
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "get_project failed: {error}"
        ))])),
    }
}

/// Run the full index tool call.
pub async fn index_project(
    state: &Arc<SharedContext>,
    args: IndexProjectArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Only explicit request overrides are passed through; empty lists mean
    // "use the effective project configuration", which the engine resolves
    // from the merged registry configuration.
    let mut options = match args.root_dir.clone() {
        Some(dir) => IndexOptions::new(PathBuf::from(dir)),
        None => IndexOptions::default(),
    }
    .with_extensions(Vec::new())
    .with_exclude_dirs(Vec::new());
    if let Some(extensions) = args.extensions.clone()
        && !extensions.is_empty()
    {
        options = options.with_extensions(extensions);
    }
    if let Some(exclude_dirs) = args.exclude_dirs.clone()
        && !exclude_dirs.is_empty()
    {
        options = options.with_exclude_dirs(exclude_dirs);
    }
    match state.engine.index(args.project_id, options).await {
        Ok(result) => {
            let status = if result.is_success() {
                "success"
            } else {
                "partial"
            };
            let payload = json!({
                "project_id": args.project_id,
                "status": status,
                "total_files": result.total_files,
                "indexed_files": result.indexed_files,
                "failed_files": result.failed_files,
                "total_entities": result.total_entities,
                "total_relations": result.total_relations,
                "errors": result.errors(),
            });
            if result.is_success() {
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    payload.to_string(),
                )]))
            } else {
                Ok(CallToolResult::error(vec![ContentBlock::text(
                    payload.to_string(),
                )]))
            }
        }
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "index_project failed: {error}"
        ))])),
    }
}

/// Run the incremental index tool call.
pub async fn incremental_index(
    state: &Arc<SharedContext>,
    args: IncrementalIndexArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let deleted = args.deleted.clone().unwrap_or_default();
    if !is_valid_deleted_shape(deleted.len(), args.files.len()) {
        return Ok(CallToolResult::error(vec![ContentBlock::text(
            "`deleted` must be empty, length 1, or match the number of files",
        )]));
    }
    let mut changes: Vec<(PathBuf, bool)> = Vec::with_capacity(args.files.len());
    for (index, file) in args.files.iter().enumerate() {
        let is_deleted = match deleted.len() {
            0 => false,
            1 => deleted[0],
            _ => deleted[index],
        };
        changes.push((PathBuf::from(file), is_deleted));
    }
    let coordinator = match state
        .engine
        .get_hot_update_coordinator(args.project_id)
        .await
    {
        Ok(coordinator) => coordinator,
        Err(error) => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "hot update coordinator unavailable: {error}"
            ))]));
        }
    };
    let coordinator = coordinator.lock().await;
    match coordinator.run_explicit_changes(changes).await {
        Ok(result) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "project_id": args.project_id,
                "operation_id": result.operation_id,
                "status": format!("{:?}", result.status),
            })
            .to_string(),
        )])),
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "incremental_index failed: {error}"
        ))])),
    }
}

/// Run the engine health tool call.
pub async fn health(
    state: &Arc<SharedContext>,
    args: ProjectArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let relation = state
        .engine
        .get_relation_capability_info(args.project_id)
        .await
        .ok()
        .map(|info| {
            json!({
                "active_epoch": info.active_epoch,
                "runtime_epoch": info.runtime_epoch,
                "rebuild_required": info.rebuild_required,
            })
        });
    let bm25_enabled = { state.engine.fulltext().diagnose_summary().await.reachable };
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({
            "project_id": args.project_id,
            "metadata_store_configured": state.engine.metadata_store().is_some(),
            "bm25_enabled": bm25_enabled,
            "relation": relation,
        })
        .to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_index_deleted_defaults_to_none() {
        let args: IncrementalIndexArgs =
            serde_json::from_value(serde_json::json!({"project_id": 3, "files": ["src/a.rs"]}))
                .expect("minimal incremental args deserialize");
        assert!(args.deleted.is_none());
    }

    #[test]
    fn deleted_shape_accepts_empty_single_and_aligned() {
        assert!(is_valid_deleted_shape(0, 3));
        assert!(is_valid_deleted_shape(1, 3));
        assert!(is_valid_deleted_shape(3, 3));
        assert!(!is_valid_deleted_shape(2, 3));
    }
}
