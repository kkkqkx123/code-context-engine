//! Execute index handler
//!
//! This module provides handlers for full index execution.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use std::path::PathBuf;

use cce_api::models::IndexResponse;
use cce_orchestrator::{IndexOptions, IndexResult};

/// Index query parameters
#[derive(Debug, Deserialize)]
pub struct IndexQuery {
    /// Project ID for project-specific configuration
    pub project_id: i64,
    /// Root directory to index
    pub path: String,
    /// File extensions to include (e.g., ["rs", "py"])
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Directories to exclude (e.g., ["target", "node_modules"])
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    /// Whether to respect .gitignore files
    #[serde(default = "default_respect_gitignore")]
    pub respect_gitignore: bool,
    /// Additional ignore patterns (gitignore-style)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Path to custom gitignore file
    pub custom_gitignore: Option<String>,
}

fn default_respect_gitignore() -> bool {
    true
}

fn index_response_from_result(result: IndexResult) -> IndexResponse {
    let has_errors = !result.errors().is_empty();
    IndexResponse {
        success: result.is_success(),
        files_scanned: result.total_files,
        files_indexed: result.indexed_files,
        failed_files: result.failed_files,
        total_entities: result.total_entities,
        total_relations: result.total_relations,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
        message: if !has_errors {
            format!(
                "Indexing completed, a total of {} files were processed, {} entities were extracted, and {} relationships were identified.",
                result.indexed_files, result.total_entities, result.total_relations
            )
        } else {
            format!(
                "The indexing process is complete. {} files were successful, and {} files failed.",
                result.indexed_files, result.failed_files
            )
        },
        errors: result.errors().to_vec(),
    }
}

/// Handle index request
#[axum::debug_handler]
pub async fn handle_index(
    State(state): State<crate::api::state::AppState>,
    Json(query): Json<IndexQuery>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // Validate project_id
    if let Err(e) = crate::api::validation::validate_project_id(query.project_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(IndexResponse {
                success: false,
                files_scanned: 0,
                files_indexed: 0,
                failed_files: 0,
                total_entities: 0,
                total_relations: 0,
                total_vectors: 0,
                elapsed_ms: 0,
                message: format!("Invalid project_id: {}", e),
                errors: vec![e.to_string()],
            }),
        );
    }

    // Validate root directory
    let root_dir = PathBuf::from(&query.path);
    if !root_dir.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(IndexResponse {
                success: false,
                files_scanned: 0,
                files_indexed: 0,
                failed_files: 0,
                total_entities: 0,
                total_relations: 0,
                total_vectors: 0,
                elapsed_ms: 0,
                message: format!("Directory does not exist: {}", query.path),
                errors: vec![format!("Directory does not exist: {}", query.path)],
            }),
        );
    }

    if !root_dir.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(IndexResponse {
                success: false,
                files_scanned: 0,
                files_indexed: 0,
                failed_files: 0,
                total_entities: 0,
                total_relations: 0,
                total_vectors: 0,
                elapsed_ms: 0,
                message: format!("The path is not a directory: {}", query.path),
                errors: vec![format!("The path is not a directory: {}", query.path)],
            }),
        );
    }

    // Build index options
    let mut options = IndexOptions::new(&root_dir)
        .with_extensions(if query.extensions.is_empty() {
            vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "java".to_string(),
            ]
        } else {
            query.extensions
        })
        .with_exclude_dirs(if query.exclude_dirs.is_empty() {
            vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "vendor".to_string(),
            ]
        } else {
            query.exclude_dirs
        })
        .with_gitignore(query.respect_gitignore)
        .with_ignore_patterns(query.ignore_patterns);

    if let Some(custom_gitignore) = query.custom_gitignore {
        options = options.with_custom_gitignore(custom_gitignore);
    }

    // Execute indexing with project_id using engine's index method
    let result = match state.engine.index(query.project_id, options).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IndexResponse {
                    success: false,
                    files_scanned: 0,
                    files_indexed: 0,
                    failed_files: 0,
                    total_entities: 0,
                    total_relations: 0,
                    total_vectors: 0,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    message: format!("Index failure: {}", e),
                    errors: vec![e.to_string()],
                }),
            );
        }
    };

    let response = index_response_from_result(result);
    let status_code = if response.success {
        StatusCode::OK
    } else {
        StatusCode::PARTIAL_CONTENT
    };

    (status_code, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_query_default() {
        let query = IndexQuery {
            project_id: 1,
            path: "/test".to_string(),
            extensions: vec![],
            exclude_dirs: vec![],
            respect_gitignore: true,
            ignore_patterns: vec![],
            custom_gitignore: None,
        };

        assert!(query.respect_gitignore); // default
        assert!(query.extensions.is_empty());
    }

    #[test]
    fn test_index_response_from_result() {
        let result = IndexResult {
            total_files: 10,
            indexed_files: 9,
            failed_files: 1,
            total_entities: 100,
            total_relations: 50,
            total_vectors: 150,
            elapsed_ms: 1000,
            ..Default::default()
        };

        let response = index_response_from_result(result);
        assert!(response.success);
        assert_eq!(response.files_scanned, 10);
        assert_eq!(response.files_indexed, 9);
        assert_eq!(response.failed_files, 1);
    }
}
