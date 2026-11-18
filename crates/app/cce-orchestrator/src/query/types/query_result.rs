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
    /// Weight applied to this sub-query's candidate scores before the
    /// cross-query merge (`score *= weight`, then dedup keeps the highest
    /// weighted score per alignment key). Must be finite and in `(0, 1]`;
    /// defaults to 1.0 (neutral).
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
    /// Per-request result-filter override (see
    /// [`super::query_options::QueryOptions::enable_result_filter`])
    pub enable_result_filter: Option<bool>,
    /// Query intent override applied to every sub-query. None means auto-detect.
    pub query_intent: Option<super::query_options::QueryIntent>,
    /// Relation snapshot version observed when the request was assembled.
    /// It participates in the aggregate cache key so expanded results cannot
    /// outlive the graph revision they were rendered from.
    pub relation_epoch: Option<i64>,
    /// Optional neighbour resolver for engine-side relation expansion.
    /// Sub-queries run stripped of the resolver; expansion applies once to
    /// the merged hits (see [`super::query_options::QueryOptions::neighbour_resolver`]).
    pub neighbour_resolver: Option<crate::query::expansion::NeighbourResolver>,
}

/// Terminal state of one sub-query inside an aggregated request.
///
/// Aggregated search degrades instead of failing: a sub-query that errored or
/// ran out of budget is reported rather than silently dropped, so a caller can
/// tell "no match" from "never ran".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubQueryStatus {
    /// Recalled a non-empty candidate list.
    Succeeded,
    /// Ran to completion and recalled nothing.
    Empty,
    /// Failed before producing a candidate list.
    Failed,
    /// Cut off by the aggregated request's sub-query deadline.
    TimedOut,
}

impl SubQueryStatus {
    /// Stable lowercase label used by the transport layer.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
        }
    }
}

/// Per-sub-query execution accounting.
///
/// Weight and budget tuning needs to see which sub-query actually paid off;
/// candidate and contribution counts are what separates "recalled a lot but
/// nothing survived the merge" from "never recalled anything".
#[derive(Debug, Clone)]
pub struct SubQueryStat {
    /// Zero-based position in the caller's sub-query list.
    pub index: usize,
    /// Query text as executed.
    pub text: String,
    /// Terminal state.
    pub status: SubQueryStatus,
    /// Candidates recalled before alignment and merging.
    pub candidates: usize,
    /// Final hits this sub-query contributed to.
    pub contributed: usize,
    /// Wall-clock time spent in this sub-query.
    pub elapsed_ms: u64,
    /// Failure detail for [`SubQueryStatus::Failed`] /
    /// [`SubQueryStatus::TimedOut`].
    pub error: Option<String>,
}

impl SubQueryStat {
    /// Whether this sub-query consumed retrieval budget without contributing a
    /// single final hit: either it never produced a candidate list, or every
    /// candidate it produced lost its merge key to a stronger neighbour.
    pub fn is_wasted(&self) -> bool {
        self.contributed == 0
    }
}

/// Query execution result
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Result items
    pub items: Vec<SearchResult>,
    /// Total count before limiting
    pub total: usize,
    /// Wall-clock time of the current call in milliseconds.
    pub elapsed_ms: u64,
    /// Query sources used
    pub sources: Vec<String>,
    /// Number of sub-queries requested by the caller.
    pub planned_sub_queries: usize,
    /// Number of sub-queries that entered the retrieval pipeline.
    pub executed_sub_queries: usize,
    /// Number of sub-queries that produced a usable candidate list.
    pub succeeded_sub_queries: usize,
    /// Number of executed sub-queries served by the searcher cache.
    pub sub_query_cache_hits: usize,
    /// Sub-query texts that failed during aggregated search (empty for single
    /// queries). Makes partial degradation visible to the caller instead of a
    /// silent union of the surviving sub-queries.
    pub failed_sub_queries: Vec<String>,
    /// Per-sub-query accounting in caller order (empty for single queries).
    /// Carries the detail `failed_sub_queries` cannot: how much each
    /// sub-query recalled, how much of it survived the merge, and whether a
    /// miss was an error or a deadline.
    pub sub_query_stats: Vec<SubQueryStat>,
    /// False when the request degraded and must not populate the result cache.
    pub cacheable: bool,
    /// True when this result was served from the aggregate query cache.
    pub from_cache: bool,
}

impl Default for QueryResult {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            elapsed_ms: 0,
            sources: Vec::new(),
            planned_sub_queries: 1,
            executed_sub_queries: 1,
            succeeded_sub_queries: 1,
            sub_query_cache_hits: 0,
            failed_sub_queries: Vec::new(),
            sub_query_stats: Vec::new(),
            cacheable: true,
            from_cache: false,
        }
    }
}
