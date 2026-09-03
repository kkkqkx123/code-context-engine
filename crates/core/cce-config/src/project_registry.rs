//! Project registry data types
//!
//! This module contains the shared types used by the project registry
//! implementation. The runtime registry itself lives in
//! `cce_infrastructure`.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::global::AppConfig;

/// Project metadata used by the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    /// Unique project ID
    pub id: i64,
    /// Project name
    pub name: String,
    /// Root path of the project
    pub root_path: String,
    /// Path to configuration file (relative to root_path)
    pub config_file_path: String,
    /// Primary programming language
    pub language: Option<String>,
    /// File extensions to include
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Directories to exclude
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    /// Whether to respect .gitignore
    #[serde(default)]
    pub respect_gitignore: bool,
    /// Custom ignore patterns
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Last indexed timestamp (ISO 8601 format)
    pub last_indexed: Option<String>,
    /// Created timestamp (ISO 8601 format)
    pub created_at: String,
    /// Updated timestamp (ISO 8601 format)
    pub updated_at: String,
}

/// Project configuration wrapper (includes metadata and full config)
#[derive(Debug, Clone)]
pub struct ProjectEntry {
    /// Project metadata
    pub metadata: ProjectMetadata,
    /// Full application configuration
    pub config: AppConfig,
    /// When this entry was loaded
    pub loaded_at: Instant,
    /// Version number (incremented on each update)
    pub version: u64,
}

/// Error type for project registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Project not found by ID
    #[error("Project not found: {0}")]
    ProjectNotFound(i64),
    /// Path not found
    #[error("Path not found: {0}")]
    PathNotFound(String),
    /// Duplicate project path
    #[error("Duplicate project path: {0}")]
    DuplicatePath(String),
    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    /// IO error
    #[error("IO error: {0}")]
    Io(std::io::Error),
    /// Database error
    #[error("Database error: {0}")]
    Database(String),
}

impl From<std::io::Error> for RegistryError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Immutable project scope that binds a logical project ID and its Qdrant group ID.
///
/// # Invariants
/// - `project_id` is always > 0
/// - `project_group_id` is always non-empty
///
/// Once constructed, the scope is immutable. All components that need project-level
/// isolation receive a `ProjectScope` at construction time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScope {
    project_id: i64,
    project_group_id: String,
}

impl ProjectScope {
    /// Create a new `ProjectScope`, validating that `project_id > 0`
    /// and `project_group_id` is non-empty.
    pub fn new(
        project_id: i64,
        project_group_id: impl Into<String>,
    ) -> Result<Self, RegistryError> {
        if project_id <= 0 {
            return Err(RegistryError::Validation(
                "project_id must be positive".to_string(),
            ));
        }
        let group = project_group_id.into();
        if group.trim().is_empty() {
            return Err(RegistryError::Validation(
                "project_group_id must not be empty".to_string(),
            ));
        }
        Ok(Self {
            project_id,
            project_group_id: group,
        })
    }

    /// The logical project ID.
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// The Qdrant group ID used for payload-scoped isolation.
    pub fn project_group_id(&self) -> &str {
        &self.project_group_id
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectScope;

    #[test]
    fn project_scope_requires_positive_project_id() {
        assert!(ProjectScope::new(0, "project-group").is_err());
        assert!(ProjectScope::new(-1, "project-group").is_err());
    }

    #[test]
    fn project_scope_requires_non_blank_group_id() {
        assert!(ProjectScope::new(1, "").is_err());
        assert!(ProjectScope::new(1, "   ").is_err());
    }

    #[test]
    fn project_scope_preserves_valid_identity() {
        let scope = ProjectScope::new(7, "project-7-root").expect("valid project scope");

        assert_eq!(scope.project_id(), 7);
        assert_eq!(scope.project_group_id(), "project-7-root");
    }
}
