//! Index command handlers

use anyhow::Result;

use crate::cli::IndexCommands;
use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_success};
use cce_api::models::{
    DeadLetterRetryResponse, IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest,
    IndexResponse, ParseRequest, ParseResponse,
};

/// Index run parameters
struct IndexRunParams<'a> {
    project_id: i64,
    path: &'a str,
    extensions: &'a str,
    exclude: &'a str,
    gitignore: bool,
    custom_gitignore: &'a Option<String>,
}

pub async fn execute(cmd: &IndexCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        IndexCommands::Run {
            project_id,
            path,
            extensions,
            exclude,
            gitignore,
            custom_gitignore,
        } => {
            let params = IndexRunParams {
                project_id: *project_id,
                path,
                extensions,
                exclude,
                gitignore: *gitignore,
                custom_gitignore,
            };
            run_index(&client, &params, verbose).await
        }
        IndexCommands::Incremental {
            project_id,
            add,
            remove,
            force,
        } => incremental_index(&client, *project_id, add, remove, *force, verbose).await,
        IndexCommands::Parse { file } => parse_file(&client, file, verbose).await,
        IndexCommands::RetryDeadLetter { project_id } => {
            retry_dead_letter(&client, *project_id, verbose).await
        }
    }
}

async fn retry_dead_letter(client: &ApiClient, project_id: i64, verbose: bool) -> Result<()> {
    if verbose {
        println!(
            "Running dead-letter truncate-retry for project {}...",
            project_id
        );
    }

    let url = format!("/api/project/{}/dead-letter/retry", project_id);
    let empty_body = serde_json::json!({});
    let response: DeadLetterRetryResponse = client.post(&url, &empty_body).await?;

    if response.success {
        print_success(&response.message);
        println!(
            "  retried={} succeeded={} still_failed={}",
            response.retried, response.succeeded, response.still_failed
        );
    } else {
        print_error(&response.message);
    }
    Ok(())
}

async fn run_index(client: &ApiClient, params: &IndexRunParams<'_>, verbose: bool) -> Result<()> {
    let ext_list: Vec<String> = params
        .extensions
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let exclude_list: Vec<String> = params
        .exclude
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let request = IndexRequest {
        project_id: params.project_id,
        path: params.path.to_string(),
        extensions: ext_list,
        exclude_dirs: exclude_list,
        respect_gitignore: params.gitignore,
        ignore_patterns: Vec::new(),
        custom_gitignore: params.custom_gitignore.clone(),
    };

    if verbose {
        println!("Indexing directory: {}", params.path);
    }

    let response: IndexResponse = client.post("/api/index", &request).await?;

    if response.success {
        println!();
        print_success(&response.message);
        println!();
        println!("  Files scanned:   {}", response.files_scanned);
        println!("  Files indexed:   {}", response.files_indexed);
        println!("  Failed files:    {}", response.failed_files);
        println!("  Degraded files:  {}", response.degraded_files);
        println!("  Skipped (perm):  {}", response.skipped_permanent);
        println!("  Total entities:  {}", response.total_entities);
        println!("  Total relations: {}", response.total_relations);
        println!("  Total vectors:   {}", response.total_vectors);
        if response.circuit_open {
            print_error("Backend circuit breaker opened during this run");
        }
        println!(
            "  Elapsed time:    {}",
            format_duration(response.elapsed_ms)
        );

        if !response.errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &response.errors {
                print_error(error);
            }
        }
    } else {
        print_error(&response.message);
        println!("  Failed files:    {}", response.failed_files);
        println!("  Degraded files:  {}", response.degraded_files);
        println!("  Skipped (perm):  {}", response.skipped_permanent);
        if response.circuit_open {
            print_error("Backend circuit breaker opened during this run");
        }
        for error in &response.errors {
            print_error(error);
        }
    }

    Ok(())
}

async fn incremental_index(
    client: &ApiClient,
    project_id: i64,
    add: &Option<String>,
    remove: &Option<String>,
    force: bool,
    verbose: bool,
) -> Result<()> {
    let files_to_index: Vec<String> = add
        .as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let files_to_remove: Vec<String> = remove
        .as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let request = IncrementalIndexRequest {
        project_id,
        files_to_index,
        files_to_remove,
        force_reindex: force,
    };

    if verbose {
        println!("Executing incremental index...");
    }

    let response: IncrementalIndexResponse =
        client.post("/api/index/incremental", &request).await?;

    if response.success {
        print_success("Incremental index completed");
        println!();
        println!("  Files indexed:   {}", response.files_indexed);
        println!("  Files removed:   {}", response.files_removed);
        println!("  Total entities:  {}", response.total_entities);
        println!("  Total vectors:   {}", response.total_vectors);
        println!(
            "  Elapsed time:    {}",
            format_duration(response.elapsed_ms)
        );

        if !response.errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &response.errors {
                print_error(error);
            }
        }
    } else {
        print_error("Incremental index failed");
    }

    Ok(())
}

async fn parse_file(client: &ApiClient, file: &str, verbose: bool) -> Result<()> {
    let request = ParseRequest {
        file_path: file.to_string(),
    };

    if verbose {
        println!("Parsing file: {}", file);
    }

    let response: ParseResponse = client.post("/api/parse", &request).await?;

    if response.success {
        print_success(&format!("Parsed {} successfully", response.file_path));
        println!();
        println!("  Language:  {}", response.language);
        println!("  Encoding:  {}", response.encoding);
        println!("  Entities:  {}", response.entities.len());
        println!("  Relations: {}", response.relations.len());
        println!("  Elapsed:   {}", format_duration(response.elapsed_ms));

        if !response.entities.is_empty() {
            println!();
            println!("Entities:");
            for entity in &response.entities {
                println!(
                    "  [{:>4}] {:<10} {} (L{}-L{})",
                    entity.id, entity.kind, entity.name, entity.start_line, entity.end_line
                );
            }
        }
    } else {
        print_error("Parse failed");
    }

    Ok(())
}
