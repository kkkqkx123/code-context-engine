//! File and entity lookup tools for the MCP surface.
//!
//! `read_file` returns bounded, line-numbered source windows of project
//! files; `file_outline` lists the symbol skeleton of one file; and
//! `entity_detail` resolves one stable symbol id to its definition. All
//! three are read-only and budget-bounded so downstream generators can
//! verify prose against source with a limited tool-call budget.

use std::sync::Arc;

use cce_server_shared::context::SharedContext;
use cce_server_shared::services::{relation, tools};
use cce_types::error_codes;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use super::support::{relation_tool_error, tool_failure};

/// Arguments for a bounded project file read.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    /// Project ID the file belongs to (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Project-relative file path.
    #[schemars(length(min = 1))]
    pub path: String,
    /// 1-based first line to return (default 1).
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
    /// Maximum lines to return (default 200, hard cap 2000).
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub limit: Option<usize>,
}

/// Arguments for a single-file symbol outline.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileOutlineArgs {
    /// Project ID the file belongs to (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Project-relative file path.
    #[schemars(length(min = 1))]
    pub path: String,
}

/// Arguments for entity detail lookup by stable symbol id.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntityDetailArgs {
    /// Project ID the entity belongs to (required).
    #[schemars(range(min = 1))]
    pub project_id: i64,
    /// Stable symbol ID (for example `sym_…`).
    #[schemars(length(min = 1))]
    pub entity_id: String,
}

/// Run the bounded project file read.
pub async fn read_file(
    state: &Arc<SharedContext>,
    args: ReadFileArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match tools::read_file(state, args.project_id, &args.path, args.offset, args.limit).await {
        Ok(view) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "path": view.path,
                "start_line": view.start_line,
                "end_line": view.end_line,
                "total_lines": view.total_lines,
                "lines": view.lines,
                "truncated": view.truncated,
            })
            .to_string(),
        )])),
        Err(tools::ToolsServiceError::InvalidInput(message)) => {
            Ok(tool_failure(error_codes::INVALID_INPUT, message))
        }
        Err(tools::ToolsServiceError::Execution(message)) => {
            Ok(tool_failure(error_codes::QUERY_ERROR, message))
        }
    }
}

/// Run the single-file symbol outline via the shared symbol tool.
pub async fn file_outline(
    state: &Arc<SharedContext>,
    args: FileOutlineArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_orchestrator::GetSymbolsRequest;

    let answer = tools::get_symbols(
        state,
        args.project_id,
        GetSymbolsRequest {
            paths: vec![args.path.clone()],
        },
    )
    .await;
    match answer.result {
        Ok(result) => {
            let mut payload = json!({
                "path": args.path,
                "result": result,
            });
            if let Some(info) = answer.relation_info {
                payload["relation_info"] = info;
            }
            Ok(CallToolResult::success(vec![ContentBlock::text(
                payload.to_string(),
            )]))
        }
        Err(message) => Ok(tool_failure(error_codes::QUERY_ERROR, message)),
    }
}

/// Run the entity detail lookup by stable symbol id.
pub async fn entity_detail(
    state: &Arc<SharedContext>,
    args: EntityDetailArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match relation::function_detail(state, args.project_id, &args.entity_id).await {
        Ok(detail) => {
            let payload: Value = json!({
                "entity": {
                    "stable_id": detail.entity.stable_id,
                    "name": detail.entity.name,
                    "file_path": detail.entity.file_path,
                },
                "signature": detail.signature,
                "start_line": detail.start_line,
                "end_line": detail.end_line,
                "doc_comment": detail.doc_comment,
                "relation_epoch": detail.stamp.relation_epoch,
                "relation_info": detail.stamp.stale,
            });
            Ok(CallToolResult::success(vec![ContentBlock::text(
                payload.to_string(),
            )]))
        }
        Err(error) => relation_tool_error("entity_detail", &error),
    }
}
