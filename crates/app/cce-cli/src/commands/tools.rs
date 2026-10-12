//! Tool command handlers

use clap::Subcommand;

use anyhow::Result;
use colored::Colorize;

use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{
    BatchFoldRequest, BatchFoldResponse, CompressApiResponse, CompressRequest, DiagnoseApiResponse,
    DiagnoseRequest, FindReferencesRequest, FindReferencesResponse, FoldRequest, FoldResponse,
    GetSymbolsRequest, GetSymbolsResponse, GotoDefinitionRequest, GotoDefinitionResponse,
    KeywordSearchApiResponse, KeywordSearchRequest, KeywordTermOperator,
};

pub(crate) struct LocationParams {
    project_id: i64,
    path: String,
    line: usize,
    column: Option<usize>,
    symbol: Option<String>,
    server: String,
    verbose: bool,
}

pub async fn execute(cmd: &ToolCommands, server: &str, verbose: bool) -> Result<()> {
    match cmd {
        ToolCommands::Compress {
            file_path,
            include_entities,
            include_groups,
            project_id,
        } => {
            execute_compress(
                *project_id,
                file_path,
                *include_entities,
                *include_groups,
                server,
                verbose,
            )
            .await
        }
        ToolCommands::BatchCompress {
            file_paths,
            include_entities,
            include_groups,
            max_concurrency,
        } => {
            crate::commands::batch_compress::execute(
                file_paths,
                *include_entities,
                *include_groups,
                max_concurrency.unwrap_or(4),
                server,
                verbose,
            )
            .await
        }
        ToolCommands::Diagnose {
            code,
            language,
            file_name,
            include_ast,
        } => {
            execute_diagnose(
                code,
                language.as_deref(),
                file_name.as_deref(),
                *include_ast,
                server,
                verbose,
            )
            .await
        }
        ToolCommands::Symbols { paths, project_id } => {
            execute_get_symbols(*project_id, paths, server, verbose).await
        }
        ToolCommands::References {
            path,
            line,
            column,
            symbol,
            context_lines,
            include_snippet,
            include_entity_info,
            project_id,
        } => {
            execute_find_references(
                LocationParams {
                    project_id: *project_id,
                    path: path.clone(),
                    line: *line,
                    column: *column,
                    symbol: symbol.clone(),
                    server: server.to_string(),
                    verbose,
                },
                *context_lines,
                *include_snippet,
                *include_entity_info,
            )
            .await
        }
        ToolCommands::Definition {
            path,
            line,
            column,
            symbol,
            include_body,
            project_id,
        } => {
            execute_goto_definition(
                LocationParams {
                    project_id: *project_id,
                    path: path.clone(),
                    line: *line,
                    column: *column,
                    symbol: symbol.clone(),
                    server: server.to_string(),
                    verbose,
                },
                *include_body,
            )
            .await
        }
        ToolCommands::KeywordSearch {
            query,
            project_id,
            top_n,
            offset,
            term_operator,
        } => {
            execute_keyword_search(
                *project_id,
                query,
                *top_n,
                *offset,
                Some(term_operator.as_str()),
                server,
                verbose,
            )
            .await
        }
        ToolCommands::Fold {
            text,
            language,
            file_name,
            max_tokens,
            mode,
        } => {
            execute_fold(
                text,
                language.as_deref(),
                file_name.as_deref(),
                *max_tokens,
                mode.as_deref(),
                server,
                verbose,
            )
            .await
        }
        ToolCommands::FoldBatch {
            items,
            languages,
            file_names,
            item_max_tokens,
            item_modes,
            language,
            max_tokens,
            mode,
            max_concurrency,
        } => {
            execute_fold_batch(
                &FoldBatchCliParams {
                    items,
                    languages,
                    file_names,
                    item_max_tokens,
                    item_modes,
                    language,
                    max_tokens,
                    mode,
                    max_concurrency: *max_concurrency,
                },
                server,
                verbose,
            )
            .await
        }
    }
}

pub(crate) struct FoldBatchCliParams<'a> {
    items: &'a [String],
    languages: &'a [String],
    file_names: &'a [String],
    item_max_tokens: &'a [usize],
    item_modes: &'a [String],
    language: &'a Option<String>,
    max_tokens: &'a Option<usize>,
    mode: &'a Option<String>,
    max_concurrency: Option<usize>,
}

