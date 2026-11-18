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
#[derive(Debug, Clone, PartialEq)]
pub struct SearchInput {
    pub project_id: i64,
    pub query: String,
    pub query_type: String,
    /// Rows wanted for this page, already multiplied by the page number: the
    /// wire request has no offset, so the caller asks for everything up to the
    /// end of its page and slices.
    pub limit: usize,
    pub directory_prefix: Option<String>,
    pub exclude_tests: bool,
    /// Drop hits below this fused score; `None` keeps the server default.
    pub min_score: Option<f32>,
    /// Path globs to keep or drop, in the server's glob syntax.
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    /// Per-request rerank override.
    pub rerank: bool,
}

impl SearchInput {
    fn request(&self) -> SearchRequest {
        SearchRequest {
            project_id: Some(self.project_id),
            project_path: None,
            query: self.query.trim().to_string(),
            query_type: self.query_type.clone(),
            limit: self.limit,
            min_score: self.min_score,
            directory_prefix: self.directory_prefix.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            include_patterns: self.include_patterns.clone(),
            exclude_content_types: if self.exclude_tests {
                vec!["test".to_string()]
            } else {
                Vec::new()
            },
            enable_rerank: Some(self.rerank),
            ..SearchRequest::default()
        }
    }
}

/// One page of a retrieval answer after the client-side slicing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Index of the first row shown, one-based.
    pub first: usize,
    /// Row count on this page.
    pub shown: usize,
    /// Total rows the server reported.
    pub total: usize,
}

impl Page {
    /// Whether the server holds rows past this page.
    pub fn has_more(&self) -> bool {
        self.first.saturating_sub(1) + self.shown < self.total
    }
}

/// Slice one page out of a response.
///
/// The request has no offset, so page `n` asks for `n * page_size` rows and
/// this function drops the earlier ones. `total` decides whether a next page
/// exists, never the number of rows received: a server that caps `limit`
/// would otherwise report "no more rows" while rows remain.
pub fn page(items: &[SearchResultItem], total: usize, page: usize, page_size: usize) -> Page {
    let start = page.saturating_mul(page_size);
    let shown = items.len().saturating_sub(start);
    Page {
        first: if shown == 0 { start } else { start + 1 },
        shown,
        total,
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

/// Aggregated retrieval with the same filters as a single query.
///
/// The filter set is passed as one value instead of seven arguments so the
/// single and aggregated paths cannot drift.
pub struct AggregatedInput {
    pub project_id: i64,
    pub queries: Vec<SubQueryRequest>,
    pub limit: usize,
    pub directory_prefix: Option<String>,
    pub exclude_tests: bool,
    pub min_score: Option<f32>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub rerank: bool,
}

pub fn run_aggregated(
    cx: &App,
    client: Client,
    input: AggregatedInput,
) -> Task<Result<SearchResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = AggregatedSearchRequest {
            project_id: Some(input.project_id),
            project_path: None,
            query: None,
            sub_queries: input.queries,
            limit: input.limit,
            min_score: input.min_score,
            directory_prefix: input.directory_prefix,
            exclude_content_types: if input.exclude_tests {
                vec!["test".to_string()]
            } else {
                Vec::new()
            },
            exclude_patterns: input.exclude_patterns,
            include_patterns: input.include_patterns,
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            enable_rerank: Some(input.rerank),
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
            contributing_sub_queries: 0,
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
            min_score: Some(0.2),
            include_patterns: vec!["src/**".to_string()],
            exclude_patterns: Vec::new(),
            rerank: true,
        };
        let request = input.request();
        assert_eq!(request.project_id, Some(3));
        assert_eq!(request.query, "retry budget");
        assert_eq!(request.exclude_content_types, vec!["test".to_string()]);
        assert_eq!(request.min_score, Some(0.2));
        assert_eq!(request.include_patterns, vec!["src/**".to_string()]);
        assert_eq!(request.enable_rerank, Some(true));
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
    fn a_page_slices_what_the_request_over_fetched() {
        let items: Vec<SearchResultItem> = (0..25).map(|_| item(Vec::new(), Vec::new())).collect();
        let first = page(&items[..10], 120, 0, 10);
        assert_eq!(
            first,
            Page {
                first: 1,
                shown: 10,
                total: 120
            }
        );
        assert!(first.has_more());

        let third = page(&items[..25], 25, 2, 10);
        assert_eq!(
            third,
            Page {
                first: 21,
                shown: 5,
                total: 25
            }
        );
        assert!(!third.has_more(), "the last page stops at total");
    }

    #[test]
    fn the_last_page_is_empty_past_the_end() {
        let items: Vec<SearchResultItem> = vec![item(Vec::new(), Vec::new())];
        let past = page(&items, 1, 4, 10);
        assert_eq!(past.shown, 0);
        assert!(!past.has_more());
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
