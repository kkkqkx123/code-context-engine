//! Graph command handlers

use anyhow::Result;

use crate::cli::GraphCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_output, print_success};
use cce_api::models::{
    GraphComponentsResponse, GraphImpactResponse, GraphPathResponse, GraphSubgraphResponse,
};

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
            project_id,
        } => get_ego(&client, *project_id, id, *depth, direction, verbose, format).await,
        GraphCommands::Path {
            from,
            to,
            depth,
            project_id,
        } => get_path(&client, *project_id, from, to, *depth, verbose, format).await,
        GraphCommands::Subgraph { ids, project_id } => {
            get_subgraph(&client, *project_id, ids, verbose, format).await
        }
        GraphCommands::Components { project_id } => {
            get_components(&client, *project_id, verbose, format).await
        }
        GraphCommands::Export { limit, project_id } => {
            export_graph(&client, *project_id, *limit, verbose, format).await
        }
        GraphCommands::Impact { file, project_id } => {
            get_impact(&client, *project_id, file, verbose, format).await
        }
    }
}

fn print_subgraph(response: &GraphSubgraphResponse) {
    print_success(&format!(
        "Graph: {} nodes, {} edges (epoch {})",
        response.nodes.len(),
        response.edges.len(),
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
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Fetching ego graph: {id}");
    }
    let path = format!(
        "/api/project/{project_id}/graph/ego?entity_id={id}&depth={depth}&direction={direction}"
    );
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
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
    from: &str,
    to: &str,
    depth: usize,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Finding graph path: {from} -> {to}");
    }
    let path =
        format!("/api/project/{project_id}/graph/path?start={from}&end={to}&max_depth={depth}");
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
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Fetching subgraph: {ids}");
    }
    let path = format!("/api/project/{project_id}/graph/subgraph?ids={ids}");
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
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
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Fetching graph components");
    }
    let path = format!("/api/project/{project_id}/graph/components");
    let response: GraphComponentsResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
    } else if response.success {
        print_success(&format!(
            "{} components (epoch {})",
            response.components.len(),
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
    limit: usize,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    if verbose {
        println!("Exporting graph (limit {limit})");
    }
    let path = format!("/api/project/{project_id}/graph/export?limit={limit}");
    let response: GraphSubgraphResponse = client.get(&path).await?;
    if matches!(format, crate::cli::OutputFormat::Json) {
        print_output(format, &response);
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
        for dependent in &response.transitive_dependents {
            println!("  transitive: {dependent}");
        }
    } else {
        print_error("Graph impact query failed");
    }
    Ok(())
}
