//! Search tool arguments and implementations for the MCP surface.
//!
//! Hybrid and keyword search delegate to the shared query service and the
//! orchestrator keyword tool, so MCP and the HTTP surface observe the same
//! runtime parameters and item shapes.

use cce_server_shared::context::SharedContext;
use cce_server_shared::services::search::{
    FileSearchSpec, SearchService, SharedQueryFields, SingleQuerySpec,
};
use cce_server_shared::services::tools::{self, KeywordOperator, KeywordSpec};
use cce_types::error_codes;
use rmcp::model::{CallToolResult, ContentBlock};

use super::support::tool_failure;
use serde_json::{Value, json};
use std::sync::Arc;

/// Arguments for hybrid search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Project ID to search within (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Natural-language or code query string.
    pub query: String,
    /// Maximum number of results to return (default 10).
    #[serde(default)]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<usize>,
    /// Retrieval source: vector, bm25, or hybrid (default).
    #[serde(default)]
    #[schemars(extend("enum" = ["vector", "bm25", "hybrid"]))]
    pub query_type: Option<String>,
    /// Minimum unified score; results below are dropped.
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Optional directory prefix to restrict results (e.g. "src/parser").
    #[serde(default)]
    pub directory_prefix: Option<String>,
    /// Glob exclude patterns.
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
    /// Glob include patterns.
    #[serde(default)]
    pub include_patterns: Option<Vec<String>>,
    /// Content types to exclude (test, generated, dependency).
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["test", "tests", "generated", "generate", "dependency", "dependencies", "vendor"] }))]
    pub exclude_content_types: Option<Vec<String>>,
    /// File categories to include (code, config, documentation, schema, other).
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["code", "config", "documentation", "schema", "other"] }))]
    pub include_categories: Option<Vec<String>>,
    /// File categories to exclude.
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["code", "config", "documentation", "schema", "other"] }))]
    pub exclude_categories: Option<Vec<String>>,
    /// Rerank master switch override.
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Rerank candidate budget override.
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
    /// Query intent override: semantic, keyword, hybrid, entity.
    #[serde(default)]
    #[schemars(extend("enum" = ["semantic", "keyword", "hybrid", "entity"]))]
    pub query_intent: Option<String>,
    /// Opt-in relation expansion: resolve call-graph neighbours for top hits.
    #[serde(default)]
    pub expand: Option<bool>,
}

/// One sub-query of an aggregated search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AggregatedSubQueryArg {
    /// Query text (must not be blank).
    #[schemars(length(min = 1))]
    pub text: String,
    /// Retrieval source for this sub-query: vector, bm25, or hybrid (default).
    #[serde(default)]
    #[schemars(extend("enum" = ["vector", "bm25", "hybrid"]))]
    pub query_type: Option<String>,
    /// Share of the merged score this sub-query can contribute, in (0, 1].
    /// Defaults to 1.0. The schema uses a closed interval because JSON Schema
    /// cannot express an exclusive lower bound; runtime validation keeps (0, 1].
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub weight: Option<f32>,
}

/// Arguments for aggregated (multi sub-query) search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AggregatedSearchArgs {
    /// Project ID to search within (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Original question the sub-queries were derived from. Used as the rerank
    /// context; when absent the sub-query texts are joined, which a semantic
    /// reranker reads as a synthetic query.
    #[serde(default)]
    pub query: Option<String>,
    /// Sub-queries to run concurrently and merge (1 to 16).
    #[schemars(length(min = 1, max = 16))]
    pub sub_queries: Vec<AggregatedSubQueryArg>,
    /// Maximum number of merged results (default 10).
    #[serde(default)]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<usize>,
    /// Minimum merged score; results below are dropped.
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Optional directory prefix to restrict results (e.g. "src/parser").
    #[serde(default)]
    pub directory_prefix: Option<String>,
    /// Glob exclude patterns.
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
    /// Glob include patterns.
    #[serde(default)]
    pub include_patterns: Option<Vec<String>>,
    /// Content types to exclude (test, generated, dependency).
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["test", "tests", "generated", "generate", "dependency", "dependencies", "vendor"] }))]
    pub exclude_content_types: Option<Vec<String>>,
    /// File categories to include (code, config, documentation, schema, other).
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["code", "config", "documentation", "schema", "other"] }))]
    pub include_categories: Option<Vec<String>>,
    /// File categories to exclude.
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["code", "config", "documentation", "schema", "other"] }))]
    pub exclude_categories: Option<Vec<String>>,
    /// Rerank master switch override.
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Rerank candidate budget override.
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
    /// Query intent override: semantic, keyword, hybrid, entity.
    #[serde(default)]
    #[schemars(extend("enum" = ["semantic", "keyword", "hybrid", "entity"]))]
    pub query_intent: Option<String>,
    /// Opt-in relation expansion: resolve call-graph neighbours for top hits.
    #[serde(default)]
    pub expand: Option<bool>,
}

