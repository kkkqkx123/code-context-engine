//! Entity command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::output::{print_error, print_output, print_success, truncate};
use cce_transport::SharedHttpClient;
use cce_api::models::{
    CallChainResponse, CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    EntitySearchRequest, EntitySearchResponse, FunctionCallersResponse, FunctionCallsResponse,
    FunctionDetailResponse,
};

pub async fn execute_with_client(
    cmd: &EntityCommands,
    client: &SharedHttpClient,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {

    match cmd {
        EntityCommands::Function { id, project_id } => {
            get_function(client, *project_id, id, verbose).await
        }
        EntityCommands::Calls {
            id,
            project_id,
            max_depth,
            offset,
            limit,
            exclude_tests,
            directory_prefix,
            excluded_files,
            domains,
            include_external,
        } => {
            let filter = RelationFilterArgs {
                max_depth: *max_depth,
                offset: *offset,
                limit: *limit,
                exclude_tests: *exclude_tests,
                directory_prefix: directory_prefix.clone(),
                excluded_files: excluded_files.clone(),
                domains: domains.clone(),
                include_external: *include_external,
            };
            get_function_calls(client, *project_id, id, &filter, verbose).await
        }
        EntityCommands::Callers {
            id,
            project_id,
            max_depth,
            offset,
            limit,
            exclude_tests,
            directory_prefix,
            excluded_files,
            domains,
            include_external,
        } => {
            let filter = RelationFilterArgs {
                max_depth: *max_depth,
                offset: *offset,
                limit: *limit,
                exclude_tests: *exclude_tests,
                directory_prefix: directory_prefix.clone(),
                excluded_files: excluded_files.clone(),
                domains: domains.clone(),
                include_external: *include_external,
            };
            get_function_callers(client, *project_id, id, &filter, verbose).await
        }
        EntityCommands::CallChain {
            id,
            direction,
            max_depth,
            offset,
            limit,
            exclude_tests,
            directory_prefix,
            excluded_files,
            domains,
            include_external,
            project_id,
        } => {
            let filter = RelationFilterArgs {
                max_depth: *max_depth,
                offset: *offset,
                limit: *limit,
                exclude_tests: *exclude_tests,
                directory_prefix: directory_prefix.clone(),
                excluded_files: excluded_files.clone(),
                domains: domains.clone(),
                include_external: *include_external,
            };
            get_call_chain(client, *project_id, id, direction, &filter, verbose).await
        }
        EntityCommands::CallPath {
            from,
            to,
            depth,
            project_id,
        } => get_call_path(client, *project_id, from, to, *depth, verbose).await,
        EntityCommands::Inheritance { id, project_id } => {
            get_class_inheritance(client, *project_id, id, verbose).await
        }
        EntityCommands::Implementations { id, project_id } => {
            get_class_implementations(client, *project_id, id, verbose).await
        }
        EntityCommands::Search {
            query,
            project_id,
            project_path,
            limit,
            kind,
        } => {
            let search = EntitySearchParams {
                query,
                project_id: *project_id,
                project_path: project_path.clone(),
                limit: *limit,
                kind,
            };
            search_entities(client, &search, verbose, format).await
        }
    }
}

struct RelationFilterArgs {
    max_depth: usize,
    offset: Option<usize>,
    limit: usize,
    exclude_tests: Option<bool>,
    directory_prefix: Option<String>,
    excluded_files: Option<String>,
    domains: Option<String>,
    include_external: Option<bool>,
}

struct EntitySearchParams<'a> {
    query: &'a str,
    project_id: Option<i64>,
    project_path: Option<String>,
    limit: i64,
    kind: &'a Option<String>,
}

fn relation_filter_query(filter: &RelationFilterArgs) -> String {
    let mut parts = vec![
        format!("max_depth={}", filter.max_depth),
        format!("limit={}", filter.limit),
    ];
    if let Some(offset) = filter.offset {
        parts.push(format!("offset={offset}"));
    }
    if let Some(exclude_tests) = filter.exclude_tests {
        parts.push(format!("exclude_tests={exclude_tests}"));
    }
    if let Some(prefix) = &filter.directory_prefix {
        parts.push(format!("directory_prefix={}", urlencoding::encode(prefix)));
    }
    if let Some(files) = &filter.excluded_files {
        for file in files.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("excluded_files={}", urlencoding::encode(file)));
        }
    }
    if let Some(domains) = &filter.domains {
        parts.push(format!("domains={}", urlencoding::encode(domains)));
    }
    if let Some(include_external) = filter.include_external {
        parts.push(format!("include_external={include_external}"));
    }
    parts.join("&")
}

