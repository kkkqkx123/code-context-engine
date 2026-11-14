//! Entity relation tools (callees, callers, call chain) for the MCP surface.
//!
//! All three tools delegate to the shared relation service, so the call-domain
//! filter, seed resolution and pagination are the same ones the HTTP surface
//! uses.

use cce_server_shared::context::SharedContext;
use cce_server_shared::services::relation::{self, CallDirection, RelationFilterSpec};
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::support::{tool_error, tool_failure};

/// Arguments identifying an entity by ID.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntityArgs {
    /// Project ID the entity belongs to (required).
    pub project_id: i64,
    /// Stable symbol ID (for example `sym_…`).
    pub entity_id: String,
    /// Pagination offset for callees/callers listing (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Pagination limit for callees/callers listing.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments for call-chain traversal.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CallChainArgs {
    /// Project ID the entity belongs to (required).
    pub project_id: i64,
    /// Stable symbol ID (for example `sym_…`).
    pub entity_id: String,
    /// Traversal direction: "forward"/"down" (callees, default) or
    /// "backward"/"up"/"callers" (callers).
    #[serde(default)]
    pub direction: Option<String>,
    /// Maximum traversal depth (default 3).
    #[serde(default)]
    pub max_depth: Option<usize>,
    /// Pagination offset into the chain nodes (default 0).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Pagination limit for chain nodes. Defaults to the full chain.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Run the entity callees tool call.
pub async fn entity_callees(
    state: &Arc<SharedContext>,
    args: EntityArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let options = RelationFilterSpec::default().apply(
        relation::call_domain_options()
            .with_offset(args.offset.unwrap_or(0))
            .with_limit(args.limit.unwrap_or(20)),
    );
    let edges = match relation::callees(state, args.project_id, &args.entity_id, &options).await {
        Ok(edges) => edges,
        Err(error) => return tool_error("callees", error),
    };
    let items: Vec<Value> = edges
        .items
        .iter()
        .map(|edge| {
            json!({
                "caller": edge.caller.stable_id,
                "callee_id": edge.callee.as_ref().map(|entity| entity.stable_id.clone()),
                "callee_name": edge.target_name,
                "relation_type": edge.relation_type,
                "is_external": edge.is_external,
                "start_line": edge.start_line,
            })
        })
        .collect();
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({
            "entity_id": args.entity_id,
            "total": edges.total,
            "callees": items,
        })
        .to_string(),
    )]))
}

/// Run the entity callers tool call.
pub async fn entity_callers(
    state: &Arc<SharedContext>,
    args: EntityArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let options = RelationFilterSpec::default().apply(
        relation::call_domain_options()
            .with_offset(args.offset.unwrap_or(0))
            .with_limit(args.limit.unwrap_or(20)),
    );
    let edges = match relation::callers(state, args.project_id, &args.entity_id, &options).await {
        Ok(edges) => edges,
        Err(error) => return tool_error("callers", error),
    };
    let items: Vec<Value> = edges
        .items
        .iter()
        .map(|edge| {
            json!({
                "caller": edge.caller.stable_id,
                "caller_name": edge.caller.name,
                "relation_type": edge.relation_type,
                "is_external": edge.is_external,
            })
        })
        .collect();
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({
            "entity_id": args.entity_id,
            "total": edges.total,
            "callers": items,
        })
        .to_string(),
    )]))
}

/// Run the call chain traversal tool call.
pub async fn call_chain(
    state: &Arc<SharedContext>,
    args: CallChainArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let direction = match args.direction.as_deref() {
        None => CallDirection::Forward,
        Some(raw) => match CallDirection::parse(raw) {
            Some(direction) => direction,
            None => {
                return Ok(tool_failure(format!(
                    "call_chain failed: direction must be one of down, forward, up, or backward, got '{raw}'"
                )));
            }
        },
    };
    let max_depth = args.max_depth.unwrap_or(3);
    let options = RelationFilterSpec::default().apply(
        relation::call_domain_options()
            .with_max_depth(max_depth)
            .with_offset(args.offset.unwrap_or(0))
            .with_limit(args.limit.unwrap_or(usize::MAX)),
    );
    let page =
        match relation::call_chain(state, args.project_id, &args.entity_id, direction, options)
            .await
        {
            Ok(page) => page,
            Err(error) => return tool_error("call_chain", error),
        };
    let items: Vec<Value> = page
        .items
        .iter()
        .map(|node| {
            json!({
                "function_id": node.entity.stable_id,
                "function_name": node.entity.name,
                "file_path": node.entity.file_path,
                "depth": node.depth,
                "relation_type": node.relation_type,
                "call_line": node.call_line,
            })
        })
        .collect();
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({
            "entity_id": args.entity_id,
            "direction": direction.label(),
            "max_depth": max_depth,
            "offset": args.offset.unwrap_or(0),
            "total": page.total,
            "nodes": items,
        })
        .to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_chain_args_default_to_forward_depth_three() {
        let args: CallChainArgs =
            serde_json::from_value(serde_json::json!({"project_id": 1, "entity_id": "sym_a"}))
                .expect("minimal args deserialize");
        assert!(args.direction.is_none());
        assert_eq!(args.max_depth.unwrap_or(3), 3);
    }
}
