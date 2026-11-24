//! Read-only graph tools for the MCP surface.
//!
//! `graph_module` exposes one file's module relations (imports, exports,
//! caller files) and `graph_ego` the bounded neighborhood of one entity.
//! Both delegate to the shared graph service the HTTP surface uses; tool
//! exposure stays governed by the configured allow-list.

use std::sync::Arc;

use cce_server_shared::context::SharedContext;
use cce_server_shared::services::graph::{
    self, GraphDirectionSpec, GraphFilterSpec, GraphPageSpec,
};
use cce_types::error_codes;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::json;

use super::support::{relation_tool_error, tool_failure};

/// Arguments for the file module relations tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphModuleArgs {
    /// Project ID the file belongs to (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Project-relative file path.
    #[schemars(length(min = 1))]
    pub file: String,
}

/// Arguments for the ego neighborhood tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GraphEgoArgs {
    /// Project ID the entity belongs to (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Stable symbol ID (for example `sym_…`).
    #[schemars(length(min = 1))]
    pub entity_id: String,
    /// Traversal depth (default 1, capped by the project max call depth).
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub depth: Option<usize>,
    /// Direction: `out`/`forward`, `in`/`backward`, or `both` (default).
    #[serde(default)]
    #[schemars(extend("enum" = ["out", "forward", "in", "backward", "both"]))]
    pub direction: Option<String>,
    /// Pagination offset (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Pagination limit (default 500).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Comma-separated relation domains (call, dependency, structural, ...).
    #[serde(default)]
    pub domains: Option<String>,
    /// Comma-separated exact relation types.
    #[serde(default)]
    pub relation_types: Option<String>,
    /// Whether to include edges pointing outside the project (default true).
    #[serde(default)]
    pub include_external: Option<bool>,
}

/// Run the file module relations tool call.
pub async fn graph_module(
    state: &Arc<SharedContext>,
    args: GraphModuleArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match graph::module(state, args.project_id, &args.file).await {
        Ok(view) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "file": view.file,
                "exports": view.exports,
                "caller_files": view.caller_files,
                "imports": view.imports.iter().map(|edge| json!({
                    "entity_id": edge.entity_id,
                    "target": edge.target,
                    "relation": edge.relation,
                    "domain": edge.domain,
                })).collect::<Vec<_>>(),
                "relation_epoch": view.stamp.relation_epoch,
                "relation_info": view.stamp.stale,
            })
            .to_string(),
        )])),
        Err(error) => relation_tool_error("graph_module", &error),
    }
}

/// Run the ego neighborhood tool call.
pub async fn graph_ego(
    state: &Arc<SharedContext>,
    args: GraphEgoArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let direction = match args.direction.as_deref() {
        None => GraphDirectionSpec::Both,
        Some(raw) => match GraphDirectionSpec::parse(raw) {
            Some(direction) => direction,
            None => {
                return Ok(tool_failure(
                    error_codes::INVALID_INPUT,
                    "direction must be one of out, forward, in, backward, both".to_string(),
                ));
            }
        },
    };
    let filter = GraphFilterSpec {
        domains: split_list(args.domains.as_deref()),
        relation_types: split_list(args.relation_types.as_deref()),
        include_external: args.include_external.unwrap_or(true),
    };
    let page = GraphPageSpec {
        offset: args.offset.unwrap_or(0),
        limit: args.limit.unwrap_or(500),
    };
    match graph::ego(
        state,
        args.project_id,
        &args.entity_id,
        args.depth.unwrap_or(1),
        direction,
        &filter,
        page,
    )
    .await
    {
        Ok(view) => {
            let payload = json!({
                "nodes": view.nodes,
                "edges": view.edges,
                "total_nodes": view.total_nodes,
                "total_edges": view.total_edges,
                "truncated": view.truncated(),
                "relation_epoch": view.stamp.relation_epoch,
                "relation_info": view.stamp.stale,
            });
            Ok(CallToolResult::success(vec![ContentBlock::text(
                payload.to_string(),
            )]))
        }
        Err(error) => relation_tool_error("graph_ego", &error),
    }
}

/// Split a comma-separated wire list into trimmed non-empty items.
fn split_list(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
