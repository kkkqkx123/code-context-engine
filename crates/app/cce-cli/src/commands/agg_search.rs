//! Aggregated search command handler

use anyhow::Result;
use clap::Parser;

use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_success, truncate};
use cce_api::models::{AggregatedSearchRequest, SearchResponse, SearchResultItem, SubQueryRequest};
use cce_service::CodeContextService;

/// Execute multi-query aggregated search
#[derive(Parser, Debug)]
#[command(name = "agg-search", about = "Advanced search with multiple queries")]
pub struct AggSearchCommand {
    /// Project ID for scoping the query (optional)
    #[arg(short, long)]
    pub project_id: Option<i64>,

    /// Project root path (optional if project-id is provided)
    #[arg(long)]
    pub project_path: Option<String>,

    /// BM25 keyword queries (append @weight for a per-query override, e.g. "login flow@1.5")
    #[arg(long = "bm25")]
    pub bm25_queries: Vec<String>,

    /// Vector semantic queries (append @weight for a per-query override)
    #[arg(long = "vector")]
    pub vector_queries: Vec<String>,

    /// Hybrid queries (append @weight for a per-query override)
    #[arg(long = "hybrid")]
    pub hybrid_queries: Vec<String>,

    /// Summary queries (append @weight for a per-query override)
    #[arg(long = "summary")]
    pub summary_queries: Vec<String>,

    /// Weight applied to BM25 sub-query scores
    #[arg(long, default_value_t = 1.2)]
    pub bm25_weight: f32,

    /// Weight applied to vector sub-query scores
    #[arg(long, default_value_t = 1.0)]
    pub vector_weight: f32,

    /// Weight applied to hybrid sub-query scores
    #[arg(long, default_value_t = 1.0)]
    pub hybrid_weight: f32,

    /// Weight applied to summary sub-query scores
    #[arg(long, default_value_t = 1.0)]
    pub summary_weight: f32,

    /// Result limit
    #[arg(short, long, default_value_t = 10)]
    pub limit: usize,

    /// Minimum score threshold
    #[arg(long)]
    pub min_score: Option<f32>,

    /// Directory prefix filter
    #[arg(long)]
    pub directory: Option<String>,

    /// Content types to exclude (comma-separated): test, generated, vendor
    #[arg(long)]
    pub exclude_content_types: Option<String>,

    /// Exclude patterns (comma-separated)
    #[arg(long)]
    pub exclude_patterns: Option<String>,

    /// Include patterns (comma-separated)
    #[arg(long)]
    pub include_patterns: Option<String>,

    /// Include only specific category values (comma-separated)
    #[arg(long)]
    pub include_categories: Option<String>,

    /// Exclude specific category values (comma-separated)
    #[arg(long)]
    pub exclude_categories: Option<String>,

    /// Force reranking on/off for this query (defaults to config)
    #[arg(long)]
    pub enable_rerank: Option<bool>,

    /// Override the maximum number of rerank candidates
    #[arg(long)]
    pub rerank_max_candidates: Option<usize>,

    /// Query intent override applied to every sub-query (empty means auto-detect)
    #[arg(long)]
    pub query_intent: Option<String>,

    /// Resolve call-graph neighbours for the merged top hits
    #[arg(long)]
    pub expand: Option<bool>,
}

/// Split a per-query weight suffix of the form `text@weight` from a raw
/// sub-query value. A trailing segment after the last `@` is treated as a
/// weight only when it parses to a finite non-negative number and the text
/// part is non-empty; otherwise the whole value is kept as query text with
/// the type-level fallback weight.
fn split_weighted_query(raw: &str, fallback: f32) -> (String, f32) {
    match raw.rsplit_once('@') {
        Some((text, weight)) if !text.trim().is_empty() => match weight.trim().parse::<f32>() {
            Ok(w) if w.is_finite() && w >= 0.0 => (text.trim().to_string(), w),
            _ => (raw.to_string(), fallback),
        },
        _ => (raw.to_string(), fallback),
    }
}

impl AggSearchCommand {
    pub async fn execute(&self, client: &ApiClient, verbose: bool) -> Result<()> {
        let request = self.build_request()?;
        if verbose {
            println!("Project ID: {:?}", self.project_id);
            println!("Sub-queries: {}", request.sub_queries.len());
            for (i, sq) in request.sub_queries.iter().enumerate() {
                println!(
                    "  {}. [{}] {} (weight: {:.1})",
                    i + 1,
                    sq.query_type,
                    sq.text,
                    sq.weight
                );
            }
        }
        let response: SearchResponse = client.search_aggregated(&request).await?;
        Self::render(&response);
        Ok(())
    }

