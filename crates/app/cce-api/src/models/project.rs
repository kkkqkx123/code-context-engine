//! Project management models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Project configuration
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ProjectConfig {
    /// Project ID
    pub id: String,
    /// Project name
    pub name: String,
    /// Root directory path
    pub root_path: String,
    /// File extensions to include
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Directories to exclude
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    /// Whether to respect .gitignore
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    /// Additional ignore patterns
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Created timestamp (RFC 3339)
    pub created_at: String,
    /// Last indexed timestamp (RFC 3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed: Option<String>,
}

/// Create project request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// Project name (optional, auto-generated if not provided)
    #[serde(default)]
    pub name: Option<String>,
    /// Root directory path
    pub root_path: String,
    /// File extensions to include
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Directories to exclude
    #[serde(default)]
    pub exclude_dirs: Vec<String>,
    /// Whether to respect .gitignore
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    /// Additional ignore patterns
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

/// Update project request
#[derive(Debug, Serialize, Deserialize, Default, ToSchema)]
pub struct UpdateProjectRequest {
    /// Project name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// File extensions to include
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    /// Directories to exclude
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_dirs: Option<Vec<String>>,
    /// Whether to respect .gitignore
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_gitignore: Option<bool>,
    /// Additional ignore patterns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_patterns: Option<Vec<String>>,
}

/// Project list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectListResponse {
    pub success: bool,
    pub projects: Vec<ProjectConfig>,
    pub total: usize,
}

/// Project detail response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectDetailResponse {
    pub success: bool,
    pub project: ProjectConfig,
}

/// Delete project response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectDeleteResponse {
    pub success: bool,
    pub message: String,
    pub project_id: i64,
}

/// Project index trigger response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectIndexResponse {
    pub success: bool,
    pub project_id: i64,
    pub project_name: String,
    pub indexed_files: usize,
    pub total_entities: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
}

fn default_true() -> bool {
    true
}
