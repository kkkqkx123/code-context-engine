//! Query result and aggregated query types

use super::search_config::SearchConfig;
use super::search_result::SearchResult;
use crate::query::retrieval::core::vector::FilterOptions;

/// Sub-query definition for multi-query aggregation scenarios
#[derive(Debug, Clone)]
pub struct SubQuery {
    /// Query text
    pub text: String,
    /// Retrieval source used for this subquery (e.g. BM25 only, Vector only)
    pub sources: super::query_options::SearchSources,
    /// Weights (used to weight the final result when fusing)
    pub weight: f32,
}

/// Batch/Aggregate Query Options
#[derive(Debug, Clone, Default)]
pub struct AggregatedQueryOptions {
    /// Original user issue (for logging or subsequent LLM processing)
    pub original_query: String,
    /// Project ID for query scoping (all queries must be project-scoped)
    pub project_id: i64,
    /// List of decomposed subqueries
    pub sub_queries: Vec<SubQuery>,
    /// Global configuration (e.g. limit, filters)
    pub global_config: SearchConfig,
    /// filtration conditions
    pub filters: Option<FilterOptions>,
    /// Global exclude patterns (applied to all sub-queries)
    pub exclude_patterns: Vec<String>,
    /// Global include patterns (applied to all sub-queries)
    pub include_patterns: Vec<String>,
    /// Per-request rerank override (see [`super::query_options::QueryOptions::enable_rerank`])
    pub enable_rerank: Option<bool>,
}

/// Query execution result
#[derive(Debug, Clone, Default)]
pub struct QueryResult {
    /// Result items
    pub items: Vec<SearchResult>,
    /// Total count before limiting
    pub total: usize,
    /// Execution time in milliseconds
    pub elapsed_ms: u64,
    /// Query sources used
    pub sources: Vec<String>,
    /// Number of sub-queries executed (for aggregated search)
    pub sub_queries_count: usize,
}
