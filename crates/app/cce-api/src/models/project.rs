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

/// Project metadata key marking gateway-supplied projects.
pub const SUPPLY_MODE_KEY: &str = "supply_mode";

/// Gateway supply marker value. Projects carrying it receive all changes
/// through the gateway event entry; local filesystem watching is refused.
pub const SUPPLY_MODE_GATEWAY: &str = "gateway";

/// Local supply marker value, the default for single-host projects.
pub const SUPPLY_MODE_LOCAL: &str = "local";

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
    /// File supply mode: local or gateway. Remote templates set gateway
    /// so the host refuses local watching and expects gateway events.
    #[serde(default)]
    pub supply_mode: Option<String>,
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
    /// File supply mode: local or gateway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply_mode: Option<String>,
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
