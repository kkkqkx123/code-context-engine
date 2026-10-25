//! Index command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_success};
use cce_api::models::{
    DeadLetterAcknowledgeRequest, DeadLetterActionResponse, DeadLetterListResponse,
    DeadLetterRetryRequest, IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest,
    IndexResponse, ParseRequest, ParseResponse,
};

/// Execute index commands through the domain service abstraction.
pub async fn execute_with_service(
    cmd: &IndexCommands,
    service: &crate::service::CliService,
    server: &str,
    verbose: bool,
) -> Result<()> {
    match cmd {
        IndexCommands::Run {
            project_id,
            path,
            extensions,
            exclude,
            ignore_patterns,
            gitignore,
            custom_gitignore,
        } => {
            let ext_list: Vec<String> = extensions
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let exclude_list: Vec<String> =
                exclude.split(',').map(|s| s.trim().to_string()).collect();
            let ignore_list: Vec<String> = ignore_patterns
                .as_ref()
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let request = IndexRequest {
                project_id: *project_id,
                path: path.to_string(),
                extensions: ext_list,
                exclude_dirs: exclude_list,
                respect_gitignore: *gitignore,
                ignore_patterns: ignore_list,
                custom_gitignore: custom_gitignore.clone(),
            };
            if verbose {
                println!("Indexing directory: {}", path);
            }
            match service.index(request).await {
                Ok(response) => {
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
                    }
                }
                Err(e) => print_error(&format!("Index execution failed: {e}")),
            }
            Ok(())
        }
        IndexCommands::Incremental {
            project_id,
            add,
            remove,
            force,
        } => {
            let client = ApiClient::new(server)?;
            incremental_index(&client, *project_id, add, remove, *force, verbose).await
        }
        IndexCommands::Parse { file } => {
            let client = ApiClient::new(server)?;
            parse_file(&client, file, verbose).await
        }
        IndexCommands::RetryDeadLetter { project_id, files } => {
            let client = ApiClient::new(server)?;
            retry_dead_letter(&client, *project_id, files.as_deref(), verbose).await
        }
        IndexCommands::DeadLetters { project_id } => {
            let client = ApiClient::new(server)?;
            list_dead_letters(&client, *project_id, verbose).await
        }
        IndexCommands::AcknowledgeDeadLetter {
            project_id,
            file,
            module,
        } => {
            let client = ApiClient::new(server)?;
            acknowledge_dead_letter(&client, *project_id, file, module.as_deref(), verbose).await
        }
    }
}

async fn retry_dead_letter(
    client: &ApiClient,
    project_id: i64,
    files: Option<&str>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Running dead-letter retry for project {}...", project_id);
    }

    let request = DeadLetterRetryRequest {
        files: files
            .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default(),
    };
    let response: DeadLetterActionResponse = client
        .post(
            &cce_api::paths::project_dead_letters_retry(project_id),
            &request,
        )
        .await?;

    if response.success {
        print_success(&response.message);
        println!("  affected={}", response.affected);
    } else {
        print_error(&response.message);
    }
    Ok(())
}

async fn list_dead_letters(client: &ApiClient, project_id: i64, verbose: bool) -> Result<()> {
    if verbose {
        println!("Listing dead letters for project {}...", project_id);
    }
    let response: DeadLetterListResponse = client
        .get(&cce_api::paths::project_dead_letters(project_id))
        .await?;
    println!("  files={}", response.files.len());
    for file in &response.files {
        println!("  {} (version {})", file.file_path, file.version);
    }
    Ok(())
}

async fn acknowledge_dead_letter(
    client: &ApiClient,
    project_id: i64,
    file: &str,
    module: Option<&str>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Acknowledging dead letter {}...", file);
    }
    let request = DeadLetterAcknowledgeRequest {
        file_path: file.to_string(),
        module: module.map(str::to_string),
    };
    let response: DeadLetterActionResponse = client
        .post(
            &cce_api::paths::project_dead_letters_acknowledge(project_id),
            &request,
        )
        .await?;
    if response.success {
        print_success(&response.message);
    } else {
        print_error(&response.message);
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

    let response: IncrementalIndexResponse = client
        .post(cce_api::paths::INDEX_INCREMENTAL, &request)
        .await?;

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

    let response: ParseResponse = client.post(cce_api::paths::PARSE, &request).await?;

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

/// Index commands
#[derive(Subcommand)]
pub enum IndexCommands {
    /// Execute full index on a directory
    Run {
        /// Project ID (required)
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Root directory to index
        #[arg(short, long)]
        path: String,

        /// File extensions to include (comma-separated)
        #[arg(short, long, default_value = "rs,py,js,ts,c,cpp,java")]
        extensions: String,

        /// Directories to exclude (comma-separated)
        #[arg(short = 'x', long, default_value = "node_modules,target,.git,vendor")]
        exclude: String,

        /// Ignore patterns for files (comma-separated globs)
        #[arg(long)]
        ignore_patterns: Option<String>,

        /// Respect .gitignore
        #[arg(long, default_value = "true")]
        gitignore: bool,

        /// Custom gitignore file path
        #[arg(long)]
        custom_gitignore: Option<String>,
    },

    /// Execute incremental index
    Incremental {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Files to index (comma-separated paths)
        #[arg(short, long)]
        add: Option<String>,

        /// Files to remove (comma-separated paths)
        #[arg(short, long)]
        remove: Option<String>,

        /// Force re-index
        #[arg(short, long)]
        force: bool,
    },

    /// Parse a single file
    Parse {
        /// File path to parse
        #[arg(short, long)]
        file: String,
    },

    /// Run a retry pass over the dead letter queue (empty files means all)
    RetryDeadLetter {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
        /// Restrict retry to these comma-separated files; empty means all
        #[arg(short, long)]
        files: Option<String>,
    },

    /// List dead-letter files of a project
    DeadLetters {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Acknowledge one dead-letter file so it leaves the retry flow
    AcknowledgeDeadLetter {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
        /// File path to acknowledge
        #[arg(short, long)]
        file: String,
        /// Restrict acknowledgement to one module
        #[arg(short, long)]
        module: Option<String>,
    },
}
