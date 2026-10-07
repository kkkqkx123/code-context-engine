//! Graph command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{print_error, print_output, print_success};
use cce_api::models::{
    GraphComponentsResponse, GraphCyclesResponse, GraphEntityImpactResponse, GraphImpactResponse,
    GraphModuleResponse, GraphPathResponse, GraphStructuralResponse, GraphSubgraphResponse,
};

struct GraphQueryParams {
    offset: usize,
    limit: usize,
    domains: String,
    include_external: bool,
}

struct OutputParams {
    verbose: bool,
    format: crate::cli::OutputFormat,
}

/// The two endpoints of a path query.
struct PathSeeds<'a> {
    from: &'a str,
    to: &'a str,
}

pub async fn execute(
    cmd: &GraphCommands,
    server: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        GraphCommands::Ego {
            id,
            depth,
            direction,
            offset,
            limit,
            domains,
            include_external,
            project_id,
        } => {
            let params = GraphQueryParams {
                offset: *offset,
                limit: *limit,
                domains: domains.clone(),
                include_external: *include_external,
            };
            let output = OutputParams { verbose, format };
            get_ego(
                &client,
                *project_id,
                id,
                *depth,
                direction,
                &params,
                &output,
            )
            .await
        }
        GraphCommands::Path {
            from,
            to,
            depth,
            domains,
            include_external,
            project_id,
        } => {
            let params = GraphQueryParams {
                offset: 0,
                limit: 0,
                domains: domains.clone(),
                include_external: *include_external,
            };
            let output = OutputParams { verbose, format };
            get_path(
                &client,
                *project_id,
                PathSeeds { from, to },
                *depth,
                &params,
                &output,
            )
            .await
        }
        GraphCommands::Subgraph {
            ids,
            offset,
            limit,
            domains,
            include_external,
            project_id,
        } => {
            let params = GraphQueryParams {
                offset: *offset,
                limit: *limit,
                domains: domains.clone(),
                include_external: *include_external,
            };
            let output = OutputParams { verbose, format };
            get_subgraph(&client, *project_id, ids, &params, &output).await
        }
        GraphCommands::Components {
            offset,
            limit,
            domains,
            include_external,
            project_id,
        } => {
            let params = GraphQueryParams {
                offset: *offset,
                limit: *limit,
                domains: domains.clone(),
                include_external: *include_external,
            };
            get_components(&client, *project_id, &params, verbose, format).await
        }
        GraphCommands::Export {
            limit,
            offset,
            domains,
            include_external,
            project_id,
        } => {
            let params = GraphQueryParams {
                offset: *offset,
                limit: *limit,
                domains: domains.clone(),
                include_external: *include_external,
            };
            let output = OutputParams { verbose, format };
            export_graph(&client, *project_id, &params, &output).await
        }
        GraphCommands::Impact { file, project_id } => {
            get_impact(&client, *project_id, file, verbose, format).await
        }
        GraphCommands::EntityImpact {
            entity_id,
            max_depth,
            project_id,
        } => get_entity_impact(&client, *project_id, entity_id, *max_depth, verbose, format).await,
        GraphCommands::Cycles {
            level,
            limit,
            project_id,
        } => get_cycles(&client, *project_id, level, *limit, verbose, format).await,
        GraphCommands::Structural {
            entity_id,
            kind,
            direction,
            limit,
            project_id,
        } => {
            let output = OutputParams { verbose, format };
            get_structural(
                &client,
                *project_id,
                entity_id,
                kind,
                direction,
                *limit,
                &output,
            )
            .await
        }
        GraphCommands::Module { file, project_id } => {
            get_module(&client, *project_id, file, verbose, format).await
        }
    }
}

