//! Search command handlers

use anyhow::Result;

use crate::cli::SearchCommands;
use crate::client::ApiClient;
use crate::output::{
    format_duration, format_score, print_error, print_output, print_success, truncate,
};
use cce_api::models::{SearchRequest, SearchResponse, SearchResultItem};

/// Search query parameters
struct SearchQueryParams<'a> {
    project_id: i64,
    project_path: Option<&'a str>,
    query: &'a str,
    query_type: &'a str,
    limit: usize,
    min_score: Option<f32>,
    extensions: &'a Option<String>,
    directory: &'a Option<String>,
    entities: &'a Option<String>,
    languages: &'a Option<String>,
    exclude_content_types: &'a Option<String>,
    exclude: &'a Option<String>,
    include: &'a Option<String>,
    call_chain_depth: Option<usize>,
    include_call_chain: bool,
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
            extensions,
            directory,
            entities,
            languages,
            exclude_content_types,
            exclude,
            include,
            call_chain_depth,
            include_call_chain,
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
                extensions,
                directory,
                entities,
                languages,
                exclude_content_types,
                exclude,
                include,
                call_chain_depth: *call_chain_depth,
                include_call_chain: *include_call_chain,
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
    let file_extensions: Vec<String> = params
        .extensions
        .as_ref()
        .map(|s| s.split(',').map(|e| e.trim().to_string()).collect())
        .unwrap_or_default();

    let entity_types: Vec<String> = params
        .entities
        .as_ref()
        .map(|s| s.split(',').map(|e| e.trim().to_string()).collect())
        .unwrap_or_default();

    let language_list: Vec<String> = params
        .languages
        .as_ref()
        .map(|s| s.split(',').map(|l| l.trim().to_string()).collect())
        .unwrap_or_default();

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
        file_extensions,
        entity_types,
        languages: language_list,
        exclude_content_types,
        exclude_patterns,
        include_patterns,
        include_categories: vec![],
        exclude_categories: vec![],
        call_chain_depth: params.call_chain_depth,
        include_call_chain: params.include_call_chain,
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
    let entity_ids = if item.entity_ids.is_empty() {
        String::new()
    } else {
        let list = item
            .entity_ids
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("[entity_ids:{}] ", list)
    };
    println!(
        "{}. {}{} {} {}:{}-{}",
        index,
        entity_ids,
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

    // Print call chain if available
    if let Some(ref call_chain) = item.call_chain {
        if !call_chain.is_empty() {
            println!("   Call chain:");
            for node in call_chain {
                println!(
                    "     -> {} ({})",
                    node.function_name,
                    truncate(&node.file_path, 40)
                );
            }
        }
    }

    println!();
}
