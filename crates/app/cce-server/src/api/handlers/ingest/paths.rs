//! Project path resolution for gateway-driven ingest.
//!
//! Resolves registered project roots, confines pushed relative paths inside
//! the project mirror, and maps gateway paths to the canonical storage
//! identity shared with the local scan pipeline.

use std::path::{Component, Path, PathBuf};

use cce_api::models::{ErrorResponse, error_codes};
use cce_storage_metadb_sqlite::ProjectRepository;

use crate::api::state::AppState;

/// Resolve the registered root directory of a project.
pub(crate) fn project_root(state: &AppState, project_id: i64) -> Result<PathBuf, ErrorResponse> {
    let store = state.engine.metadata_store().ok_or_else(|| {
        ErrorResponse::new(error_codes::STORAGE_ERROR, "Metadata store not initialized")
    })?;
    let record = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, project_id))
        .map_err(|e| {
            ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {e}"),
            )
        })?
        .ok_or_else(|| {
            ErrorResponse::new(error_codes::ENTITY_NOT_FOUND, "Project does not exist")
        })?;
    Ok(PathBuf::from(record.root_path))
}

/// Join a gateway relative path onto the project root.
///
/// Absolute paths and parent components are rejected so a pushed manifest
/// can never escape the project mirror.
pub(crate) fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.replace('\\', "/");
    if normalized.trim().is_empty() {
        return Err("relative path must not be empty".to_string());
    }
    let candidate = Path::new(&normalized);
    if candidate.is_absolute() {
        return Err(format!("absolute paths are not accepted: {relative}"));
    }
    for component in candidate.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!("path escapes the project root: {relative}"));
        }
    }
    let joined = root.join(candidate);
    if !joined.starts_with(root) {
        return Err(format!("path escapes the project root: {relative}"));
    }
    Ok(joined)
}

/// Canonical storage identity shared with the local scan pipeline.
pub(crate) fn storage_path(relative: &str) -> String {
    cce_types::path::normalize_project_path(&relative.replace('\\', "/"))
}
