//! Search command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_output, print_success, truncate};
use cce_api::models::{SearchRequest, SearchResponse, SearchResultItem};

/// Search query parameters
struct SearchQueryParams<'a> {
    project_id: i64,
    project_path: Option<&'a str>,
    query: &'a str,
    query_type: &'a str,
    limit: usize,
    min_score: Option<f32>,
    directory: &'a Option<String>,
    exclude_content_types: &'a Option<String>,
    exclude: &'a Option<String>,
    include: &'a Option<String>,
    enable_rerank: Option<bool>,
    rerank_max_candidates: Option<usize>,
}

pub async fn execute(
    cmd: &SearchCommands,
    server: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        SearchCommands::Query {
            project_id,
            project_path,
            query,
            query_type,
            limit,
            min_score,
            directory,
            exclude_content_types,
            exclude,
            include,
            enable_rerank,
            rerank_max_candidates,
        } => {
            let params = SearchQueryParams {
                project_id: *project_id,
                project_path: project_path.as_deref(),
                query,
                query_type,
                limit: *limit,
                min_score: *min_score,
                directory,
                exclude_content_types,
                exclude,
                include,
                enable_rerank: *enable_rerank,
                rerank_max_candidates: *rerank_max_candidates,
            };
            search_query(&client, &params, params.project_id, verbose, format).await
        }
    }
}

async fn search_query(
    client: &ApiClient,
    params: &SearchQueryParams<'_>,
    project_id: i64,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let exclude_patterns: Vec<String> = params
        .exclude
        .as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let include_patterns: Vec<String> = params
        .include
        .as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let exclude_content_types: Vec<String> = params
        .exclude_content_types
        .as_ref()
        .map(|s| s.split(',').map(|ct| ct.trim().to_string()).collect())
        .unwrap_or_default();

    let request = SearchRequest {
        project_id: Some(project_id),
        project_path: params.project_path.map(|s| s.to_string()),
        query: params.query.to_string(),
        query_type: params.query_type.to_string(),
        limit: params.limit,
        min_score: params.min_score,
        directory_prefix: params.directory.clone(),
        exclude_content_types,
        exclude_patterns,
        include_patterns,
        include_categories: vec![],
        exclude_categories: vec![],
        enable_rerank: params.enable_rerank,
        rerank_max_candidates: params.rerank_max_candidates,
    };

    if verbose {
        println!("Searching: {}", params.query);
        println!("Type: {}", params.query_type);
    }

    let response: SearchResponse = client.post("/api/search", &request).await?;

    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "Found {} results in {}",
            response.total,
            format_duration(response.elapsed_ms)
        ));

        if !response.sources_used.is_empty() {
            println!("Sources: {}", response.sources_used.join(", "));
        }

        println!();

        if response.items.is_empty() {
            println!("No results found");
        } else {
            for (i, item) in response.items.iter().enumerate() {
                print_result_item(i + 1, item);
            }
        }
    } else {
        print_error("Search failed");
    }

    Ok(())
}

fn print_result_item(index: usize, item: &SearchResultItem) {
    let entity_names = if item.entity_names.is_empty() {
        String::new()
    } else {
        format!("[{}] ", item.entity_names.join(", "))
    };
    println!(
        "{index}.{names} {path} {start}-{end}",
        index = index,
        names = entity_names,
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

/// Search commands
#[derive(Subcommand)]
pub enum SearchCommands {
    /// Search code
    Query {
        /// Project ID (required)
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Project root path (optional if --project-id is provided)
        #[arg(long)]
        project_path: Option<String>,

        /// Search query
        #[arg(short, long)]
        query: String,

        /// Query type: vector, bm25, hybrid, summary
        #[arg(short = 't', long, default_value = "hybrid")]
        query_type: String,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Minimum score threshold
        #[arg(long)]
        min_score: Option<f32>,

        /// Filter by directory prefix
        #[arg(long)]
        directory: Option<String>,

        /// Content types to exclude (comma-separated): test, generated, vendor
        #[arg(long)]
        exclude_content_types: Option<String>,

        /// Exclude patterns (comma-separated)
        #[arg(long)]
        exclude: Option<String>,

        /// Include patterns (comma-separated)
        #[arg(long)]
        include: Option<String>,

        /// Force reranking on/off for this query (defaults to config)
        #[arg(long)]
        enable_rerank: Option<bool>,

        /// Override the maximum number of rerank candidates
        #[arg(long)]
        rerank_max_candidates: Option<usize>,
    },
}
