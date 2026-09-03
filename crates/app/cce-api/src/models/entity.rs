//! Entity query models

use serde::{Deserialize, Serialize};

/// Function detail response
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDetailResponse {
    pub success: bool,
    pub function: FunctionInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Function information
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionInfo {
    /// Stable symbol ID (string)
    pub id: String,
    pub name: String,
    pub signature: String,
    pub parameters: Vec<ParameterInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
}

/// Parameter information
#[derive(Debug, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

/// Function calls response
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCallsResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub function_id: String,
    pub function_name: String,
    pub callees: Vec<CallChainNode>,
    pub total_callees: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Function callers response
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCallersResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub function_id: String,
    pub function_name: String,
    pub callers: Vec<CallChainNode>,
    pub total_callers: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Call chain node
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallChainNode {
    pub function_id: String,
    pub function_name: String,
    pub file_path: String,
    pub depth: usize,
    pub relation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_line: Option<usize>,
}

/// Call chain response
#[derive(Debug, Serialize, Deserialize)]
pub struct CallChainResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub function_id: String,
    pub function_name: String,
    pub direction: String,
    pub call_chain: Vec<CallChainNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Call path query parameters
#[derive(Debug, Deserialize)]
pub struct CallPathQuery {
    pub start_id: String,
    pub end_id: String,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

/// Call path response
#[derive(Debug, Serialize, Deserialize)]
pub struct CallPathResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub start_function_id: String,
    pub end_function_id: String,
    pub path_found: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<CallChainNode>,
    pub path_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Class inheritance response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassInheritanceResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub class_id: String,
    pub class_name: String,
    pub base_classes: Vec<ClassRelation>,
    pub derived_classes: Vec<ClassRelation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Class relation
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassRelation {
    pub class_id: String,
    pub class_name: String,
    pub file_path: String,
    pub depth: usize,
}

/// Class implementations response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClassImplementationsResponse {
    pub success: bool,
    pub relation_epoch: i64,
    pub class_id: String,
    pub class_name: String,
    pub implemented_interfaces: Vec<InterfaceRelation>,
    pub implementing_classes: Vec<ClassRelation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_info: Option<serde_json::Value>,
}

/// Interface relation
#[derive(Debug, Serialize, Deserialize)]
pub struct InterfaceRelation {
    pub interface_id: String,
    pub interface_name: String,
    pub file_path: String,
}

/// Entity search request (FTS5)
#[derive(Debug, Serialize, Deserialize)]
pub struct EntitySearchRequest {
    /// Search query (supports FTS5 syntax)
    pub query: String,
    /// Project ID to search within
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    /// Project root path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// Maximum number of results
    #[serde(default = "default_entity_search_limit")]
    pub limit: i64,
    /// Filter by entity kind
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_filter: Option<String>,
}

/// Entity search result
#[derive(Debug, Serialize, Deserialize)]
pub struct EntitySearchResult {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub file_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_start_row: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_end_row: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    pub project_id: i64,
    pub rank: f32,
}

/// Entity search response
#[derive(Debug, Serialize, Deserialize)]
pub struct EntitySearchResponse {
    pub success: bool,
    pub total: usize,
    pub items: Vec<EntitySearchResult>,
    pub elapsed_ms: u64,
}

fn default_max_depth() -> usize {
    10
}

fn default_entity_search_limit() -> i64 {
    20
}
