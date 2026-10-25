//! Search command handlers

use clap::Subcommand;

use crate::client::CliApi;
use crate::output::truncate;
use cce_api::models::SearchResultItem;

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

/// Execute search commands through the client abstraction.
pub async fn execute_with_client(
    cmd: &SearchCommands,
    client: &cce_transport::SharedHttpClient,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> anyhow::Result<()> {
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
            include_categories,
            exclude_categories,
            enable_rerank,
            rerank_max_candidates,
            query_intent,
            expand,
        } => {
            let exclude_patterns: Vec<String> = exclude
                .as_ref()
                .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
                .unwrap_or_default();
            let include_patterns: Vec<String> = include
                .as_ref()
                .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
                .unwrap_or_default();
            let exclude_ct: Vec<String> = exclude_content_types
                .as_ref()
                .map(|s| s.split(',').map(|ct| ct.trim().to_string()).collect())
                .unwrap_or_default();
            let include_cats: Vec<String> = include_categories
                .as_ref()
                .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
                .unwrap_or_default();
            let exclude_cats: Vec<String> = exclude_categories
                .as_ref()
                .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
                .unwrap_or_default();
            let request = cce_api::models::SearchRequest {
                project_id: Some(*project_id),
                project_path: project_path.clone(),
                query: query.to_string(),
                query_type: query_type.to_string(),
                limit: *limit,
                min_score: *min_score,
                directory_prefix: directory.clone(),
                exclude_content_types: exclude_ct,
                exclude_patterns,
                include_patterns,
                include_categories: include_cats,
                exclude_categories: exclude_cats,
                enable_rerank: *enable_rerank,
                rerank_max_candidates: *rerank_max_candidates,
                query_intent: query_intent.clone(),
                expand: *expand,
            };
            if verbose {
                println!("Searching: {}", query);
                println!("Type: {}", query_type);
            }
            match client.search(request).await {
                Ok(response) => {
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        crate::output::print_output(format, &response);
                    } else if response.success {
                        crate::output::print_success(&format!(
                            "Found {} results in {}",
                            response.total,
                            crate::output::format_duration(response.elapsed_ms)
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
                        crate::output::print_error("Search failed");
                    }
                    Ok(())
                }
                Err(e) => {
                    crate::output::print_error(&format!("Search failed: {e}"));
                    Ok(())
                }
            }
        }
    }
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

        /// Include only specific category values (comma-separated): code, config, documentation, schema, other
        #[arg(long)]
        include_categories: Option<String>,

        /// Exclude specific category values (comma-separated): code, config, documentation, schema, other
        #[arg(long)]
        exclude_categories: Option<String>,

        /// Force reranking on/off for this query (defaults to config)
        #[arg(long)]
        enable_rerank: Option<bool>,

        /// Override the maximum number of rerank candidates
        #[arg(long)]
        rerank_max_candidates: Option<usize>,

        /// Query intent override: semantic, keyword, hybrid, entity (empty means auto-detect)
        #[arg(long)]
        query_intent: Option<String>,

        /// Resolve call-graph neighbours for the top hits
        #[arg(long)]
        expand: Option<bool>,
    },
}