fn print_subgraph(response: &GraphSubgraphResponse) {
    print_success(&format!(
        "Graph: {} nodes (total {}), {} edges (total {}) (epoch {})",
        response.nodes.len(),
        response.total_nodes,
        response.edges.len(),
        response.total_edges,
        response.relation_epoch
    ));
    println!();
    for node in &response.nodes {
        println!(
            "  {} [{}] {} {}",
            node.id, node.kind, node.label, node.source_file
        );
    }
    if !response.edges.is_empty() {
        println!();
        for edge in &response.edges {
            println!(
                "  {} -[{}|{}]-> {}",
                edge.source, edge.relation, edge.confidence, edge.target
            );
        }
    }
}

async fn get_ego(
    client: &ApiClient,
    project_id: i64,
    id: &str,
    depth: usize,
    direction: &str,
    params: &GraphQueryParams,
    output: &OutputParams,
) -> Result<()> {
    if output.verbose {
        println!("Fetching ego graph: {id}");
    }
    let path = format!(
        "/api/project/{project_id}/graph/ego?entity_id={id}&depth={depth}&direction={direction}&offset={}&limit={}&domains={}&include_external={}",
        params.offset, params.limit, params.domains, params.include_external
    );
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(output.format, crate::cli::OutputFormat::Json) {
        print_output(output.format, &response);
    } else if response.success {
        print_subgraph(&response);
    } else {
        print_error("Graph ego query failed");
    }
    Ok(())
}

async fn get_path(
    client: &ApiClient,
    project_id: i64,
    seeds: PathSeeds<'_>,
    depth: usize,
    params: &GraphQueryParams,
    output: &OutputParams,
) -> Result<()> {
    let (verbose, format) = (output.verbose, output.format);
    if verbose {
        println!("Finding graph path: {} -> {}", seeds.from, seeds.to);
    }
    let path = format!(
        "/api/project/{project_id}/graph/path?start={}&end={}&max_depth={depth}&domains={}&include_external={}",
        seeds.from, seeds.to, params.domains, params.include_external
    );
    let response: GraphPathResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        if response.path_found {
            print_success(&format!(
                "Path: {} nodes, {} edges (epoch {})",
                response.nodes.len(),
                response.edges.len(),
                response.relation_epoch
            ));
            for node in &response.nodes {
                println!("  {} [{}] {}", node.id, node.kind, node.label);
            }
        } else {
            print_success("No path found");
        }
    } else {
        print_error("Graph path query failed");
    }
    Ok(())
}

async fn get_subgraph(
    client: &ApiClient,
    project_id: i64,
    ids: &str,
    params: &GraphQueryParams,
    output: &OutputParams,
) -> Result<()> {
    if output.verbose {
        println!("Fetching subgraph: {ids}");
    }
    let path = format!(
        "/api/project/{project_id}/graph/subgraph?ids={ids}&offset={}&limit={}&domains={}&include_external={}",
        params.offset, params.limit, params.domains, params.include_external
    );
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(output.format, crate::cli::OutputFormat::Json) {
        print_output(output.format, &response);
    } else if response.success {
        print_subgraph(&response);
    } else {
        print_error("Graph subgraph query failed");
    }
    Ok(())
}

async fn get_components(
    client: &ApiClient,
    project_id: i64,
    params: &GraphQueryParams,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Fetching graph components");
    }
    let path = format!(
        "/api/project/{project_id}/graph/components?offset={}&limit={}&domains={}&include_external={}",
        params.offset, params.limit, params.domains, params.include_external
    );
    let response: GraphComponentsResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "{} components (total {}, epoch {})",
            response.components.len(),
            response.total_components,
            response.relation_epoch
        ));
        for (i, group) in response.components.iter().enumerate() {
            println!("  component {i}: {} entities", group.len());
        }
    } else {
        print_error("Graph components query failed");
    }
    Ok(())
}

