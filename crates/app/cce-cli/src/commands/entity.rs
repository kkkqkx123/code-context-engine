//! Entity command handlers

use anyhow::Result;

use crate::cli::EntityCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_output, print_success, truncate};
use cce_api::models::{
    CallChainResponse, CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    EntitySearchRequest, EntitySearchResponse, FunctionCallersResponse, FunctionCallsResponse,
    FunctionDetailResponse,
};

pub async fn execute(
    cmd: &EntityCommands,
    server: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        EntityCommands::Function { id, project_id } => {
            get_function(&client, *project_id, id, verbose).await
        }
        EntityCommands::Calls { id, project_id } => {
            get_function_calls(&client, *project_id, id, verbose).await
        }
        EntityCommands::Callers { id, project_id } => {
            get_function_callers(&client, *project_id, id, verbose).await
        }
        EntityCommands::CallChain {
            id,
            direction,
            project_id,
        } => get_call_chain(&client, *project_id, id, direction, verbose).await,
        EntityCommands::CallPath {
            from,
            to,
            depth,
            project_id,
        } => get_call_path(&client, *project_id, from, to, *depth, verbose).await,
        EntityCommands::Inheritance { id, project_id } => {
            get_class_inheritance(&client, *project_id, id, verbose).await
        }
        EntityCommands::Implementations { id, project_id } => {
            get_class_implementations(&client, *project_id, id, verbose).await
        }
        EntityCommands::Search {
            query,
            project_id,
            limit,
            kind,
        } => search_entities(&client, query, *project_id, *limit, kind, verbose, format).await,
    }
}

async fn get_function(
    client: &ApiClient,
    project_id: i64,
    function_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function: {}", function_id);
    }

    let path = format!("/api/project/{}/function/{}", project_id, function_id);
    let response: FunctionDetailResponse = client.get(&path).await?;

    if response.success {
        let func = &response.function;
        print_success(&format!("Function: {}", func.name));
        println!();
        println!("  ID:           {}", func.id);
        println!("  Signature:    {}", func.signature);
        println!("  File:         {}", func.file_path);
        println!("  Lines:        {}-{}", func.start_line, func.end_line);

        if !func.parameters.is_empty() {
            println!("  Parameters:");
            for param in &func.parameters {
                let type_str = param
                    .type_name
                    .as_ref()
                    .map(|t| format!(": {}", t))
                    .unwrap_or_default();
                println!("    - {}{}", param.name, type_str);
            }
        }

        if let Some(ref return_type) = func.return_type {
            println!("  Return type:  {}", return_type);
        }

        if let Some(ref doc) = func.doc_comment {
            println!("  Doc comment:  {}", truncate(doc, 80));
        }
    } else {
        print_error("Failed to get function");
    }

    Ok(())
}

async fn get_function_calls(
    client: &ApiClient,
    project_id: i64,
    function_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function calls: {}", function_id);
    }

    let path = format!("/api/project/{}/function/{}/calls", project_id, function_id);
    let response: FunctionCallsResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!(
            "Function {} calls {} functions",
            response.function_name, response.total_callees
        ));
        println!();

        if response.callees.is_empty() {
            println!("No calls found");
        } else {
            for callee in &response.callees {
                println!(
                    "  -> {} (L{}) [{}]",
                    callee.function_name,
                    callee.call_line.unwrap_or(0),
                    truncate(&callee.file_path, 40)
                );
            }
        }
    } else {
        print_error("Failed to get function calls");
    }

    Ok(())
}

async fn get_function_callers(
    client: &ApiClient,
    project_id: i64,
    function_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function callers: {}", function_id);
    }

    let path = format!(
        "/api/project/{}/function/{}/callers",
        project_id, function_id
    );
    let response: FunctionCallersResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!(
            "Function {} is called by {} functions",
            response.function_name, response.total_callers
        ));
        println!();

        if response.callers.is_empty() {
            println!("No callers found");
        } else {
            for caller in &response.callers {
                println!(
                    "  <- {} (L{}) [{}]",
                    caller.function_name,
                    caller.call_line.unwrap_or(0),
                    truncate(&caller.file_path, 40)
                );
            }
        }
    } else {
        print_error("Failed to get function callers");
    }

    Ok(())
}

async fn get_call_chain(
    client: &ApiClient,
    project_id: i64,
    function_id: &str,
    direction: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!(
            "Fetching call chain for: {} (direction: {})",
            function_id, direction
        );
    }

    let path = format!(
        "/api/project/{}/call-chain/{}?direction={}",
        project_id, function_id, direction
    );
    let response: CallChainResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!(
            "Call chain for {} ({}):",
            response.function_name, response.direction
        ));
        println!();

        if response.call_chain.is_empty() {
            println!("No call chain found");
        } else {
            for node in &response.call_chain {
                let indent = "  ".repeat(node.depth);
                println!(
                    "{}{} {} [{}]",
                    indent,
                    if node.relation_type == "caller" {
                        "<-"
                    } else {
                        "->"
                    },
                    node.function_name,
                    truncate(&node.file_path, 40)
                );
            }
        }
    } else {
        print_error("Failed to get call chain");
    }

    Ok(())
}