/// Arguments for fuzzy file-path search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FileSearchArgs {
    /// Project ID to search within (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Fuzzy filename query.
    #[schemars(length(min = 1))]
    pub query: String,
    /// Maximum number of hits (default 10).
    #[serde(default)]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<usize>,
    /// When true attach size and line counts, otherwise paths only.
    #[serde(default)]
    pub detail: Option<bool>,
    /// Optional directory prefix to restrict results (e.g. "src/parser").
    #[serde(default)]
    pub directory_prefix: Option<String>,
    /// Glob exclude patterns.
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
    /// Glob include patterns.
    #[serde(default)]
    pub include_patterns: Option<Vec<String>>,
    /// Content types to exclude (test, generated, dependency).
    #[serde(default)]
    #[schemars(extend("items" = { "type": "string", "enum": ["test", "tests", "generated", "generate", "dependency", "dependencies", "vendor"] }))]
    pub exclude_content_types: Option<Vec<String>>,
}

/// Arguments for keyword (BM25) search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeywordSearchArgs {
    /// Project ID to search within (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Keyword query string.
    pub query: String,
    /// Maximum number of results (default 10).
    #[serde(default)]
    #[schemars(range(min = 1, max = 100))]
    pub top_n: Option<usize>,
    /// Number of results to skip for pagination (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Term operator combining query terms: "or" (default) or "and".
    #[serde(default)]
    #[schemars(extend("enum" = ["or", "and"]))]
    pub term_operator: Option<String>,
}

/// Run a hybrid search tool call.
pub async fn search(
    state: &Arc<SharedContext>,
    args: SearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Query assembly, execution, and conversion live in the shared query
    // service together with the HTTP surface, so both transports observe
    // the same runtime parameters and expansion behavior. Items reuse the
    // shared API DTO shape instead of a parallel MCP-only mapping.
    let service = SearchService::new(state);
    let sources = match SearchService::parse_search_sources(
        args.query_type.as_deref().unwrap_or("hybrid"),
        "query type",
    ) {
        Ok(sources) => sources,
        Err(error) => {
            return Ok(tool_failure(
                error_codes::INVALID_INPUT,
                format!("search failed: {error}"),
            ));
        }
    };
    let mut exclude_content_types = Vec::new();
    for raw in args.exclude_content_types.clone().unwrap_or_default() {
        match SearchService::parse_exclude_content_type(&raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => {
                return Ok(tool_failure(
                    error_codes::INVALID_INPUT,
                    format!("search failed: {error}"),
                ));
            }
        }
    }
    let mut include_categories = Vec::new();
    for raw in args.include_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "include") {
            Ok(category) => include_categories.push(category),
            Err(error) => {
                return Ok(tool_failure(
                    error_codes::INVALID_INPUT,
                    format!("search failed: {error}"),
                ));
            }
        }
    }
    let mut exclude_categories = Vec::new();
    for raw in args.exclude_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "exclude") {
            Ok(category) => exclude_categories.push(category),
            Err(error) => {
                return Ok(tool_failure(
                    error_codes::INVALID_INPUT,
                    format!("search failed: {error}"),
                ));
            }
        }
    }
    let query_intent = match SearchService::parse_query_intent(args.query_intent.as_deref()) {
        Ok(intent) => intent,
        Err(error) => {
            return Ok(tool_failure(
                error_codes::INVALID_INPUT,
                format!("search failed: {error}"),
            ));
        }
    };
    let spec = SingleQuerySpec {
        query: args.query.clone(),
        sources,
        fields: SharedQueryFields {
            limit: args.limit,
            min_score: args.min_score,
            enable_rerank: args.enable_rerank,
            rerank_max_candidates: args.rerank_max_candidates,
            directory_prefix: args.directory_prefix.clone(),
            exclude_content_types,
            include_categories,
            exclude_categories,
            exclude_patterns: args.exclude_patterns.clone().unwrap_or_default(),
            include_patterns: args.include_patterns.clone().unwrap_or_default(),
            query_intent,
            expand: args.expand.unwrap_or(false),
        },
    };
    match service.run_single(args.project_id, spec).await {
        Ok(outcome) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "total": outcome.total,
                "elapsed_ms": outcome.elapsed_ms,
                "sources": outcome.sources,
                "failed_sub_queries": outcome.failed_sub_queries,
                "planned_sub_queries": outcome.planned_sub_queries,
                "executed_sub_queries": outcome.executed_sub_queries,
                "succeeded_sub_queries": outcome.succeeded_sub_queries,
                "sub_query_cache_hits": outcome.sub_query_cache_hits,
                "from_cache": outcome.from_cache,
                "relation_epoch": outcome.relation_epoch,
                "relation_stale": outcome.relation_stale,
                "stages": outcome.stages,
                "items": outcome.items,
            })
            .to_string(),
        )])),
        Err(error) => Ok(tool_failure(
            error_codes::QUERY_ERROR,
            format!("search failed: {error}"),
        )),
    }
}

