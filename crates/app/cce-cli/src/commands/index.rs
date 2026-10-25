//! Index command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::client::CliApi;
use crate::output::{format_duration, print_error, print_success};
use cce_api::models::{
    DeadLetterAcknowledgeRequest, DeadLetterActionResponse, DeadLetterListResponse,
    DeadLetterRetryRequest, IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest,
    ParseRequest, ParseResponse,
};
use cce_transport::SharedHttpClient;

/// Execute index commands through the client abstraction.
pub async fn execute_with_client(
    cmd: &IndexCommands,
    client: &SharedHttpClient,
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
            push,
            no_commit,
            token: _,
            timeout_secs: _,
            compress: _,
        } => {
            if *push {
                anyhow::bail!(
                    "push mode needs a gateway client; call execute_push_with_gateway instead"
                );
            }
            if *no_commit {
                crate::output::print_warning(
                    "--no-commit only applies to --push mode and is ignored here",
                );
            }
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
            match client.index(request).await {
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
        } => incremental_index(client, *project_id, add, remove, *force, verbose).await,
        IndexCommands::Parse { file } => parse_file(client, file, verbose).await,
        IndexCommands::RetryDeadLetter { project_id, files } => {
            retry_dead_letter(client, *project_id, files.as_deref(), verbose).await
        }
        IndexCommands::DeadLetters { project_id } => {
            list_dead_letters(client, *project_id, verbose).await
        }
        IndexCommands::AcknowledgeDeadLetter {
            project_id,
            file,
            module,
        } => acknowledge_dead_letter(client, *project_id, file, module.as_deref(), verbose).await,
    }
}

/// Execute `index run --push` through the ingest path.
///
/// Local scan plus manifest/batch/commit replaces the single blocking
/// server-side request. Each batch is its own HTTP call so large trees make
/// progress instead of holding one 300s request; the shared gateway supply
/// performs the scan, fingerprinting, batching, and commit, and this wrapper
/// only translates index flags onto it.
///
/// Behavior notes: the single-request path lets the server read `path`
/// directly (local deployment), reuse its scan cache, and run checkpoint
/// resume; the push path stages local bytes on the server and then runs the
/// same index pipelines over staged files, including relation graph build on
/// commit. Counts may differ when filters diverge (`custom_gitignore` and
/// `ignore_patterns` have no gateway equivalent and are ignored with a
/// warning), so compare file/entity/vector totals when validating a new
/// deployment.
pub async fn execute_push_with_gateway(
    cmd: &IndexCommands,
    gateway: &cce_gateway::GatewayClient,
    verbose: bool,
) -> Result<()> {
    let IndexCommands::Run {
        project_id,
        path,
        extensions,
        exclude,
        gitignore,
        no_commit,
        compress,
        custom_gitignore,
        ignore_patterns,
        ..
    } = cmd
    else {
        anyhow::bail!("push mode only supports `index run`");
    };
    if custom_gitignore.is_some() {
        crate::output::print_warning(
            "custom_gitignore has no gateway equivalent and is ignored in push mode",
        );
    }
    if ignore_patterns.is_some() {
        crate::output::print_warning(
            "ignore_patterns has no gateway equivalent and is ignored in push mode",
        );
    }
    let mut params = cce_gateway::SyncParams::with_defaults(*project_id, path.clone());
    params.apply_overrides(cce_gateway::SyncParamOverrides {
        extensions: Some(extensions.clone()),
        exclude: Some(exclude.clone()),
        gitignore: Some(*gitignore),
        commit: Some(!*no_commit),
        compress: *compress,
        ..Default::default()
    });
    cce_gateway::sync_once(gateway, &params, verbose)
        .await
        .map(|_| ())
}

async fn retry_dead_letter(
    client: &SharedHttpClient,
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

async fn list_dead_letters(
    client: &SharedHttpClient,
    project_id: i64,
    verbose: bool,
) -> Result<()> {
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
    client: &SharedHttpClient,
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
    client: &SharedHttpClient,
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

async fn parse_file(client: &SharedHttpClient, file: &str, verbose: bool) -> Result<()> {
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

        /// Push file contents via ingest (manifest/batch/commit) instead of
        /// a single server-side path request. Use for remote servers or
        /// large trees needing per-batch progress.
        #[arg(long, default_value = "false")]
        push: bool,

        /// Stage files without running the index commit (push mode only)
        #[arg(long, default_value = "false")]
        no_commit: bool,

        /// Admission token; explicit value outranks CCE_API_TOKEN (push mode)
        #[arg(long, env = "CCE_API_TOKEN")]
        token: Option<String>,

        /// Per-command HTTP timeout in seconds for the single-request path
        #[arg(long)]
        timeout_secs: Option<u64>,

        /// Compress chunks before upload (push mode)
        #[arg(long)]
        compress: Option<bool>,
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