pub async fn execute_compress(
    _project_id: i64,
    file_path: &str,
    include_entities: bool,
    include_groups: bool,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let request = CompressRequest {
        file_path: file_path.to_string(),
        include_entities,
        include_groups,
    };

    if verbose {
        println!("Compressing code...");
    }

    let response: CompressApiResponse = client
        .post(cce_api::paths::TOOLS_COMPRESS, &request)
        .await?;

    match (response.success, response.result) {
        (true, Some(result)) => {
            print_success(&format!(
                "Code compressed: {} ({}, {} entities requested)",
                result.file_path,
                result.language,
                if result.entities.is_some() {
                    "with"
                } else {
                    "without"
                }
            ));
            if result.from_cache {
                println!("(served from cache)");
            }
            println!();
            println!("{}", result.semantic_text);
        }
        _ => {
            let err = response
                .error
                .as_deref()
                .unwrap_or("Failed to compress code");
            print_error(err);
        }
    }

    Ok(())
}

pub async fn execute_diagnose(
    code: &str,
    language: Option<&str>,
    file_name: Option<&str>,
    include_ast: bool,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let request = DiagnoseRequest {
        code: code.to_string(),
        language: language.map(|s| s.to_string()),
        file_name: file_name.map(|s| s.to_string()),
        include_ast,
    };

    if verbose {
        println!("Diagnosing code...");
    }

    let response: DiagnoseApiResponse = client
        .post(cce_api::paths::TOOLS_DIAGNOSE, &request)
        .await?;

    if let (true, Some(result)) = (response.success, response.result) {
        if result.diagnostics.is_empty() {
            print_success(&format!("{}: no issues found", result.language));
        } else {
            println!("Found {} issue(s):", result.diagnostics.len());
            println!();

            for diagnostic in &result.diagnostics {
                println!(
                    "[{}] {} (line {}, col {}) [{}]",
                    diagnostic.kind.red(),
                    diagnostic.message,
                    diagnostic.position.row,
                    diagnostic.position.column,
                    diagnostic.precision,
                );
            }
        }
    } else {
        let err = response
            .error
            .as_deref()
            .unwrap_or("Failed to diagnose code");
        print_error(err);
    }

    Ok(())
}

pub async fn execute_get_symbols(
    project_id: i64,
    paths: &[String],
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let request = GetSymbolsRequest {
        project_id,
        paths: paths.to_vec(),
    };

    if verbose {
        println!("Getting symbols...");
    }

    let response: GetSymbolsResponse = client.post(cce_api::paths::TOOLS_SYMBOLS, &request).await?;

    if response.success {
        let Some(result) = response.result.as_ref() else {
            print_error("No data returned");
            return Ok(());
        };

        print_success(&format!(
            "Processed {} files ({} ok, {} failed)",
            result.results.len(),
            result.success_count,
            result.fail_count
        ));
        println!();

        let mut total_symbols = 0;
        for file in &result.results {
            let Some(symbols) = file.symbols.as_ref() else {
                continue;
            };
            total_symbols += symbols.len();
            println!("{} ({} symbols)", file.path, symbols.len());

            for symbol in symbols {
                let sig = symbol
                    .detail
                    .as_ref()
                    .map(|s| format!(": {}", s))
                    .unwrap_or_default();
                println!(
                    "  [{:>5}] {:<15} {}{}",
                    symbol.line, symbol.kind, symbol.name, sig
                );
            }
        }

        if total_symbols == 0 {
            println!("No symbols found");
        }
    } else {
        let err = response.error.as_deref().unwrap_or("Failed to get symbols");
        print_error(err);
    }

    Ok(())
}

pub async fn execute_find_references(
    params: LocationParams,
    context_lines: Option<usize>,
    include_snippet: Option<bool>,
    include_entity_info: Option<bool>,
) -> Result<()> {
    let client = ApiClient::new(&params.server)?;

    let request = FindReferencesRequest {
        project_id: params.project_id,
        path: params.path,
        line: params.line,
        column: params.column,
        symbol: params.symbol,
        context_lines,
        include_snippet,
        include_entity_info,
    };

    if params.verbose {
        println!("Finding references...");
    }

    let response: FindReferencesResponse = client
        .post(cce_api::paths::TOOLS_REFERENCES, &request)
        .await?;

    if response.success {
        let Some(result) = response.result.as_ref() else {
            print_error("No data returned");
            return Ok(());
        };

        print_success(&format!(
            "Found {} references in {} files",
            result.total_count, result.file_count
        ));
        println!();

        if result.total_count == 0 {
            println!("No references found");
        } else {
            for group in &result.references {
                println!("{} ({} references)", group.path, group.count);
                for reference in &group.references {
                    println!(
                        "  {}:{}:{}",
                        reference.path, reference.line, reference.column
                    );

                    if let Some(ref snippet) = reference.snippet {
                        for line in snippet.lines().take(3) {
                            println!("    {}", line);
                        }
                    }
                }
            }
        }
    } else {
        let err = response
            .error
            .as_deref()
            .unwrap_or("Failed to find references");
        print_error(err);
    }

    Ok(())
}

