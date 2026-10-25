//! Graph command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::output::{print_error, print_output, print_success};
use cce_api::models::GraphSubgraphResponse;

/// Execute graph commands through the client abstraction.
///
/// Output rendering stays command-local; transport encoding lives in the
/// client's typed domain methods.
pub async fn execute_with_client(
    cmd: &GraphCommands,
    client: &crate::client::ApiClient,
    _server: &str,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    use cce_api::service_params::{
        GraphComponentsParams, GraphCyclesParams, GraphEgoParams, GraphEntityImpactParams,
        GraphExportParams, GraphImpactParams, GraphModuleParams, GraphPathParams,
        GraphStructuralParams, GraphSubgraphParams,
    };
    match cmd {
        GraphCommands::Ego {
            id,
            depth,
            direction,
            offset,
            limit,
            domains,
            relation_types,
            include_external,
            project_id,
        } => {
            if verbose {
                println!("Fetching ego graph: {id}");
            }
            match client
                .graph_ego(
                    *project_id,
                    GraphEgoParams {
                        entity_id: id.clone(),
                        depth: *depth,
                        direction: direction.clone(),
                        offset: *offset,
                        limit: *limit,
                        domains: domains.clone(),
                        relation_types: relation_types.clone(),
                        include_external: *include_external,
                    },
                )
                .await
            {
                Ok(response) => {
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        print_output(format, &response);
                    } else if response.success {
                        print_subgraph(&response);
                    } else {
                        print_error("Graph ego query failed");
                    }
                }
                Err(e) => print_error(&format!("Graph ego query failed: {e}")),
            }
        }
        GraphCommands::Path {
            from,
            to,
            depth,
            domains,
            relation_types,
            include_external,
            project_id,
        } => {
            if verbose {
                println!("Finding graph path: {from} -> {to}");
            }
            match client
                .graph_path(
                    *project_id,
                    GraphPathParams {
                        start: from.clone(),
                        end: to.clone(),
                        max_depth: *depth,
                        domains: domains.clone(),
                        relation_types: relation_types.clone(),
                        include_external: *include_external,
                    },
                )
                .await
            {
                Ok(response) => {
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
                }
                Err(e) => print_error(&format!("Graph path query failed: {e}")),
            }
        }
        GraphCommands::Subgraph {
            ids,
            offset,
            limit,
            domains,
            relation_types,
            include_external,
            project_id,
        } => {
            if verbose {
                println!("Fetching subgraph: {ids}");
            }
            let seeds = ids
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect();
            match client
                .graph_subgraph(
                    *project_id,
                    GraphSubgraphParams {
                        ids: seeds,
                        offset: *offset,
                        limit: *limit,
                        domains: domains.clone(),
                        relation_types: relation_types.clone(),
                        include_external: *include_external,
                    },
                )
                .await
            {
                Ok(response) => {
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        print_output(format, &response);
                    } else if response.success {
                        print_subgraph(&response);
                    } else {
                        print_error("Graph subgraph query failed");
                    }
                }
                Err(e) => print_error(&format!("Graph subgraph query failed: {e}")),
            }
        }
        GraphCommands::Components {
            offset,
            limit,
            domains,
            relation_types,
            include_external,
            project_id,
        } => {
            if verbose {
                println!("Fetching graph components");
            }
            match client
                .graph_components(
                    *project_id,
                    GraphComponentsParams {
                        offset: *offset,
                        limit: *limit,
                        domains: domains.clone(),
                        relation_types: relation_types.clone(),
                        include_external: *include_external,
                    },
                )
                .await
            {
                Ok(response) => {
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        print_output(format, &response);
                    } else if response.success {
                        print_success(&format!(
                            "{} components (total {}, epoch {})",
                            response.components.len(),
                            response.total_components,
                            response.relation_epoch
                        ));
                        for (index, group) in response.components.iter().enumerate() {
                            println!("  component {index}: {} entities", group.len());
                        }
                    } else {
                        print_error("Graph components query failed");
                    }
                }
                Err(e) => print_error(&format!("Graph components query failed: {e}")),
            }
        }
        GraphCommands::Export {
            limit,
            offset,
            domains,
            relation_types,
            include_external,
            project_id,
        } => {
            if verbose {
                println!("Exporting graph (limit {limit})");
            }
            match client
                .graph_export(
                    *project_id,
                    GraphExportParams {
                        limit: *limit,
                        offset: *offset,
                        domains: domains.clone(),
                        relation_types: relation_types.clone(),
                        include_external: *include_external,
                    },
                )
                .await
            {
                Ok(response) => {
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        print_output(format, &response);
                    } else if response.success {
                        print_subgraph(&response);
                    } else {
                        print_error("Graph export failed");
                    }
                }
                Err(e) => print_error(&format!("Graph export failed: {e}")),
            }
        }
        GraphCommands::Impact { file, project_id } => {
            if verbose {
                println!("Analyzing impact: {file}");
            }
            match client
                .graph_impact(*project_id, GraphImpactParams { file: file.clone() })
                .await
            {
                Ok(response) => {
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
                }
                Err(e) => print_error(&format!("Graph impact query failed: {e}")),
            }
        }
        GraphCommands::EntityImpact {
            entity_id,
            max_depth,
            scope,
            project_id,
        } => {
            if verbose {
                println!("Analyzing entity impact: {entity_id}");
            }
            match client
                .graph_entity_impact(
                    *project_id,
                    GraphEntityImpactParams {
                        entity_id: entity_id.clone(),
                        max_depth: *max_depth,
                        scope: scope.clone(),
                    },
                )
                .await
            {
                Ok(response) => {
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
                }
                Err(e) => print_error(&format!("Graph entity impact query failed: {e}")),
            }
        }
        GraphCommands::Cycles {
            level,
            limit,
            project_id,
        } => {
            if verbose {
                println!("Finding {level} dependency cycles");
            }
            match client
                .graph_cycles(
                    *project_id,
                    GraphCyclesParams {
                        level: level.clone(),
                        limit: *limit,
                    },
                )
                .await
            {
                Ok(response) => {
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
                }
                Err(e) => print_error(&format!("Graph cycle query failed: {e}")),
            }
        }
        GraphCommands::Structural {
            entity_id,
            kind,
            direction,
            limit,
            project_id,
        } => {
            if verbose {
                println!("Fetching {kind} relations for {entity_id} ({direction})");
            }
            match client
                .graph_structural(
                    *project_id,
                    GraphStructuralParams {
                        entity_id: entity_id.clone(),
                        kind: kind.clone(),
                        direction: direction.clone(),
                        limit: *limit,
                    },
                )
                .await
            {
                Ok(response) => {
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
                                relation.entity_id,
                                relation.domain,
                                relation.label,
                                relation.source_file
                            );
                        }
                    } else {
                        print_error("Graph structural query failed");
                    }
                }
                Err(e) => print_error(&format!("Graph structural query failed: {e}")),
            }
        }
        GraphCommands::Module { file, project_id } => {
            if verbose {
                println!("Fetching module relations for {file}");
            }
            match client
                .graph_module(*project_id, GraphModuleParams { file: file.clone() })
                .await
            {
                Ok(response) => {
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
                }
                Err(e) => print_error(&format!("Graph module query failed: {e}")),
            }
        }
    }
    Ok(())
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

        /// Direction: in, out, or both
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

        /// Comma-separated relation types (e.g. calls,imports,extends)
        #[arg(long, default_value = "")]
        relation_types: String,

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

        /// Comma-separated relation types
        #[arg(long, default_value = "")]
        relation_types: String,

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

        /// Comma-separated relation types
        #[arg(long, default_value = "")]
        relation_types: String,

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

        /// Comma-separated relation types
        #[arg(long, default_value = "")]
        relation_types: String,

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

        /// Comma-separated relation types
        #[arg(long, default_value = "")]
        relation_types: String,

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

        /// Impact scope: entity or file
        #[arg(long, default_value = "entity")]
        scope: String,

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
