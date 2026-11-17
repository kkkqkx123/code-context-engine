//! Retrieval commands: semantic, keyword and aggregated search.
//!
//! Panels hand over their raw inputs; this module assembles the wire request,
//! applies the project scope and the test-file toggle, and hands a graph seed
//! back for hits that carry stable symbol ids.

use cce_api::models::{
    AggregatedSearchRequest, EntitySearchRequest, EntitySearchResponse, SearchRequest,
    SearchResponse, SearchResultItem, SubQueryRequest,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Query types the panel offers, in display order.
pub const QUERY_TYPES: [&str; 3] = ["hybrid", "vector", "bm25"];

/// One single-query retrieval request as the panel collects it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchInput {
    pub project_id: i64,
    pub query: String,
    pub query_type: String,
    pub limit: usize,
    pub directory_prefix: Option<String>,
    pub exclude_tests: bool,
}

impl SearchInput {
    fn request(&self) -> SearchRequest {
        SearchRequest {
            project_id: Some(self.project_id),
            project_path: None,
            query: self.query.trim().to_string(),
            query_type: self.query_type.clone(),
            limit: self.limit,
            directory_prefix: self.directory_prefix.clone(),
            exclude_content_types: if self.exclude_tests {
                vec!["test".to_string()]
            } else {
                Vec::new()
            },
            ..SearchRequest::default()
        }
    }
}

pub fn run(
    cx: &App,
    client: Client,
    input: SearchInput,
) -> Task<Result<SearchResponse, ClientError>> {
    let request = input.request();
    runtime::spawn(cx, async move { client.search(&request).await })
}

pub fn run_entity(
    cx: &App,
    client: Client,
    project_id: i64,
    query: String,
    limit: i64,
) -> Task<Result<EntitySearchResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = EntitySearchRequest {
            query: query.trim().to_string(),
            project_id: Some(project_id),
            project_path: None,
            limit,
            kind_filter: None,
        };
        client.entity_search(&request).await
    })
}

/// One sub-query per line, each weighted equally.
///
/// The panel does not expose per-sub-query weights: a weighted form without a
/// visible scale invites mistyped numbers, and equal weights already express
/// the intent of "search these phrases together".
pub fn sub_queries(text: &str, query_type: &str) -> Vec<SubQueryRequest> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| SubQueryRequest {
            text: line.to_string(),
            query_type: query_type.to_string(),
            weight: 1.0,
        })
        .collect()
}

pub fn run_aggregated(
    cx: &App,
    client: Client,
    project_id: i64,
    queries: Vec<SubQueryRequest>,
    limit: usize,
    exclude_tests: bool,
) -> Task<Result<SearchResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = AggregatedSearchRequest {
            project_id: Some(project_id),
            project_path: None,
            sub_queries: queries,
            limit,
            min_score: None,
            directory_prefix: None,
            exclude_content_types: if exclude_tests {
                vec!["test".to_string()]
            } else {
                Vec::new()
            },
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            enable_rerank: None,
            rerank_max_candidates: None,
            query_intent: None,
            expand: None,
        };
        client.search_aggregated(&request).await
    })
}

/// Seed the graph panel with the first stable symbol of a hit.
///
/// File-level hits carry no symbol id, so they stay in the search panel: a
/// graph query without a stable id has nothing to resolve.
pub fn graph_seed(item: &SearchResultItem) -> Option<(String, String)> {
    let entity_id = item.stable_symbol_ids.first()?.clone();
    let label = item
        .entity_names
        .first()
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", item.file_path, item.start_line));
    Some((entity_id, label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_api::models::ContentStateDto;

    fn item(stable_symbol_ids: Vec<String>, entity_names: Vec<String>) -> SearchResultItem {
        SearchResultItem {
            file_path: "src/lib.rs".to_string(),
            code_chunk: String::new(),
            start_line: 10,
            end_line: 20,
            entity_type: None,
            entity_names,
            entity_ids: Vec::new(),
            stable_symbol_ids,
            score: 0.5,
            sources: Vec::new(),
            content_state: ContentStateDto::Full,
            expanded: false,
            expanded_nodes: 0,
            forward_nodes: 0,
            backward_nodes: 0,
            omitted_self_evident: 0,
            secondary_evidence: None,
            entity_scores: Vec::new(),
        }
    }

    #[test]
    fn request_applies_scope_and_test_filter() {
        let input = SearchInput {
            project_id: 3,
            query: "  retry budget  ".to_string(),
            query_type: "hybrid".to_string(),
            limit: 10,
            directory_prefix: None,
            exclude_tests: true,
        };
        let request = input.request();
        assert_eq!(request.project_id, Some(3));
        assert_eq!(request.query, "retry budget");
        assert_eq!(request.exclude_content_types, vec!["test".to_string()]);
    }

    #[test]
    fn sub_queries_skip_blank_lines() {
        let queries = sub_queries("alpha\n\n  beta  \n", "bm25");
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[1].text, "beta");
        assert_eq!(queries[0].query_type, "bm25");
        assert!((queries[0].weight - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn graph_seed_needs_a_stable_symbol() {
        assert!(graph_seed(&item(Vec::new(), vec!["load".to_string()])).is_none());
        let seed = graph_seed(&item(vec!["sym_abc".to_string()], vec!["load".to_string()]));
        assert_eq!(seed, Some(("sym_abc".to_string(), "load".to_string())));
    }

    #[test]
    fn graph_seed_falls_back_to_the_location() {
        let seed = graph_seed(&item(vec!["sym_abc".to_string()], Vec::new()));
        assert_eq!(
            seed,
            Some(("sym_abc".to_string(), "src/lib.rs:10".to_string()))
        );
    }
}
