//! API input validation utilities

use std::path::Path;
use thiserror::Error;

use cce_storage_sqlite::project_registry::ProjectRegistry;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid project_id: {0}")]
    InvalidProjectId(String),

    #[error("Invalid project_path: {0}")]
    InvalidProjectPath(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Ambiguous project specification: cannot specify both project_id and project_path")]
    AmbiguousProject,

    #[error("Missing project specification: must provide either project_id or project_path")]
    MissingProject,

    #[error("Invalid limit: {0}. Must be between 1 and {1}")]
    InvalidLimit(usize, usize),

    #[error("Invalid glob pattern: {0}")]
    InvalidGlobPattern(String),

    #[error("Too many sub-queries: {0}. Maximum is {1}")]
    TooManySubQueries(usize, usize),

    #[error("Invalid query type: {0}")]
    InvalidQueryType(String),
}

/// Validate project_id
pub fn validate_project_id(project_id: i64) -> Result<(), ValidationError> {
    if project_id <= 0 {
        return Err(ValidationError::InvalidProjectId(
            "must be positive".to_string(),
        ));
    }
    Ok(())
}

/// Resolve project ID from either project_id or project_path
///
/// This function supports two modes:
/// 1. Direct project_id (fast path)
/// 2. Project path lookup (converts path to ID)
///
/// # Arguments
/// * `project_id` - Optional project ID
/// * `project_path` - Optional project root path
/// * `registry` - Project registry for path lookup
///
/// # Returns
/// Resolved project ID
pub async fn resolve_project_id(
    project_id: Option<i64>,
    project_path: Option<&str>,
    registry: &ProjectRegistry,
) -> Result<i64, ValidationError> {
    match (project_id, project_path) {
        (Some(id), None) => {
            // Direct ID mode
            validate_project_id(id)?;
            Ok(id)
        }
        (None, Some(path)) => {
            // Path lookup mode
            if path.is_empty() {
                return Err(ValidationError::InvalidProjectPath(
                    "path cannot be empty".to_string(),
                ));
            }

            // Normalize and canonicalize the path
            let normalized_path = match normalize_and_canonicalize_path(path) {
                Ok(p) => p,
                Err(e) => {
                    return Err(ValidationError::InvalidProjectPath(e));
                }
            };

            // Look up project by path
            match registry.find_by_path(Path::new(&normalized_path)).await {
                Ok(entry) => Ok(entry.metadata.id),
                Err(e) => Err(ValidationError::ProjectNotFound(format!(
                    "{} (error: {})",
                    normalized_path, e
                ))),
            }
        }
        (Some(_), Some(_)) => Err(ValidationError::AmbiguousProject),
        (None, None) => Err(ValidationError::MissingProject),
    }
}

/// Normalize and canonicalize a file path
///
/// This function:
/// 1. Converts to absolute path
/// 2. Resolves symlinks and relative components (.., .)
/// 3. Normalizes path separators
fn normalize_and_canonicalize_path(path: &str) -> Result<String, String> {
    let path_buf = Path::new(path);

    // Try to canonicalize (resolves symlinks, .., etc.)
    match path_buf.canonicalize() {
        Ok(canonical) => Ok(canonical.to_string_lossy().to_string()),
        Err(e) => {
            // If canonicalization fails (e.g., path doesn't exist),
            // try to at least make it absolute
            if path_buf.is_absolute() {
                Ok(path_buf.to_string_lossy().to_string())
            } else {
                match std::env::current_dir() {
                    Ok(cwd) => {
                        let absolute = cwd.join(path_buf);
                        Ok(absolute.to_string_lossy().to_string())
                    }
                    Err(_) => Err(format!("Failed to resolve path '{}': {}", path, e)),
                }
            }
        }
    }
}

/// Validate result limit
pub fn validate_limit(limit: usize, max: usize) -> Result<(), ValidationError> {
    if limit == 0 || limit > max {
        return Err(ValidationError::InvalidLimit(limit, max));
    }
    Ok(())
}

/// Validate glob patterns
pub fn validate_glob_patterns(patterns: &[String]) -> Result<(), ValidationError> {
    for pattern in patterns {
        if pattern.contains('\0') {
            return Err(ValidationError::InvalidGlobPattern(
                "contains null byte".to_string(),
            ));
        }
        if pattern.len() > 1000 {
            return Err(ValidationError::InvalidGlobPattern(
                "pattern too long (max 1000 chars)".to_string(),
            ));
        }
        if pattern.is_empty() {
            return Err(ValidationError::InvalidGlobPattern(
                "pattern is empty".to_string(),
            ));
        }
        if pattern.starts_with('/') {
            return Err(ValidationError::InvalidGlobPattern(
                "pattern must be a relative path (cannot start with '/')".to_string(),
            ));
        }
        if pattern.contains("***") {
            return Err(ValidationError::InvalidGlobPattern(
                "pattern contains invalid consecutive wildcards '***'".to_string(),
            ));
        }
    }
    Ok(())
}

/// Validate sub-queries count
pub fn validate_sub_queries_count(count: usize, max: usize) -> Result<(), ValidationError> {
    if count == 0 {
        return Err(ValidationError::TooManySubQueries(0, max));
    }
    if count > max {
        return Err(ValidationError::TooManySubQueries(count, max));
    }
    Ok(())
}
