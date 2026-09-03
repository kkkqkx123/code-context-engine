//! Aggregated search command handler

use anyhow::Result;
use clap::Parser;

use crate::client::ApiClient;
use crate::output::{format_duration, format_score, print_error, print_success, truncate};
use cce_api::models::{
    AggregatedSearchRequest, AggregatedSearchResponse, SearchResult, SubQueryRequest,
};

/// Execute multi-query aggregated search
#[derive(Parser, Debug)]
#[command(name = "agg-search", about = "Advanced search with multiple queries")]
pub struct AggSearchCommand {
    /// Project ID for scoping the query (optional)
    #[arg(short, long)]
    pub project_id: Option<i64>,

    /// BM25 keyword queries
    #[arg(long = "bm25")]
    pub bm25_queries: Vec<String>,

    /// Vector semantic queries
    #[arg(long = "vector")]
    pub vector_queries: Vec<String>,

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
                weight: 1.2, // Higher weight for BM25
            });
        }

        // Add Vector sub-queries
        for query in &self.vector_queries {
            sub_queries.push(SubQueryRequest {
                text: query.clone(),
                query_type: "vector".to_string(),
                weight: 1.0,
            });
        }

        if sub_queries.is_empty() {
            anyhow::bail!("至少需要提供一个子查询 (--bm25 或 --vector)");
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

        let request = AggregatedSearchRequest {
            project_id: self.project_id,
            project_path: None,
            sub_queries,
            limit: self.limit,
            min_score: self.min_score,
            directory_prefix: self.directory.clone(),
            exclude_content_types,
            exclude_patterns,
            include_patterns,
            include_categories: vec![],
            exclude_categories: vec![],
            enable_rerank: None,
            rerank_max_candidates: None,
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

        let response: AggregatedSearchResponse = client.search_aggregated(&request).await?;

        if response.success {
            print_success(&format!(
                "Found {} results in {}",
                response.total,
                format_duration(response.elapsed_ms)
            ));

            println!();

            if response.results.is_empty() {
                println!("No results found");
            } else {
                for (i, item) in response.results.iter().enumerate() {
                    print_result_item(i + 1, item);
                }
            }
        } else {
            print_error("Aggregated search failed");
        }

        Ok(())
    }
}

fn print_result_item(index: usize, item: &SearchResult) {
    println!(
        "{}. {} [{}] {}:{}-{}",
        index,
        format_score(item.score),
        item.source,
        truncate(&item.file_path, 50),
        item.start_line,
        item.end_line
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