pub async fn execute_goto_definition(params: LocationParams, include_body: bool) -> Result<()> {
    let client = ApiClient::new(&params.server)?;

    let request = GotoDefinitionRequest {
        project_id: params.project_id,
        path: params.path,
        line: params.line,
        column: params.column,
        symbol: params.symbol,
        include_body,
    };

    if params.verbose {
        println!("Finding definition...");
    }

    let response: GotoDefinitionResponse = client
        .post(cce_api::paths::TOOLS_DEFINITION, &request)
        .await?;

    if response.success {
        let Some(result) = response.result.as_ref() else {
            print_error("No data returned");
            return Ok(());
        };

        if result.definitions.is_empty() {
            println!("Definition not found");
        } else {
            for def in &result.definitions {
                print_success(&format!(
                    "Definition found: {}:{} ({})",
                    def.location.path, def.location.line, def.name
                ));
                println!();

                if !def.signature.is_empty() {
                    println!("Signature: {}", def.signature);
                }
            }
        }
    } else {
        let err = response
            .error
            .as_deref()
            .unwrap_or("Failed to find definition");
        print_error(err);
    }

    Ok(())
}

pub async fn execute_keyword_search(
    project_id: i64,
    query: &str,
    top_n: usize,
    offset: usize,
    term_operator: Option<&str>,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let operator = match term_operator.map(str::to_ascii_lowercase).as_deref() {
        Some("and") => KeywordTermOperator::And,
        _ => KeywordTermOperator::Or,
    };
    let request = KeywordSearchRequest {
        query: query.to_string(),
        top_n,
        project_id,
        epoch: None,
        offset,
        term_operator: operator,
    };

    if verbose {
        println!("Keyword searching: {}", query);
    }

    let response: KeywordSearchApiResponse = client
        .post(cce_api::paths::TOOLS_KEYWORD_SEARCH, &request)
        .await?;

    if let (true, Some(result)) = (response.success, response.result) {
        print_success(&format!("Found {} results", result.total));
        println!();

        if result.results.is_empty() {
            println!("No results found");
        } else {
            for (i, item) in result.results.iter().enumerate() {
                println!(
                    "  {:>3}. {} (score: {:.4})",
                    i + 1,
                    item.file_path,
                    item.score
                );
                println!(
                    "       {} (L{}-L{})",
                    item.title, item.start_line, item.end_line
                );
            }
        }
    } else {
        let err = response.error.as_deref().unwrap_or("Unknown error");
        print_error(&format!("Keyword search failed: {}", err));
    }

    Ok(())
}

pub async fn execute_fold(
    text: &str,
    language: Option<&str>,
    file_name: Option<&str>,
    max_tokens: Option<usize>,
    mode: Option<&str>,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;
    let request = FoldRequest {
        text: text.to_string(),
        language: language.map(str::to_string),
        file_name: file_name.map(str::to_string),
        max_tokens,
        mode: mode.map(str::to_string),
    };
    if verbose {
        println!("Folding file...");
    }
    let response: FoldResponse = client.post(cce_api::paths::TOOLS_FOLD, &request).await?;
    if response.success {
        print_success("Fold completed");
        println!();
        println!("{}", response.folded_text);
    } else {
        print_error("Fold failed");
    }
    Ok(())
}

pub async fn execute_fold_batch(
    params: &FoldBatchCliParams<'_>,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;
    let request_items = params
        .items
        .iter()
        .enumerate()
        .map(|(index, text)| cce_api::models::BatchFoldItem {
            id: index.to_string(),
            text: text.clone(),
            language: params.languages.get(index).cloned(),
            file_name: params.file_names.get(index).cloned(),
            max_tokens: params.item_max_tokens.get(index).copied(),
            mode: params.item_modes.get(index).cloned(),
        })
        .collect();
    let request = BatchFoldRequest {
        items: request_items,
        language: params.language.clone(),
        max_tokens: *params.max_tokens,
        mode: params.mode.clone(),
        max_concurrency: params.max_concurrency,
    };
    if verbose {
        println!("Folding {} files...", params.items.len());
    }
    let response: BatchFoldResponse = client
        .post(cce_api::paths::TOOLS_FOLD_BATCH, &request)
        .await?;
    print_success(&format!("Folded {} files", response.results.len()));
    Ok(())
}

