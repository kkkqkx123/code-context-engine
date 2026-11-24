//! MCP-facing budget plumbing: `_meta` parsing and `_budget` injection.
//!
//! The counting core lives in `cce-server-shared::services::budget` with no
//! `rmcp` dependency. This module is the only place that knows about the MCP
//! wire format: it extracts `wiki_run` from `tools/call` `_meta` (primary) or
//! from common top-level arguments (fallback), and writes budget metadata back
//! into the tool payload.

use cce_server_shared::services::budget::{BudgetExhausted, BudgetState, WikiRunContext};
use rmcp::model::{CallToolResult, ContentBlock, JsonObject};
use serde_json::{Map, Value, json};

use crate::handler::support::tool_failure;

/// `_meta` key carrying the downstream run context.
pub const WIKI_RUN_META_KEY: &str = "wiki_run";

/// Common argument keys accepted on the fallback path.
const RUN_ID_ARG: &str = "run_id";
const PAGE_ID_ARG: &str = "page_id";
const EPOCH_ARG: &str = "snapshot_epoch";

/// Parse a run context from a `wiki_run` JSON value.
fn parse_context(value: &Value) -> Result<WikiRunContext, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "wiki_run must be a JSON object".to_string())?;
    let string_field = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(format!("wiki_run.{key} must be a string")),
        }
    };
    let run_id =
        string_field("run_id")?.ok_or_else(|| "wiki_run.run_id is required".to_string())?;
    let page_id = string_field("page_id")?;
    let snapshot_epoch =
        match object.get("snapshot_epoch") {
            None | Some(Value::Null) => None,
            Some(Value::Number(number)) => Some(number.as_u64().ok_or_else(|| {
                "wiki_run.snapshot_epoch must be a non-negative integer".to_string()
            })?),
            Some(_) => {
                return Err("wiki_run.snapshot_epoch must be a non-negative integer".to_string());
            }
        };
    let context = WikiRunContext {
        run_id,
        page_id,
        snapshot_epoch,
    };
    context.validate().map_err(str::to_string)?;
    Ok(context)
}

/// Extract a run context from `_meta`, falling back to common top-level args.
///
/// Returns `Ok(None)` when neither path carries a run context, which means an
/// ordinary one-off call with no budget tracking (behavior unchanged).
pub fn parse_run_context(
    meta: Option<&JsonObject>,
    arguments: Option<&JsonObject>,
) -> Result<Option<WikiRunContext>, String> {
    if let Some(meta) = meta
        && let Some(value) = meta.get(WIKI_RUN_META_KEY)
    {
        return parse_context(value).map(Some);
    }
    if let Some(args) = arguments
        && args.contains_key(RUN_ID_ARG)
    {
        let mut probe = Map::new();
        probe.insert(
            "run_id".to_string(),
            args.get(RUN_ID_ARG).cloned().unwrap_or(Value::Null),
        );
        if let Some(page) = args.get(PAGE_ID_ARG) {
            probe.insert("page_id".to_string(), page.clone());
        }
        if let Some(epoch) = args.get(EPOCH_ARG) {
            probe.insert("snapshot_epoch".to_string(), epoch.clone());
        }
        return parse_context(&Value::Object(probe)).map(Some);
    }
    Ok(None)
}

/// Build the `_budget` metadata object attached to a successful payload.
pub fn budget_metadata(state: &BudgetState) -> Value {
    json!({
        "run_id": state.run_id,
        "calls_used": state.calls_used,
        "calls_remaining": state.calls_remaining,
        "page_calls_used": state.page_calls_used,
        "page_calls_remaining": state.page_calls_remaining,
        "snapshot_epoch": state.snapshot_epoch,
        "cached": state.cached,
    })
}

/// Inject `_budget` at the top level of a tool payload.
///
/// Tool payloads are a single JSON text block. Parse it, add the `_budget`
/// field, and re-serialize. Non-JSON payloads are returned unchanged so this
/// never breaks an opaque tool response.
pub fn inject_budget(mut result: CallToolResult, state: &BudgetState) -> CallToolResult {
    let metadata = budget_metadata(state);
    if let Some(ContentBlock::Text(text)) = result.content.first_mut()
        && let Ok(mut payload) = serde_json::from_str::<Value>(&text.text)
        && let Some(object) = payload.as_object_mut()
    {
        object.insert("_budget".to_string(), metadata);
        text.text = payload.to_string();
    }
    result
}

/// Render a budget refusal as the shared structured `isError` channel.
pub fn budget_exhausted_result(exhausted: &BudgetExhausted) -> CallToolResult {
    tool_failure(
        cce_types::error_codes::BUDGET_EXHAUSTED,
        format!(
            "tool call budget exhausted ({:?}); switch to wcp_only fallback",
            exhausted.scope
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_with(value: Value) -> JsonObject {
        let mut meta = JsonObject::new();
        meta.insert(WIKI_RUN_META_KEY.to_string(), value);
        meta
    }

    #[test]
    fn parses_run_context_from_meta() {
        let meta = meta_with(json!({
            "run_id": "run-1",
            "page_id": "module-a",
            "snapshot_epoch": 42
        }));
        let ctx = parse_run_context(Some(&meta), None)
            .expect("parse")
            .expect("context present");
        assert_eq!(ctx.run_id, "run-1");
        assert_eq!(ctx.page_id.as_deref(), Some("module-a"));
        assert_eq!(ctx.snapshot_epoch, Some(42));
    }

    #[test]
    fn fallback_reads_common_arguments() {
        let mut args = JsonObject::new();
        args.insert("run_id".to_string(), json!("run-2"));
        args.insert("page_id".to_string(), json!("p"));
        let ctx = parse_run_context(None, Some(&args))
            .expect("parse")
            .expect("context present");
        assert_eq!(ctx.run_id, "run-2");
        assert_eq!(ctx.page_id.as_deref(), Some("p"));
    }

    #[test]
    fn no_context_returns_none() {
        assert!(parse_run_context(None, None).expect("parse").is_none());
    }

    #[test]
    fn invalid_run_id_is_rejected() {
        let meta = meta_with(json!({"run_id": "bad id!"}));
        assert!(parse_run_context(Some(&meta), None).is_err());
    }

    #[test]
    fn inject_budget_adds_top_level_field() {
        let result = CallToolResult::success(vec![ContentBlock::text(
            json!({"lines": [1, 2, 3]}).to_string(),
        )]);
        let state = BudgetState {
            run_id: "r".into(),
            calls_used: 1,
            calls_remaining: 9,
            page_calls_used: 1,
            page_calls_remaining: 4,
            snapshot_epoch: Some(3),
            cached: false,
        };
        let injected = inject_budget(result, &state);
        let text = match &injected.content[0] {
            ContentBlock::Text(text) => &text.text,
            _ => panic!("text block expected"),
        };
        let payload: Value = serde_json::from_str(text).expect("json");
        assert_eq!(payload["_budget"]["calls_remaining"], 9);
        assert_eq!(payload["_budget"]["snapshot_epoch"], 3);
        assert_eq!(payload["lines"].as_array().map(Vec::len), Some(3));
    }
}
