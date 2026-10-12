//! Search tool arguments and implementations for the MCP surface.
//!
//! Hybrid and keyword search delegate to the shared query service and the
//! orchestrator keyword tool, so MCP and the HTTP surface observe the same
//! runtime parameters and item shapes.

use cce_api::models::KeywordTermOperator;
use cce_orchestrator::KeywordSearchTool;
use cce_server_shared::services::search::{SearchService, SharedQueryFields, SingleQuerySpec};
use cce_server_shared::state::AppState;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{Value, json};
use std::sync::Arc;

/// Arguments for hybrid search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Project ID to search within (required).
    pub project_id: i64,
    /// Natural-language or code query string.
    pub query: String,
    /// Maximum number of results to return (default 10).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Retrieval source: vector, bm25, hybrid (default), or summary.
    #[serde(default)]
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
    /// Content types to exclude (currently only test/tests).
    #[serde(default)]
    pub exclude_content_types: Option<Vec<String>>,
    /// File categories to include (code, config, documentation, schema, other).
    #[serde(default)]
    pub include_categories: Option<Vec<String>>,
    /// File categories to exclude.
    #[serde(default)]
    pub exclude_categories: Option<Vec<String>>,
    /// Rerank master switch override.
    #[serde(default)]
    pub enable_rerank: Option<bool>,
    /// Rerank candidate budget override.
    #[serde(default)]
    pub rerank_max_candidates: Option<usize>,
    /// Query intent override: semantic, keyword, hybrid, entity.
    #[serde(default)]
    pub query_intent: Option<String>,
    /// Opt-in relation expansion: resolve call-graph neighbours for top hits.
    #[serde(default)]
    pub expand: Option<bool>,
}

/// Arguments for keyword (BM25) search.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeywordSearchArgs {
    /// Project ID to search within (required).
    pub project_id: i64,
    /// Keyword query string.
    pub query: String,
    /// Maximum number of results (default 10).
    #[serde(default)]
    pub top_n: Option<usize>,
    /// Number of results to skip for pagination (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Term operator combining query terms: "or" (default) or "and".
    #[serde(default)]
    pub term_operator: Option<String>,
}

/// Run a hybrid search tool call.
pub async fn search(
    state: &Arc<AppState>,
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
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "search failed: {error}"
            ))]));
        }
    };
    let mut exclude_content_types = Vec::new();
    for raw in args.exclude_content_types.clone().unwrap_or_default() {
        match SearchService::parse_exclude_content_type(&raw) {
            Ok(content_type) => exclude_content_types.push(content_type),
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "search failed: {error}"
                ))]));
            }
        }
    }
    let mut include_categories = Vec::new();
    for raw in args.include_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "include") {
            Ok(category) => include_categories.push(category),
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "search failed: {error}"
                ))]));
            }
        }
    }
    let mut exclude_categories = Vec::new();
    for raw in args.exclude_categories.clone().unwrap_or_default() {
        match SearchService::parse_file_category(&raw, "exclude") {
            Ok(category) => exclude_categories.push(category),
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "search failed: {error}"
                ))]));
            }
        }
    }
    let query_intent = match SearchService::parse_query_intent(args.query_intent.as_deref()) {
        Ok(intent) => intent,
        Err(error) => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "search failed: {error}"
            ))]));
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
                "relation_epoch": outcome.relation_epoch,
                "relation_stale": outcome.relation_stale,
                "items": outcome.items,
            })
            .to_string(),
        )])),
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "search failed: {error}"
        ))])),
    }
}

/// Run a keyword (BM25) search tool call.
pub async fn keyword_search(
    state: &Arc<AppState>,
    args: KeywordSearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let Some(sqlite) = state.engine.metadata_store_clone() else {
        return Ok(CallToolResult::error(vec![ContentBlock::text(
            "metadata store is not configured; keyword search unavailable",
        )]));
    };
    let tool = KeywordSearchTool::new(state.engine.fulltext_clone()).with_sqlite(sqlite);
    let term_operator = match args.term_operator.as_deref().map(str::to_ascii_lowercase) {
        Some(operator) if operator == "and" => KeywordTermOperator::And,
        _ => KeywordTermOperator::Or,
    };
    let request = cce_orchestrator::KeywordSearchRequest {
        query: args.query.clone(),
        top_n: args.top_n.unwrap_or(10),
        project_id: args.project_id,
        epoch: None,
        offset: args.offset.unwrap_or(0),
        term_operator: match term_operator {
            KeywordTermOperator::Or => cce_storage_bm25::TermOperator::Or,
            KeywordTermOperator::And => cce_storage_bm25::TermOperator::And,
        },
    };
    match tool.search(request).await {
        Ok(response) => {
            let results: Vec<Value> = response
                .results
                .iter()
                .map(|item| {
                    let api_item = cce_api::models::KeywordSearchItem {
                        chunk_id: item.chunk_id.clone(),
                        score: item.score,
                        file_path: item.file_path.clone(),
                        title: item.title.clone(),
                        snippet: item.snippet.clone(),
                        start_line: item.start_line,
                        end_line: item.end_line,
                    };
                    json!({
                        "chunk_id": api_item.chunk_id,
                        "score": api_item.score,
                        "file_path": api_item.file_path,
                        "title": api_item.title,
                        "snippet": api_item.snippet,
                        "start_line": api_item.start_line,
                        "end_line": api_item.end_line,
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
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "keyword_search failed: {error}"
        ))])),
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
    fn keyword_args_pagination_defaults_to_none() {
        let args: KeywordSearchArgs =
            serde_json::from_value(serde_json::json!({"project_id": 3, "query": "OnceCell"}))
                .expect("minimal keyword args deserialize");
        assert!(args.top_n.is_none());
        assert!(args.offset.is_none());
        assert!(args.term_operator.is_none());
    }
}