async fn export_graph(
    client: &ApiClient,
    project_id: i64,
    params: &GraphQueryParams,
    output: &OutputParams,
) -> Result<()> {
    if output.verbose {
        println!("Exporting graph (limit {})", params.limit);
    }
    let path = format!(
        "/api/project/{project_id}/graph/export?limit={}&offset={}&domains={}&include_external={}",
        params.limit, params.offset, params.domains, params.include_external
    );
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(output.format, crate::cli::OutputFormat::Json) {
        print_output(output.format, &response);
    } else if response.success {
        print_subgraph(&response);
    } else {
        print_error("Graph export failed");
    }
    Ok(())
}

async fn get_impact(
    client: &ApiClient,
    project_id: i64,
    file: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Analyzing impact: {file}");
    }
    let path = format!("/api/project/{project_id}/graph/impact?file={file}");
    let response: GraphImpactResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "Impact of {} (score {:.1}, epoch {})",
            response.changed_file, response.impact_score, response.relation_epoch
        ));
        for dependent in &response.direct_dependents {
            println!("  direct: {dependent}");
        }
        for dependent in &response.indirect_dependents {
            println!("  indirect: {dependent}");
        }
    } else {
        print_error("Graph impact query failed");
    }
    Ok(())
}

async fn get_entity_impact(
    client: &ApiClient,
    project_id: i64,
    entity_id: &str,
    max_depth: usize,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Analyzing entity impact: {entity_id}");
    }
    let path = format!(
        "/api/project/{project_id}/graph/entity-impact?entity_id={entity_id}&max_depth={max_depth}"
    );
    let response: GraphEntityImpactResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "Impact of {} (score {:.1}, epoch {})",
            response.changed_entity, response.impact_score, response.relation_epoch
        ));
        for dependent in &response.direct_dependents {
            println!("  direct: {dependent}");
        }
        for dependent in &response.indirect_dependents {
            println!("  indirect: {dependent}");
        }
    } else {
        print_error("Graph entity impact query failed");
    }
    Ok(())
}

async fn get_cycles(
    client: &ApiClient,
    project_id: i64,
    level: &str,
    limit: usize,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Finding {level} dependency cycles");
    }
    let path = format!("/api/project/{project_id}/graph/cycles?level={level}&limit={limit}");
    let response: GraphCyclesResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        let truncated = if response.truncated {
            ", truncated"
        } else {
            ""
        };
        print_success(&format!(
            "{} of {} {}-level cycles{truncated} (epoch {})",
            response.cycles.len(),
            response.total_cycles,
            response.level,
            response.relation_epoch
        ));
        for (index, cycle) in response.cycles.iter().enumerate() {
            println!("  cycle {index}: {}", cycle.members.join(" -> "));
        }
    } else {
        print_error("Graph cycle query failed");
    }
    Ok(())
}

async fn get_structural(
    client: &ApiClient,
    project_id: i64,
    entity_id: &str,
    kind: &str,
    direction: &str,
    limit: usize,
    output: &OutputParams,
) -> Result<()> {
    let (verbose, format) = (output.verbose, output.format);
    if verbose {
        println!("Fetching {kind} relations for {entity_id} ({direction})");
    }
    let path = format!(
        "/api/project/{project_id}/graph/structural?entity_id={entity_id}&kind={kind}&direction={direction}&limit={limit}"
    );
    let response: GraphStructuralResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        let truncated = if response.truncated {
            ", truncated"
        } else {
            ""
        };
        print_success(&format!(
            "{} of {} {}{truncated} (epoch {})",
            response.relations.len(),
            response.total_relations,
            response.kind,
            response.relation_epoch
        ));
        for relation in &response.relations {
            println!(
                "  {} [{}] {} {}",
                relation.entity_id, relation.domain, relation.label, relation.source_file
            );
        }
    } else {
        print_error("Graph structural query failed");
    }
    Ok(())
}

