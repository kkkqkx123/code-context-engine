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
    pub database: serde_json::Value,
    /// Active embedder configuration (free-form)
    pub embedder: serde_json::Value,
    /// Full active global configuration snapshot (read-only, free-form)
    pub config: serde_json::Value,
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
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigUpdateRequest {
    /// Project profile document (ProjectAppConfig as JSON). Only
    /// project-effective fields are accepted; global-only sections such as
    /// `server`, `database`, `logger`, `metrics`, `export`, `mcp`,
    /// `search`, `cache` or `symbol_resolution` are rejected.
    pub config: serde_json::Value,
}

/// Response for updating project config
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigUpdateResponse {
    pub success: bool,
    pub hot_reload_applied: bool,
    pub message: String,
    /// Combined profile/global version after the write. Advances when either
    /// the profile or the global configuration changes. Observe the same
    /// value through GET.
    #[serde(default)]
    pub config_version: u64,
}

/// Response for reading the stored project profile
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigGetResponse {
    pub success: bool,
    pub project_id: i64,
    pub config_version: u64,
    /// Stored project profile (ProjectAppConfig as JSON), not the merged
    /// runtime configuration. Combine with the global configuration to
    /// derive the effective values.
    pub config: serde_json::Value,
}

/// Response for reloading project config from file
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectConfigReloadResponse {
    pub success: bool,
    pub message: String,
    pub project_id: i64,
    pub config_version: u64,
}

/// Masked provider summary for the project LLM overview.
///
/// Secrets are never returned. Key state is expressed as counts and flags.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LlmProviderSummary {
    /// Provider id as used by model registries
    pub id: String,
    pub name: String,
    /// Provider type as string ("local" or "remote")
    pub provider_type: String,
    pub base_url: String,
    pub enabled: bool,
    pub has_api_keys: bool,
    pub key_count: usize,
    pub has_unresolved_placeholder: bool,
    pub api_key_file_set: bool,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub rate_limit: u32,
    pub embedding_model_count: usize,
    pub chat_model_count: usize,
    pub rerank_model_count: usize,
}

/// Embedding model entry in the project LLM overview.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EmbeddingModelView {
    pub key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub vector_dimension: usize,
    pub max_batch_tokens: usize,
    pub max_item_tokens: usize,
    pub is_default: bool,
}

/// Chat model entry in the project LLM overview.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChatModelView {
    pub key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub top_p: f32,
    pub max_input_tokens: usize,
    pub is_default: bool,
}

/// Rerank model entry in the project LLM overview.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RerankModelView {
    pub key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub mode: String,
    pub is_default: bool,
}

/// Effective vector dimension compared against the default embedding model.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LlmDimensionStatus {
    pub vector_backend: String,
    pub effective_vector_size: usize,
    pub default_embedding_model: Option<String>,
    pub default_embedding_dimension: Option<usize>,
    pub matches: bool,
    pub requires_reindex: bool,
    pub message: String,
}

/// Read-only project LLM overview.
///
/// Merged runtime configuration rendered as typed views. All secrets stay
/// masked. Embedding and provider globals are read-only here; only chat and
/// rerank selection are project-effective and editable through the typed
/// update endpoint.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectLlmOverviewResponse {
    pub success: bool,
    pub project_id: i64,
    pub config_version: u64,
    pub llm_enabled: bool,
    pub providers: Vec<LlmProviderSummary>,
    pub embedding_models: Vec<EmbeddingModelView>,
    pub chat_models: Vec<ChatModelView>,
    pub rerank_models: Vec<RerankModelView>,
    pub default_embedding: Option<String>,
    pub default_chat: Option<String>,
    pub default_rerank: Option<String>,
    pub rerank_enabled: bool,
    pub rerank_max_candidates: usize,
    pub dimension: LlmDimensionStatus,
    pub missing_api_key_providers: Vec<String>,
    pub unknown_provider_refs: Vec<String>,
    pub ephemeral_note: String,
}

/// Per-model chat parameter patch. All fields are optional; absent fields
/// keep their current values.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChatModelParamPatch {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub max_input_tokens: Option<usize>,
}

/// Typed project LLM update. Only project-effective chat and rerank fields
/// are accepted. Provider connection details and embedding model membership
/// stay global and are changed through the server configuration file.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectLlmUpdateRequest {
    pub default_chat: Option<String>,
    pub default_rerank: Option<String>,
    pub rerank_enabled: Option<bool>,
    pub rerank_max_candidates: Option<usize>,
    pub chat_model_params: Option<std::collections::HashMap<String, ChatModelParamPatch>>,
}

/// Response for the typed project LLM update.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectLlmUpdateResponse {
    pub success: bool,
    pub project_id: i64,
    pub config_version: u64,
    pub hot_reload_applied: bool,
    pub ephemeral: bool,
    pub message: String,
}

/// Connectivity probe request for a configured model.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LlmTestRequest {
    /// Service to probe: "embedding" or "chat"
    pub service: String,
    /// Model key in the corresponding registry
    pub model_key: String,
}

/// Connectivity probe result. The probe uses the stored configuration and
/// discards all transient state afterwards.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LlmTestResponse {
    pub success: bool,
    pub service: String,
    pub model_key: String,
    pub latency_ms: u64,
    pub dimension: Option<usize>,
    pub message: String,
}