/// Tool commands
#[derive(Subcommand)]
pub enum ToolCommands {
    /// Compress code from file
    Compress {
        /// File path to compress
        #[arg(short, long)]
        file_path: String,

        /// Include entities in compression
        #[arg(long, default_value_t = false)]
        include_entities: bool,

        /// Include groups in compression
        #[arg(long, default_value_t = false)]
        include_groups: bool,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Batch compress multiple files
    BatchCompress {
        /// File paths to compress (multiple --file-path flags)
        #[arg(short, long = "file-path")]
        file_paths: Vec<String>,

        /// Include entities in compression
        #[arg(long, default_value_t = false)]
        include_entities: bool,

        /// Include groups in compression
        #[arg(long, default_value_t = false)]
        include_groups: bool,

        /// Maximum concurrency
        #[arg(long)]
        max_concurrency: Option<usize>,
    },

    /// Diagnose code
    Diagnose {
        /// Code to diagnose
        #[arg(short, long)]
        code: String,

        /// Language
        #[arg(short, long)]
        language: Option<String>,

        /// File name hint
        #[arg(long)]
        file_name: Option<String>,

        /// Include AST in output
        #[arg(long, default_value_t = false)]
        include_ast: bool,
    },

    /// Get symbols from files
    Symbols {
        /// File paths
        #[arg(short, long)]
        paths: Vec<String>,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Find references at a location
    References {
        /// File path
        #[arg(short, long)]
        path: String,

        /// Line number (0-based)
        #[arg(short, long)]
        line: usize,

        /// Column number (optional)
        #[arg(long)]
        column: Option<usize>,

        /// Symbol name (optional, used if file/line/column not provided)
        #[arg(long)]
        symbol: Option<String>,

        /// Number of context lines to show
        #[arg(long)]
        context_lines: Option<usize>,

        /// Include source snippets in output
        #[arg(long)]
        include_snippet: Option<bool>,

        /// Include entity info in output
        #[arg(long)]
        include_entity_info: Option<bool>,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Go to definition
    Definition {
        /// File path
        #[arg(short, long)]
        path: String,

        /// Line number (0-based)
        #[arg(short, long)]
        line: usize,

        /// Column number (optional)
        #[arg(long)]
        column: Option<usize>,

        /// Symbol name (optional, used if file/line/column not provided)
        #[arg(long)]
        symbol: Option<String>,

        /// Include function body in output
        #[arg(long, default_value_t = false)]
        include_body: bool,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Keyword search (BM25-based with raw source snippets)
    KeywordSearch {
        /// Search query
        #[arg(short, long)]
        query: String,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        top_n: usize,

        /// Number of results to skip for pagination
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Term operator combining query terms: or, and
        #[arg(long, default_value = "or")]
        term_operator: String,
    },

    /// Fold one file into a skeleton
    Fold {
        /// Raw file text
        #[arg(short, long)]
        text: String,

        /// Language hint
        #[arg(short, long)]
        language: Option<String>,

        /// File name hint
        #[arg(long)]
        file_name: Option<String>,

        /// Token budget
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Fold mode
        #[arg(long)]
        mode: Option<String>,
    },

    /// Fold multiple files in one batch
    FoldBatch {
        /// Raw file texts (multiple --items flags)
        #[arg(short, long = "items")]
        items: Vec<String>,

        /// Per-item language hints aligned by index with --items
        #[arg(long)]
        languages: Vec<String>,

        /// Per-item file name hints aligned by index with --items
        #[arg(long)]
        file_names: Vec<String>,

        /// Per-item token budgets aligned by index with --items
        #[arg(long)]
        item_max_tokens: Vec<usize>,

        /// Per-item fold modes aligned by index with --items
        #[arg(long)]
        item_modes: Vec<String>,

        /// Global language default for items without an explicit hint
        #[arg(long)]
        language: Option<String>,

        /// Global token budget default
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Global fold mode default
        #[arg(long)]
        mode: Option<String>,

        /// Maximum concurrency (reserved for forward compatibility)
        #[arg(long)]
        max_concurrency: Option<usize>,
    },
}