async fn get_module(
    client: &ApiClient,
    project_id: i64,
    file: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Fetching module relations for {file}");
    }
    let path = format!("/api/project/{project_id}/graph/module?file={file}");
    let response: GraphModuleResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "{} imports, {} exports, {} caller files (epoch {})",
            response.imports.len(),
            response.exports.len(),
            response.caller_files.len(),
            response.relation_epoch
        ));
        for import in &response.imports {
            println!("  -[{}]-> {}", import.relation, import.target);
        }
        for caller in &response.caller_files {
            println!("  <- {caller}");
        }
    } else {
        print_error("Graph module query failed");
    }
    Ok(())
}

/// Graph traversal commands
#[derive(Subcommand)]
pub enum GraphCommands {
    /// Ego neighborhood of one entity
    Ego {
        /// Stable symbol ID
        id: String,

        /// Traversal depth
        #[arg(long, default_value = "2")]
        depth: usize,

        /// Direction: forward, backward, or both
        #[arg(long, default_value = "both")]
        direction: String,

        /// Pagination offset
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Pagination limit
        #[arg(long, default_value = "2000")]
        limit: usize,

        /// Comma-separated relation domains (call,dependency,structural,reference,template,other)
        #[arg(long, default_value = "")]
        domains: String,

        /// Include external edges
        #[arg(long, default_value = "true")]
        include_external: bool,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Shortest path between two entities
    Path {
        /// Start symbol seed (stable ID, file#name, or bare name)
        #[arg(long)]
        from: String,

        /// End symbol seed (stable ID, file#name, or bare name)
        #[arg(long)]
        to: String,

        /// Maximum search depth
        #[arg(long, default_value = "10")]
        depth: usize,

        /// Comma-separated relation domains (call,dependency,structural,reference,template,other)
        #[arg(long, default_value = "")]
        domains: String,

        /// Include external edges
        #[arg(long, default_value = "true")]
        include_external: bool,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Induced subgraph over explicit entities (comma-separated stable IDs)
    Subgraph {
        /// Comma-separated stable symbol IDs
        ids: String,

        /// Pagination offset
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Pagination limit
        #[arg(long, default_value = "2000")]
        limit: usize,

        /// Comma-separated relation domains
        #[arg(long, default_value = "")]
        domains: String,

        /// Include external edges
        #[arg(long, default_value = "true")]
        include_external: bool,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Connected components over internal edges
    Components {
        /// Pagination offset
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Maximum components
        #[arg(long, default_value = "2000")]
        limit: usize,

        /// Comma-separated relation domains (call,dependency,structural,reference,template,other)
        #[arg(long, default_value = "")]
        domains: String,

        /// Include external edges
        #[arg(long, default_value = "true")]
        include_external: bool,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Full project graph export (node-link JSON)
    Export {
        /// Maximum nodes
        #[arg(long, default_value = "2000")]
        limit: usize,

        /// Pagination offset
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Comma-separated relation domains
        #[arg(long, default_value = "")]
        domains: String,

        /// Include external edges
        #[arg(long, default_value = "true")]
        include_external: bool,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// File change impact analysis
    Impact {
        /// File path
        file: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Entity change impact analysis
    EntityImpact {
        /// Symbol seed (stable ID, file#name, or bare name)
        entity_id: String,

        /// Maximum dependent depth
        #[arg(long, default_value = "10")]
        max_depth: usize,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Dependency cycles in the call graph or file graph
    Cycles {
        /// Cycle level: entity or file
        #[arg(long, default_value = "entity")]
        level: String,

        /// Maximum cycles to report
        #[arg(long, default_value = "100")]
        limit: usize,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Structural and frontend relations of one entity
    Structural {
        /// Symbol seed (stable ID, file#name, or bare name)
        entity_id: String,

        /// Relation family (see API docs for the full list)
        #[arg(long)]
        kind: String,

        /// Direction: out or in
        #[arg(long, default_value = "out")]
        direction: String,

        /// Maximum relations
        #[arg(long, default_value = "200")]
        limit: usize,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Module-level relations of one file
    Module {
        /// File path
        file: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },
}
