//! Tool command handlers

use anyhow::Result;
use colored::Colorize;

use crate::cli::ToolCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{
    CompressRequest, CompressResponse, DiagnoseApiResponse, DiagnoseRequest,
    FindReferencesRequest as FindRefsRequest, FindReferencesResponse as FindRefsApiResponse,
    GetSymbolsRequest as GetSymsRequest, GetSymbolsResponse as GetSymsApiResponse,
    GotoDefinitionRequest as GotoDefRequest, GotoDefinitionResponse as GotoDefApiResponse,
    KeywordSearchRequest, KeywordSearchResponse as KeyWordSearchResponse,
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
        ToolCommands::KeyWordSearch {
            query,
            project_id,
            top_n,
        } => execute_keyword_search(*project_id, query, *top_n, None, server, verbose).await,
    }
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

    let response: CompressResponse = client.post("/api/tools/compress", &request).await?;

    if response.success {
        print_success(&format!(
            "Code compressed: {} -> {} bytes ({:.1}% reduction)",
            response.original_size,
            response.compressed_size,
            (1.0 - response.ratio) * 100.0
        ));
        println!();
        println!("Compressed code:");
        println!("{}", response.compressed);
    } else {
        print_error("Failed to compress code");
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

    let response: DiagnoseApiResponse = client.post("/api/tools/diagnose", &request).await?;

    if response.success {
        if response.issues.is_empty() {
            print_success("No issues found");
        } else {
            println!("Found {} issue(s):", response.issues.len());
            println!();

            for issue in &response.issues {
                let severity = match issue.severity.as_str() {
                    "error" => "ERROR".red(),
                    "warning" => "WARN".yellow(),
                    "info" => "INFO".blue(),
                    _ => issue.severity.normal(),
                };

                let location = issue
                    .line
                    .map(|l| format!(" (line {})", l))
                    .unwrap_or_default();

                println!("[{}] {}{}", severity, issue.message, location);

                if let Some(ref suggestion) = issue.suggestion {
                    println!("  Suggestion: {}", suggestion);
                }
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

    let request = GetSymsRequest {
        project_id,
        paths: paths.to_vec(),
    };

    if verbose {
        println!("Getting symbols...");
    }

    let response: GetSymsApiResponse = client.post("/api/tools/symbols", &request).await?;

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
) -> Result<()> {
    let client = ApiClient::new(&params.server)?;

    let request = FindRefsRequest {
        project_id: params.project_id,
        path: params.path,
        line: params.line,
        column: params.column,
        symbol: params.symbol,
        context_lines,
        include_snippet: None,
        include_entity_info: None,
    };

    if params.verbose {
        println!("Finding references...");
    }

    let response: FindRefsApiResponse = client.post("/api/tools/references", &request).await?;

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

    let request = GotoDefRequest {
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

    let response: GotoDefApiResponse = client.post("/api/tools/definition", &request).await?;

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
    _file_paths: Option<&[String]>,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let request = KeywordSearchRequest {
        query: query.to_string(),
        top_n,
        project_id,
        epoch: None,
    };

    if verbose {
        println!("Keyword searching: {}", query);
    }

    let response: KeyWordSearchResponse =
        client.post("/api/tools/keyword-search", &request).await?;

    if response.success {
        if let Some(data) = &response.data {
            print_success(&format!("Found {} results", data.total));
            println!();

            if data.results.is_empty() {
                println!("No results found");
            } else {
                for (i, item) in data.results.iter().enumerate() {
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
            println!("No data returned");
        }
    } else {
        let err = response.error.as_deref().unwrap_or("Unknown error");
        print_error(&format!("Keyword search failed: {}", err));
    }

    Ok(())
}
