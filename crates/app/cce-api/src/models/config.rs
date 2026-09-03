//! Configuration management models

use serde::{Deserialize, Serialize};

/// Config reload response
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigReloadResponse {
    pub success: bool,
    pub message: String,
}

/// Config info response
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigInfoResponse {
    pub initialized: bool,
    pub database: serde_json::Value,
    pub embedder: serde_json::Value,
    pub project_count: usize,
}

/// Config validate response
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigValidateResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub dependency_warnings: Vec<DependencyWarning>,
}

/// Dependency warning info
#[derive(Debug, Serialize, Deserialize)]
pub struct DependencyWarning {
    pub level: String,
    pub message: String,
    pub module: String,
}