/// Run an aggregated (multi sub-query) search tool call.
///
/// Exposes the aggregated endpoint to agents that decompose a question
/// themselves: one shared epoch view, one rerank pass over the merged union and
/// one relation expansion. Unlike N independent `search` calls, duplicate
/// bodies collapse and `sub_query_stats` shows which intent paid off.
pub async fn aggregated_search(
    state: &Arc<SharedContext>,
    args: AggregatedSearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_server_shared::services::search::{AggregatedQuerySpec, SubQuerySpec};

    let fail = |message: String| -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(tool_failure(
            error_codes::INVALID_INPUT,
            format!("aggregated_search failed: {message}"),
        ))
    };

    let mut sub_queries = Vec::with_capacity(args.sub_queries.len());
    for sub in &args.sub_queries {
        if sub.text.trim().is_empty() {
            return fail("sub-query text must not be blank".to_string());
        }
        let sources = match SearchService::parse_search_sources(
            sub.query_type.as_deref().unwrap_or("hybrid"),
            "query type in sub-query",
        ) {
            Ok(sources) => sources,
            Err(error) => return fail(error.to_string()),
        };
        sub_queries.push(SubQuerySpec {
            text: sub.text.clone(),
            sources,
            weight: sub.weight.unwrap_or(1.0),
        });
    }
    let mut exclude_content_types = Vec::new();
    for raw in args.exclude_content_types.clone().unwrap_or_default() {
        match SearchService::parse_exclude_content_type(&raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => return fail(error.to_string()),
        }
    }
    let mut include_categories = Vec::new();
    for raw in args.include_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "include") {
            Ok(category) => include_categories.push(category),
            Err(error) => return fail(error.to_string()),
        }
    }
    let mut exclude_categories = Vec::new();
    for raw in args.exclude_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "exclude") {
            Ok(category) => exclude_categories.push(category),
            Err(error) => return fail(error.to_string()),
        }
    }
    let query_intent = match SearchService::parse_query_intent(args.query_intent.as_deref()) {
        Ok(intent) => intent,
        Err(error) => return fail(error.to_string()),
    };

    let spec = AggregatedQuerySpec {
        query: args.query.clone(),
        sub_queries,
        fields: SharedQueryFields {
            limit: args.limit,
            min_score: args.min_score,
            enable_rerank: args.enable_rerank,
            rerank_max_candidates: args.rerank_max_candidates,
            directory_prefix: args.directory_prefix.clone(),
            exclude_content_types,
            include_categories,
            exclude_categories,
            exclude_patterns: args.exclude_patterns.clone().unwrap_or_default(),
            include_patterns: args.include_patterns.clone().unwrap_or_default(),
            query_intent,
            expand: args.expand.unwrap_or(false),
        },
    };
    match SearchService::new(state)
        .run_aggregated(args.project_id, spec)
        .await
    {
        Ok(outcome) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "total": outcome.total,
                "elapsed_ms": outcome.elapsed_ms,
                "sources": outcome.sources,
                "failed_sub_queries": outcome.failed_sub_queries,
                "planned_sub_queries": outcome.planned_sub_queries,
                "executed_sub_queries": outcome.executed_sub_queries,
                "succeeded_sub_queries": outcome.succeeded_sub_queries,
                "sub_query_cache_hits": outcome.sub_query_cache_hits,
                "from_cache": outcome.from_cache,
                "relation_epoch": outcome.relation_epoch,
                "relation_stale": outcome.relation_stale,
                "stages": outcome.stages,
                "sub_query_stats": outcome.sub_query_stats,
                "items": outcome.items,
            })
            .to_string(),
        )])),
        Err(error) => Ok(tool_failure(
            error_codes::QUERY_ERROR,
            format!("aggregated_search failed: {error}"),
        )),
    }
}

