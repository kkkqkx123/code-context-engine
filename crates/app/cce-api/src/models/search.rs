//! Search models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Search request
#[derive(Debug, Serialize, Deserialize, Default, ToSchema)]
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
    /// Content types to exclude (currently only "test" / "tests" is supported)
    #[serde(default)]
    pub exclude_content_types: Vec<String>,
    /// Include only specific category values (e.g., ["code", "config"])
    #[serde(default)]
    pub include_categories: Vec<String>,
    /// Exclude specific category values (e.g., ["code", "config"])
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    /// Per-request rerank override
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Per-request rerank max candidates override
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
    /// Query intent override ("semantic", "keyword", "hybrid", "entity").
    /// Empty means auto-detect.
    #[serde(default)]
    pub query_intent: Option<String>,
    /// Per-request relation-expansion opt-in. When `Some(true)` the server
    /// resolves call-graph neighbours for the top-N hits and annotates them
    /// into `code_chunk`; `None` defers to the project configuration.
    #[serde(default)]
    pub expand: Option<bool>,
}

/// Search response shared by single and aggregated search endpoints
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub success: bool,
    pub total: usize,
    pub items: Vec<SearchResultItem>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub sources_used: Vec<String>,
    /// Sub-query texts that failed during an aggregated search (empty for
    /// single queries); makes partial degradation visible to the caller.
    #[serde(default)]
    pub failed_sub_queries: Vec<String>,
    /// Number of sub-queries executed by the engine.
    #[serde(default)]
    pub sub_queries_count: usize,
    /// Whether the result was served from the shared query result cache.
    #[serde(default)]
    pub from_cache: bool,
    /// Relation epoch the backing index was at when the query ran, when a
    /// relation snapshot is available. Lets callers judge result freshness
    /// against graph query versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_epoch: Option<i64>,
    /// True when the serving relation snapshot reports itself stale.
    #[serde(default)]
    pub relation_stale: bool,
}

/// Whether a result carries its full body or a file-and-range reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContentStateDto {
    /// `code_chunk` is the full source body.
    #[default]
    Full,
    /// `code_chunk` is a reference because the body exceeded the token budget.
    ReferenceOverLimit,
    /// `code_chunk` is a reference because the source file is unreadable.
    ReferenceFileMissing,
    /// `code_chunk` is a reference because the hit is file-level.
    ReferenceFileLevel,
    /// `code_chunk` is a reference because no chunk record exists for the hit.
    ReferenceChunkMissing,
}

/// Search result item
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SearchResultItem {
    pub file_path: String,
    pub code_chunk: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    #[serde(default)]
    pub entity_names: Vec<String>,
    /// All entity IDs associated with this result (a chunk may contain
    /// multiple entities).
    #[serde(default)]
    pub entity_ids: Vec<i64>,
    /// Stable symbol IDs for the hit's entities, joinable against the graph
    /// query interfaces. Empty when no relation snapshot is available.
    #[serde(default)]
    pub stable_symbol_ids: Vec<String>,
    /// Unified relevance score (post-fusion).
    pub score: f32,
    /// Retrieval sources that contributed to this result (e.g. "vector", "bm25").
    #[serde(default)]
    pub sources: Vec<String>,
    /// Number of aggregated sub-queries that hit this result's alignment key.
    /// Zero for single-query results.
    #[serde(default)]
    pub contributing_sub_queries: usize,
    /// Whether `code_chunk` is the full body or a reference.
    #[serde(default)]
    pub content_state: ContentStateDto,
    /// True when relation expansion attached at least one neighbour unit.
    #[serde(default)]
    pub expanded: bool,
    /// Number of attached expansion units (forward + backward).
    #[serde(default)]
    pub expanded_nodes: usize,
    /// Attached callee (forward) units.
    #[serde(default)]
    pub forward_nodes: usize,
    /// Attached caller (backward) units.
    #[serde(default)]
    pub backward_nodes: usize,
    /// Forward units dropped as directly inferable from the rendered body.
    #[serde(default)]
    pub omitted_self_evident: usize,
    /// Representative hit of the recall path that lost the hybrid fusion tie.
    ///
    /// Only a reference (chunk id, scores, line range), never a body: the
    /// result itself is still the winning path's chunk. Absent for single-path
    /// hits and non-hybrid strategies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_evidence: Option<SecondaryEvidenceDto>,
    /// Per-entity fused scores when one physical chunk carries several
    /// entities (chunk grouping is enabled). Empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_scores: Vec<EntityScoreDto>,
}

/// Recall path of a secondary evidence reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecallPathDto {
    /// Dense vector path.
    Vector,
    /// BM25 full-text path.
    Bm25,
}

/// Reference to the losing recall path's representative chunk.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecondaryEvidenceDto {
    /// Path the reference belongs to.
    pub path: RecallPathDto,
    /// Chunk id of that path's representative hit.
    pub chunk_id: String,
    /// Path score normalized to `[0, 1]`.
    pub normalized_score: f32,
    /// Raw retrieval score on that path.
    pub raw_score: f32,
    /// One-based start line the path knows about (0 when unknown).
    #[serde(default)]
    pub start_line: u32,
    /// One-based end line the path knows about (0 when unknown).
    #[serde(default)]
    pub end_line: u32,
}

/// Fused score of one entity inside a multi-entity chunk group.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EntityScoreDto {
    /// Entity the score belongs to.
    pub entity_id: i64,
    /// Fused score that entity earned on its own alignment key.
    pub score: f32,
}

/// Sub-query definition for aggregated search
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubQueryRequest {
    /// Query text
    pub text: String,
    /// Query type (vector, bm25, hybrid)
    #[serde(default = "default_subquery_type")]
    pub query_type: String,
    /// Weight applied to this sub-query's candidate scores before the
    /// cross-query merge (`score *= weight`, then dedup keeps the highest
    /// weighted score per alignment key). Must be finite and >= 0.
    #[serde(default = "default_weight")]
    pub weight: f32,
}

/// Aggregated search request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    /// Query intent override applied to every sub-query.
    /// Empty means auto-detect per sub-query text.
    #[serde(default)]
    pub query_intent: Option<String>,
    /// Per-request relation-expansion opt-in, applied to the merged hits.
    #[serde(default)]
    pub expand: Option<bool>,
}

fn default_query_type() -> String {
    "hybrid".to_string()
}

fn default_limit() -> usize {
    10
}

fn default_subquery_type() -> String {
    "hybrid".to_string()
}

fn default_weight() -> f32 {
    1.0
}