    /// Execute through the domain service abstraction with identical semantics.
    pub async fn execute_with_service(
        &self,
        service: &dyn cce_service::CodeContextService,
        verbose: bool,
    ) -> Result<()> {
        let request = self.build_request()?;
        if verbose {
            println!("Project ID: {:?}", self.project_id);
            println!("Sub-queries: {}", request.sub_queries.len());
        }
        match service.search_aggregated(request).await {
            Ok(response) => {
                Self::render(&response);
                Ok(())
            }
            Err(e) => {
                print_error(&format!("Aggregated search failed: {e}"));
                Ok(())
            }
        }
    }

    fn build_request(&self) -> Result<AggregatedSearchRequest> {
        let mut sub_queries = Vec::new();
        for query in &self.bm25_queries {
            let (text, weight) = split_weighted_query(query, self.bm25_weight);
            sub_queries.push(SubQueryRequest {
                text,
                query_type: "bm25".to_string(),
                weight,
            });
        }
        for query in &self.vector_queries {
            let (text, weight) = split_weighted_query(query, self.vector_weight);
            sub_queries.push(SubQueryRequest {
                text,
                query_type: "vector".to_string(),
                weight,
            });
        }
        for query in &self.hybrid_queries {
            let (text, weight) = split_weighted_query(query, self.hybrid_weight);
            sub_queries.push(SubQueryRequest {
                text,
                query_type: "hybrid".to_string(),
                weight,
            });
        }
        for query in &self.summary_queries {
            let (text, weight) = split_weighted_query(query, self.summary_weight);
            sub_queries.push(SubQueryRequest {
                text,
                query_type: "summary".to_string(),
                weight,
            });
        }
        if sub_queries.is_empty() {
            anyhow::bail!(
                "at least one sub-query is required (--bm25, --vector, --hybrid or --summary)"
            );
        }
        let exclude_content_types: Vec<String> = self
            .exclude_content_types
            .as_ref()
            .map(|s| s.split(',').map(|ct| ct.trim().to_string()).collect())
            .unwrap_or_default();
        let exclude_patterns: Vec<String> = self
            .exclude_patterns
            .as_ref()
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default();
        let include_patterns: Vec<String> = self
            .include_patterns
            .as_ref()
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default();
        let include_categories: Vec<String> = self
            .include_categories
            .as_ref()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default();
        let exclude_categories: Vec<String> = self
            .exclude_categories
            .as_ref()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default();
        Ok(AggregatedSearchRequest {
            project_id: self.project_id,
            project_path: self.project_path.clone(),
            sub_queries,
            limit: self.limit,
            min_score: self.min_score,
            directory_prefix: self.directory.clone(),
            exclude_content_types,
            exclude_patterns,
            include_patterns,
            include_categories,
            exclude_categories,
            enable_rerank: self.enable_rerank,
            rerank_max_candidates: self.rerank_max_candidates,
            query_intent: self.query_intent.clone(),
            expand: self.expand,
        })
    }

    fn render(response: &SearchResponse) {
        if response.success {
            print_success(&format!(
                "Found {} results in {}",
                response.total,
                format_duration(response.elapsed_ms)
            ));
            println!();
            if response.items.is_empty() {
                println!("No results found");
            } else {
                for (i, item) in response.items.iter().enumerate() {
                    print_result_item(i + 1, item);
                }
            }
        } else {
            print_error("Aggregated search failed");
        }
    }
}

fn print_result_item(index: usize, item: &SearchResultItem) {
    println!(
        "{index}. {path} {start}-{end}",
        index = index,
        path = truncate(&item.file_path, 50),
        start = item.start_line,
        end = item.end_line
    );
    if let Some(ref entity_type) = item.entity_type {
        println!("   Type: {}", entity_type);
    }
    let lines: Vec<&str> = item.code_chunk.lines().take(3).collect();
    for line in lines {
        println!("   {}", line);
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_suffix_overrides_fallback() {
        let (text, weight) = split_weighted_query("login flow@1.5", 1.2);
        assert_eq!(text, "login flow");
        assert_eq!(weight, 1.5);
    }

    #[test]
    fn plain_query_keeps_fallback_weight() {
        let (text, weight) = split_weighted_query("login flow", 1.2);
        assert_eq!(text, "login flow");
        assert_eq!(weight, 1.2);
    }

    #[test]
    fn non_numeric_suffix_stays_query_text() {
        let (text, weight) = split_weighted_query("user@example.com", 1.0);
        assert_eq!(text, "user@example.com");
        assert_eq!(weight, 1.0);
    }

    #[test]
    fn invalid_weight_falls_back_to_full_text() {
        let (text, weight) = split_weighted_query("query@abc", 1.2);
        assert_eq!(text, "query@abc");
        assert_eq!(weight, 1.2);
    }

    #[test]
    fn negative_weight_falls_back_to_full_text() {
        let (text, weight) = split_weighted_query("query@-1.0", 1.2);
        assert_eq!(text, "query@-1.0");
        assert_eq!(weight, 1.2);
    }
}
