//! Search models

use serde::{Deserialize, Serialize};

/// Search request
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SearchRequest {
    /// Project ID for scoping the query (optional if project_path is provided)
    #[serde(default)]
    pub project_id: Option<i64>,
    /// Project root path (optional if project_id is provided)
    #[serde(default)]
    pub project_path: Option<String>,
    pub query: String,
    #[serde(default = "default_query_type")]
    pub query_type: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Path filter
    #[serde(default)]
    pub directory_prefix: Option<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Content types to exclude (e.g., "test", "generated", "vendor")
    #[serde(default)]
    pub exclude_content_types: Vec<String>,
    /// File extensions filter
    #[serde(default)]
    pub file_extensions: Vec<String>,
    /// Entity types filter
    #[serde(default)]
    pub entity_types: Vec<String>,
    /// Languages filter
    #[serde(default)]
    pub languages: Vec<String>,
    /// Include only specific category values (e.g., ["test", "config"])
    #[serde(default)]
    pub include_categories: Vec<String>,
    /// Exclude specific category values (e.g., ["test", "generated"])
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    #[serde(default = "default_call_chain_depth")]
    pub call_chain_depth: Option<usize>,
    #[serde(default)]
    pub include_call_chain: bool,
    /// Per-request rerank override
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Per-request rerank max candidates override
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
}

/// Search response
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub success: bool,
    pub total: usize,
    pub items: Vec<SearchResultItem>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub sources_used: Vec<String>,
}

/// Search result item
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub score: f32,
    pub file_path: String,
    pub code_chunk: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_chain: Option<Vec<super::entity::CallChainNode>>,
    #[serde(default)]
    pub entity_ids: Vec<u64>,
}

/// Sub-query definition for aggregated search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubQueryRequest {
    /// Query text
    pub text: String,
    /// Query type (vector, bm25, hybrid, summary)
    #[serde(default = "default_subquery_type")]
    pub query_type: String,
    /// Weight for weighted score fusion
    #[serde(default = "default_weight")]
    pub weight: f32,
}

/// Aggregated search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedSearchRequest {
    /// Project ID for scoping the query (optional if project_path is provided)
    #[serde(default)]
    pub project_id: Option<i64>,
    /// Project root path (optional if project_id is provided)
    #[serde(default)]
    pub project_path: Option<String>,
    /// Decomposed sub-queries
    pub sub_queries: Vec<SubQueryRequest>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Path filter
    #[serde(default)]
    pub directory_prefix: Option<String>,
    /// Content types to exclude
    #[serde(default)]
    pub exclude_content_types: Vec<String>,
    /// Global exclude patterns
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Global include patterns
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Include only specific category values
    #[serde(default)]
    pub include_categories: Vec<String>,
    /// Exclude specific category values
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    /// Per-request rerank override
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Per-request rerank max candidates override
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
}

/// Aggregated search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedSearchResponse {
    pub success: bool,
    pub results: Vec<SearchResult>,
    pub total: usize,
    pub elapsed_ms: u64,
    pub sub_queries_count: usize,
    #[serde(default)]
    pub sources_used: Vec<String>,
}

/// Search result used in aggregated search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub score: f32,
    pub file_path: String,
    pub code_chunk: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    pub source: String,
}

fn default_query_type() -> String {
    "hybrid".to_string()
}

fn default_limit() -> usize {
    10
}

fn default_call_chain_depth() -> Option<usize> {
    Some(3)
}

fn default_subquery_type() -> String {
    "hybrid".to_string()
}

fn default_weight() -> f32 {
    1.0
}