/// Run a fuzzy file-path search tool call.
///
/// Delegates to the shared query service so MCP and future HTTP callers
/// share the coordinator policy: the local branch walks the project
/// directory, branches without live files rank indexed file rows instead.
#[allow(clippy::too_many_lines)]
pub async fn file_search(
    state: &Arc<SharedContext>,
    args: FileSearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    if args.project_id <= 0 {
        return Ok(tool_failure(
            error_codes::INVALID_INPUT,
            format!("file search failed: invalid project_id {}", args.project_id),
        ));
    }
    if args.query.trim().is_empty() {
        return Ok(tool_failure(
            error_codes::INVALID_INPUT,
            "file search failed: invalid query: query must not be empty".to_string(),
        ));
    }
    let limit = args.limit.unwrap_or(10);
    if limit == 0 || limit > 100 {
        return Ok(tool_failure(
            error_codes::INVALID_INPUT,
            format!("file search failed: invalid limit {limit}: must be 1..=100"),
        ));
    }
    let service = SearchService::new(state);
    let mut exclude_content_types = Vec::new();
    for raw in args.exclude_content_types.clone().unwrap_or_default() {
        match SearchService::parse_exclude_content_type(&raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => {
                return Ok(tool_failure(
                    error_codes::INVALID_INPUT,
                    format!("search failed: {error}"),
                ));
            }
        }
    }
    let spec = FileSearchSpec {
        query: args.query.clone(),
        limit,
        detail: args.detail.unwrap_or(false),
        directory_prefix: args.directory_prefix.clone(),
        exclude_content_types,
        include_patterns: args.include_patterns.clone().unwrap_or_default(),
        exclude_patterns: args.exclude_patterns.clone().unwrap_or_default(),
    };
    match service.run_file_search(args.project_id, spec).await {
        Ok(outcome) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "total": outcome.total,
                "items": outcome.items.iter().map(|item| json!({
                    "path": item.path,
                    "size_bytes": item.size_bytes,
                    "line_count": item.line_count,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
        )])),
        Err(error) => Ok(tool_failure(
            error_codes::QUERY_ERROR,
            format!("file search failed: {error}"),
        )),
    }
}

