//! Entity relation tools (callees, callers, call chain) for the MCP surface.
//!
//! All three tools operate on the shared relation runtime through the same
//! call-domain filter: only call edges qualify, and unresolved external
//! targets are excluded.

use cce_server_shared::state::AppState;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::support::{call_domain_relation_options, canonical_call_direction, resolve_stable_id};

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
    /// "backward"/"up" (callers). The HTTP surface uses down/up while MCP
    /// historically used forward/backward; all four spellings are accepted.
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
    state: &Arc<AppState>,
    args: EntityArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let searcher = match state.get_relation_searcher(args.project_id).await {
        Ok(searcher) => searcher,
        Err(error) => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "relation runtime unavailable: {error}"
            ))]));
        }
    };
    let entity_id = match resolve_stable_id(&searcher, &args.entity_id) {
        Some(id) => id,
        None => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "unknown stable symbol ID: {}",
                args.entity_id
            ))]));
        }
    };
    let options = call_domain_relation_options()
        .with_offset(args.offset.unwrap_or(0))
        .with_limit(args.limit.unwrap_or(20));
    let callees = searcher.get_callees_paginated(entity_id, &options);
    let items: Vec<Value> = callees
        .iter()
        .map(|relation| {
            json!({
                "caller": relation.caller.to_string(),
                "callee_id": relation.callee_id.map(|id| id.to_string()),
                "callee_name": relation.callee_name,
                "relation_type": relation.relation_type.to_string(),
                "is_external": relation.is_external,
                "start_line": relation.span.start_position.row,
            })
        })
        .collect();
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({ "entity_id": args.entity_id, "callees": items }).to_string(),
    )]))
}

/// Run the entity callers tool call.
pub async fn entity_callers(
    state: &Arc<AppState>,
    args: EntityArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let searcher = match state.get_relation_searcher(args.project_id).await {
        Ok(searcher) => searcher,
        Err(error) => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "relation runtime unavailable: {error}"
            ))]));
        }
    };
    let entity_id = match resolve_stable_id(&searcher, &args.entity_id) {
        Some(id) => id,
        None => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "unknown stable symbol ID: {}",
                args.entity_id
            ))]));
        }
    };
    let options = call_domain_relation_options()
        .with_offset(args.offset.unwrap_or(0))
        .with_limit(args.limit.unwrap_or(20));
    let callers = searcher.get_callers_paginated(entity_id, &options);
    let items: Vec<Value> = callers
        .iter()
        .map(|relation| {
            json!({
                "caller": relation.caller.to_string(),
                "relation_type": relation.relation_type.to_string(),
                "is_external": relation.is_external,
            })
        })
        .collect();
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({ "entity_id": args.entity_id, "callers": items }).to_string(),
    )]))
}

/// Run the call chain traversal tool call.
pub async fn call_chain(
    state: &Arc<AppState>,
    args: CallChainArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let searcher = match state.get_relation_searcher(args.project_id).await {
        Ok(searcher) => searcher,
        Err(error) => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "relation runtime unavailable: {error}"
            ))]));
        }
    };
    let entity_id = match resolve_stable_id(&searcher, &args.entity_id) {
        Some(id) => id,
        None => {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "unknown stable symbol ID: {}",
                args.entity_id
            ))]));
        }
    };
    let max_depth = args.max_depth.unwrap_or(3);
    let options = call_domain_relation_options()
        .with_max_depth(max_depth)
        .with_offset(args.offset.unwrap_or(0))
        .with_limit(args.limit.unwrap_or(usize::MAX));
    let (canonical, backward) = canonical_call_direction(args.direction.as_deref());
    let result = if backward {
        searcher.query_backward_paginated(entity_id, &options)
    } else {
        searcher.query_forward_paginated(entity_id, &options)
    };
    match result {
        Ok(nodes) => {
            let items: Vec<Value> = nodes
                .iter()
                .map(|node| {
                    json!({
                        "function_id": node.function_id.to_string(),
                        "function_name": node.function_name,
                        "file_path": node.file_path,
                        "depth": node.depth,
                        "relation_type": node.relation_type.to_string(),
                        "call_line": node.call_line,
                    })
                })
                .collect();
            Ok(CallToolResult::success(vec![ContentBlock::text(
                json!({
                    "entity_id": args.entity_id,
                    "direction": canonical,
                    "max_depth": max_depth,
                    "offset": args.offset.unwrap_or(0),
                    "nodes": items,
                })
                .to_string(),
            )]))
        }
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "call_chain failed: {error}"
        ))])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_direction_canonicalizes_all_spellings() {
        assert_eq!(canonical_call_direction(None), ("forward", false));
        assert_eq!(
            canonical_call_direction(Some("forward")),
            ("forward", false)
        );
        assert_eq!(canonical_call_direction(Some("down")), ("forward", false));
        assert_eq!(
            canonical_call_direction(Some("backward")),
            ("backward", true)
        );
        assert_eq!(canonical_call_direction(Some("up")), ("backward", true));
        assert_eq!(
            canonical_call_direction(Some("callers")),
            ("backward", true)
        );
        assert_eq!(
            canonical_call_direction(Some("FORWARD")),
            ("forward", false)
        );
        assert_eq!(
            canonical_call_direction(Some("sideways")),
            ("forward", false)
        );
    }
}
