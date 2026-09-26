//! Configuration management models

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Config reload query parameters
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConfigReloadQuery {
    /// Project ID to reload (required)
    pub project_id: Option<i64>,
}

/// Config reload response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigReloadResponse {
    pub success: bool,
    pub message: String,
}

/// Config info response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigInfoResponse {
    pub initialized: bool,
    /// Active database configuration (free-form)
    #[schema(value_type = Object)]
    pub database: serde_json::Value,
    /// Active embedder configuration (free-form)
    #[schema(value_type = Object)]
    pub embedder: serde_json::Value,
    pub project_count: usize,
}

/// Config validate response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigValidateResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub dependency_warnings: Vec<ConfigWarningInfo>,
}

/// Cross-module dependency warning info
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ConfigWarningInfo {
    /// Warning severity ("Warning" or "Info")
    pub severity: String,
    /// The field that has a dependency requirement
    pub field: String,
    /// The field(s) that must be enabled for this feature to work
    pub depends_on: String,
    /// Suggested action to resolve the warning
    pub suggestion: String,
}

/// Request body for updating project config
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProjectConfigUpdateRequest {
    /// Project-level configuration (partial config, free-form)
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
}

/// Response for updating project config
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigUpdateResponse {
    pub success: bool,
    pub hot_reload_applied: bool,
    pub message: String,
}

/// Response for reloading project config from file
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigReloadResponse {
    pub success: bool,
    pub message: String,
    pub project_id: i64,
    pub config_version: u64,
}