async fn get_call_path(
    client: &ApiClient,
    project_id: i64,
    from: &str,
    to: &str,
    depth: usize,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!(
            "Finding call path: {} -> {} (max depth: {})",
            from, to, depth
        );
    }

    let path = format!(
        "/api/project/{}/call-path?start_id={}&end_id={}&max_depth={}",
        project_id, from, to, depth
    );
    let response: CallPathResponse = client.get(&path).await?;

    if response.success {
        if response.path_found {
            print_success(&format!(
                "Path found from {} to {} (length: {})",
                response.start_function_id, response.end_function_id, response.path_length
            ));
            println!();

            for (i, node) in response.path.iter().enumerate() {
                println!(
                    "{}. {} [{}]",
                    i + 1,
                    node.function_name,
                    truncate(&node.file_path, 40)
                );
            }
        } else {
            println!("No path found between {} and {}", from, to);
        }
    } else {
        print_error("Failed to find call path");
    }

    Ok(())
}

async fn get_class_inheritance(
    client: &ApiClient,
    project_id: i64,
    class_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching class inheritance: {}", class_id);
    }

    let path = format!("/api/project/{}/class/{}/inheritance", project_id, class_id);
    let response: ClassInheritanceResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!("Class: {}", response.class_name));
        println!();

        if !response.base_classes.is_empty() {
            println!("Base classes:");
            for base in &response.base_classes {
                println!(
                    "  ^ {} (depth: {}) [{}]",
                    base.class_name,
                    base.depth,
                    truncate(&base.file_path, 40)
                );
            }
        }

        if !response.derived_classes.is_empty() {
            println!("Derived classes:");
            for derived in &response.derived_classes {
                println!(
                    "  v {} (depth: {}) [{}]",
                    derived.class_name,
                    derived.depth,
                    truncate(&derived.file_path, 40)
                );
            }
        }

        if response.base_classes.is_empty() && response.derived_classes.is_empty() {
            println!("No inheritance relationships found");
        }
    } else {
        print_error("Failed to get class inheritance");
    }

    Ok(())
}

async fn get_class_implementations(
    client: &ApiClient,
    project_id: i64,
    class_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching class implementations: {}", class_id);
    }

    let path = format!(
        "/api/project/{}/class/{}/implementations",
        project_id, class_id
    );
    let response: ClassImplementationsResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!("Class: {}", response.class_name));
        println!();

        if !response.implemented_interfaces.is_empty() {
            println!("Implemented interfaces:");
            for iface in &response.implemented_interfaces {
                println!(
                    "  implements {} [{}]",
                    iface.interface_name,
                    truncate(&iface.file_path, 40)
                );
            }
        }

        if !response.implementing_classes.is_empty() {
            println!("Implementing classes:");
            for class in &response.implementing_classes {
                println!(
                    "  implemented by {} [{}]",
                    class.class_name,
                    truncate(&class.file_path, 40)
                );
            }
        }

        if response.implemented_interfaces.is_empty() && response.implementing_classes.is_empty() {
            println!("No implementation relationships found");
        }
    } else {
        print_error("Failed to get class implementations");
    }

    Ok(())
}

async fn search_entities(
    client: &ApiClient,
    query: &str,
    project_id: Option<i64>,
    limit: i64,
    kind: &Option<String>,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let request = EntitySearchRequest {
        query: query.to_string(),
        project_id,
        project_path: None,
        limit,
        kind_filter: kind.clone(),
    };

    if verbose {
        println!("Searching entities: {}", query);
    }

    let response: EntitySearchResponse = client.post("/api/entities/search", &request).await?;

    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!("Found {} entities", response.total));
        println!();

        if response.items.is_empty() {
            println!("No results found");
        } else {
            for (i, item) in response.items.iter().enumerate() {
                let file_info =
                    if let (Some(start), Some(end)) = (item.span_start_row, item.span_end_row) {
                        format!(" L{}-L{}", start, end)
                    } else {
                        String::new()
                    };

                let sig = item
                    .signature
                    .as_ref()
                    .map(|s| format!(" ({})", s))
                    .unwrap_or_default();

                println!(
                    "  {:>3}. [{:<12}] {}{}{}",
                    i + 1,
                    item.kind,
                    item.name,
                    sig,
                    file_info
                );
            }
        }

        println!();
        println!("  Elapsed: {}ms", response.elapsed_ms);
    } else {
        print_error("Failed to search entities");
    }

    Ok(())
}