async fn get_function(
    client: &SharedHttpClient,
    project_id: i64,
    function_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function: {}", function_id);
    }

    let path = cce_api::paths::FUNCTION_DETAIL_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(function_id));
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
    client: &SharedHttpClient,
    project_id: i64,
    function_id: &str,
    filter: &RelationFilterArgs,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function calls: {}", function_id);
    }

    let base = cce_api::paths::FUNCTION_CALLS_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(function_id));
    let path = format!("{}?{}", base, relation_filter_query(filter));
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
    client: &SharedHttpClient,
    project_id: i64,
    function_id: &str,
    filter: &RelationFilterArgs,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching function callers: {}", function_id);
    }

    let base = cce_api::paths::FUNCTION_CALLERS_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(function_id));
    let path = format!("{}?{}", base, relation_filter_query(filter));
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
    client: &SharedHttpClient,
    project_id: i64,
    function_id: &str,
    direction: &str,
    filter: &RelationFilterArgs,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!(
            "Fetching call chain for: {} (direction: {})",
            function_id, direction
        );
    }

    let base = cce_api::paths::CALL_CHAIN_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(function_id));
    let path = format!(
        "{}?direction={}&{}",
        base,
        urlencoding::encode(direction),
        relation_filter_query(filter)
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
    client: &SharedHttpClient,
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

    let base = cce_api::paths::CALL_PATH_TEMPLATE.replace(
        cce_api::paths::PROJECT_ID_PLACEHOLDER,
        &project_id.to_string(),
    );
    let path = format!(
        "{}?start_id={}&end_id={}&max_depth={}",
        base,
        urlencoding::encode(from),
        urlencoding::encode(to),
        depth
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
    client: &SharedHttpClient,
    project_id: i64,
    class_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching class inheritance: {}", class_id);
    }

    let path = cce_api::paths::CLASS_INHERITANCE_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(class_id));
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
    client: &SharedHttpClient,
    project_id: i64,
    class_id: &str,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Fetching class implementations: {}", class_id);
    }

    let path = cce_api::paths::CLASS_IMPLEMENTATIONS_TEMPLATE
        .replace(
            cce_api::paths::PROJECT_ID_PLACEHOLDER,
            &project_id.to_string(),
        )
        .replace("{id}", &urlencoding::encode(class_id));
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
    client: &SharedHttpClient,
    search: &EntitySearchParams<'_>,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let request = EntitySearchRequest {
        query: search.query.to_string(),
        project_id: search.project_id,
        project_path: search.project_path.clone(),
        limit: search.limit,
        kind_filter: search.kind.clone(),
    };

    if verbose {
        println!("Searching entities: {}", search.query);
    }

    let response: EntitySearchResponse =
        client.post(cce_api::paths::ENTITY_SEARCH, &request).await?;

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

/// Entity commands
#[derive(Subcommand)]
pub enum EntityCommands {
    /// Get function details
    Function {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get function calls (callees)
    Calls {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,

        /// Maximum traversal depth
        #[arg(long, default_value = "3")]
        max_depth: usize,

        /// Pagination offset
        #[arg(long)]
        offset: Option<usize>,

        /// Pagination limit
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Exclude entities located in test files
        #[arg(long)]
        exclude_tests: Option<bool>,

        /// Keep only entities under this directory prefix
        #[arg(long)]
        directory_prefix: Option<String>,

        /// Exact file paths to exclude (comma-separated)
        #[arg(long)]
        excluded_files: Option<String>,

        /// Keep only relations in these coarse domains (comma-separated)
        #[arg(long)]
        domains: Option<String>,

        /// Whether relations pointing outside the project are kept
        #[arg(long)]
        include_external: Option<bool>,
    },

    /// Get function callers
    Callers {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,

        /// Maximum traversal depth
        #[arg(long, default_value = "3")]
        max_depth: usize,

        /// Pagination offset
        #[arg(long)]
        offset: Option<usize>,

        /// Pagination limit
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Exclude entities located in test files
        #[arg(long)]
        exclude_tests: Option<bool>,

        /// Keep only entities under this directory prefix
        #[arg(long)]
        directory_prefix: Option<String>,

        /// Exact file paths to exclude (comma-separated)
        #[arg(long)]
        excluded_files: Option<String>,

        /// Keep only relations in these coarse domains (comma-separated)
        #[arg(long)]
        domains: Option<String>,

        /// Whether relations pointing outside the project are kept
        #[arg(long)]
        include_external: Option<bool>,
    },

    /// Get call chain
    CallChain {
        /// Function ID
        id: String,

        /// Direction: up (callers) or down (callees)
        #[arg(short, long, default_value = "down")]
        direction: String,

        /// Maximum traversal depth
        #[arg(long, default_value = "3")]
        max_depth: usize,

        /// Pagination offset
        #[arg(long)]
        offset: Option<usize>,

        /// Pagination limit
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Exclude entities located in test files
        #[arg(long)]
        exclude_tests: Option<bool>,

        /// Keep only entities under this directory prefix
        #[arg(long)]
        directory_prefix: Option<String>,

        /// Exact file paths to exclude (comma-separated)
        #[arg(long)]
        excluded_files: Option<String>,

        /// Keep only relations in these coarse domains (comma-separated)
        #[arg(long)]
        domains: Option<String>,

        /// Whether relations pointing outside the project are kept
        #[arg(long)]
        include_external: Option<bool>,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Find call path between two functions
    CallPath {
        /// Start function ID
        #[arg(long)]
        from: String,

        /// End function ID
        #[arg(long)]
        to: String,

        /// Maximum search depth
        #[arg(long, default_value = "10")]
        depth: usize,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get class inheritance
    Inheritance {
        /// Class ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get class implementations
    Implementations {
        /// Class ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Search entities (FTS5 full-text search)
    Search {
        /// Search query
        query: String,

        /// Project ID (optional)
        #[arg(long)]
        project_id: Option<i64>,

        /// Project root path (optional if project-id is provided)
        #[arg(long)]
        project_path: Option<String>,

        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: i64,

        /// Filter by entity kind (optional)
        #[arg(long)]
        kind: Option<String>,
    },
}
