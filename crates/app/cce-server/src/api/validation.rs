//! API input validation utilities

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Invalid project_id: {0}")]
    InvalidProjectId(String),

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