/// Run a keyword (BM25) search tool call.
pub async fn keyword_search(
    state: &Arc<SharedContext>,
    args: KeywordSearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let spec = KeywordSpec {
        query: args.query.clone(),
        top_n: args.top_n.unwrap_or(10),
        project_id: args.project_id,
        epoch: None,
        offset: args.offset.unwrap_or(0),
        operator: KeywordOperator::parse(args.term_operator.as_deref()),
    };
    match tools::keyword_search(state, spec).await {
        Ok(response) => {
            let results: Vec<Value> = response
                .results
                .iter()
                .map(|item| {
                    json!({
                        "chunk_id": item.chunk_id,
                        "score": item.score,
                        "file_path": item.file_path,
                        "title": item.title,
                        "snippet": item.snippet,
                        "start_line": item.start_line,
                        "end_line": item.end_line,
                    })
                })
                .collect();
            Ok(CallToolResult::success(vec![ContentBlock::text(
                json!({
                    "query": response.query,
                    "total": response.total,
                    "results": results,
                })
                .to_string(),
            )]))
        }
        Err(error) => Ok(tool_failure(
            error_codes::QUERY_ERROR,
            format!("keyword_search failed: {error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_args_optionals_default_to_none() {
        let args: SearchArgs =
            serde_json::from_value(serde_json::json!({"project_id": 3, "query": "fold"}))
                .expect("minimal search args deserialize");
        assert_eq!(args.project_id, 3);
        assert!(args.limit.is_none());
        assert!(args.query_type.is_none());
        assert!(args.expand.is_none());
    }

    #[test]
    fn aggregated_args_default_optionals() {
        let args: AggregatedSearchArgs = serde_json::from_value(serde_json::json!({
            "project_id": 3,
            "sub_queries": [{"text": "fold"}, {"text": "cache", "weight": 0.5}],
        }))
        .expect("minimal aggregated args deserialize");
        assert_eq!(args.sub_queries.len(), 2);
        assert!(args.sub_queries[0].query_type.is_none());
        assert!(args.sub_queries[0].weight.is_none());
        assert_eq!(args.sub_queries[1].weight, Some(0.5));
        assert!(args.query.is_none());
        assert!(args.limit.is_none());
        assert!(args.expand.is_none());
    }

    #[test]
    fn keyword_args_pagination_defaults_to_none() {
        let args: KeywordSearchArgs =
            serde_json::from_value(serde_json::json!({"project_id": 3, "query": "OnceCell"}))
                .expect("minimal keyword args deserialize");
        assert!(args.top_n.is_none());
        assert!(args.offset.is_none());
        assert!(args.term_operator.is_none());
    }

    #[test]
    fn file_search_args_optionals_default_to_none() {
        let args: FileSearchArgs =
            serde_json::from_value(serde_json::json!({"project_id": 3, "query": "main"}))
                .expect("minimal file search args deserialize");
        assert_eq!(args.project_id, 3);
        assert!(args.limit.is_none());
        assert!(args.detail.is_none());
        assert!(args.directory_prefix.is_none());
        assert!(args.include_patterns.is_none());
        assert!(args.exclude_patterns.is_none());
        assert!(args.exclude_content_types.is_none());
    }

    #[test]
    fn search_schema_carries_value_constraints() {
        use schemars::schema_for;

        let schema = serde_json::to_value(schema_for!(SearchArgs)).expect("schema serializes");
        let props = &schema["properties"];
        assert_eq!(props["project_id"]["minimum"], 1);
        assert_eq!(props["limit"]["minimum"], 1);
        assert_eq!(props["limit"]["maximum"], 100);
        let query_type = props["query_type"]["enum"]
            .as_array()
            .expect("query_type enum is present");
        assert_eq!(query_type.len(), 3);
    }

    #[test]
    fn aggregated_schema_carries_sub_query_bounds() {
        use schemars::schema_for;

        let schema =
            serde_json::to_value(schema_for!(AggregatedSearchArgs)).expect("schema serializes");
        let props = &schema["properties"];
        assert_eq!(props["sub_queries"]["minItems"], 1);
        assert_eq!(props["sub_queries"]["maxItems"], 16);
    }

    #[test]
    fn search_schema_carries_array_element_enums() {
        use schemars::schema_for;

        let schema = serde_json::to_value(schema_for!(SearchArgs)).expect("schema serializes");
        let props = &schema["properties"];

        let content_types = props["exclude_content_types"]["items"]["enum"]
            .as_array()
            .expect("exclude_content_types items enum is present");
        assert!(content_types.iter().any(|v| v == "test"));
        assert!(content_types.iter().any(|v| v == "dependency"));

        let include_categories = props["include_categories"]["items"]["enum"]
            .as_array()
            .expect("include_categories items enum is present");
        assert_eq!(include_categories.len(), 5);
        assert!(include_categories.iter().any(|v| v == "code"));
        assert!(include_categories.iter().any(|v| v == "documentation"));

        let exclude_categories = props["exclude_categories"]["items"]["enum"]
            .as_array()
            .expect("exclude_categories items enum is present");
        assert_eq!(exclude_categories.len(), 5);
    }
}
