//! Aggregated search command handler

use anyhow::Result;
use clap::Parser;

use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_success, truncate};
use cce_api::models::{AggregatedSearchRequest, SearchResponse, SearchResultItem, SubQueryRequest};

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

    /// BM25 keyword queries
    #[arg(long = "bm25")]
    pub bm25_queries: Vec<String>,

    /// Vector semantic queries
    #[arg(long = "vector")]
    pub vector_queries: Vec<String>,

    /// Hybrid queries
    #[arg(long = "hybrid")]
    pub hybrid_queries: Vec<String>,

    /// Summary queries
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

impl AggSearchCommand {
    pub async fn execute(&self, client: &ApiClient, verbose: bool) -> Result<()> {
        // Build sub-queries list
        let mut sub_queries = Vec::new();

        // Add BM25 sub-queries
        for query in &self.bm25_queries {
            sub_queries.push(SubQueryRequest {
                text: query.clone(),
                query_type: "bm25".to_string(),
                weight: self.bm25_weight,
            });
        }

        // Add Vector sub-queries
        for query in &self.vector_queries {
            sub_queries.push(SubQueryRequest {
                text: query.clone(),
                query_type: "vector".to_string(),
                weight: self.vector_weight,
            });
        }

        // Add Hybrid sub-queries
        for query in &self.hybrid_queries {
            sub_queries.push(SubQueryRequest {
                text: query.clone(),
                query_type: "hybrid".to_string(),
                weight: self.hybrid_weight,
            });
        }

        // Add Summary sub-queries
        for query in &self.summary_queries {
            sub_queries.push(SubQueryRequest {
                text: query.clone(),
                query_type: "summary".to_string(),
                weight: self.summary_weight,
            });
        }

        if sub_queries.is_empty() {
            anyhow::bail!(
                "at least one sub-query is required (--bm25, --vector, --hybrid or --summary)"
            );
        }

        // Parse filters
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

        let request = AggregatedSearchRequest {
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
        };

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

        Ok(())
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

    // Print code snippet (first 3 lines)
    let lines: Vec<&str> = item.code_chunk.lines().take(3).collect();
    for line in lines {
        println!("   {}", line);
    }

    println!();
}
