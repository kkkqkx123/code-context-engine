//! Shared helpers and pure predicates for the MCP tool implementations.

use cce_server_shared::services::relation::{RelationServiceError, relation_error_parts};
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;

/// Render a shared-service failure as a structured MCP tool error.
///
/// Availability and lookup failures stay visible to the caller: the relation
/// service never substitutes an empty success payload for an unavailable
/// index. The code and message come from the shared relation error contract,
/// so a variant carries the same code on HTTP and MCP.
pub fn relation_tool_error(
    operation: &str,
    error: &RelationServiceError,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let (code, message, details) = relation_error_parts(error);
    let message = format!("{operation} failed: {message}");
    Ok(match details {
        Some(details) => tool_failure_with_details(code, message, details),
        None => tool_failure(code, message),
    })
}

/// Build a structured tool error result.
///
/// The payload is a single JSON text block so callers parse success and
/// failure with the same mechanism. `isError` still marks the failure at the
/// protocol level; `code` adds a stable, machine-readable classification.
pub fn tool_failure(code: &'static str, message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(
        json!({ "error": { "code": code, "message": message } }).to_string(),
    )])
}

/// Build a structured tool error result with an opaque machine-readable detail.
pub fn tool_failure_with_details(
    code: &'static str,
    message: String,
    details: String,
) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(
        json!({ "error": { "code": code, "message": message, "details": details } }).to_string(),
    )])
}

/// Whether a tool name passes the configured allow-list. `None` allows all;
/// an empty list allows none.
pub fn is_tool_allowed(allowed: Option<&[String]>, name: &str) -> bool {
    match allowed {
        None => true,
        Some(list) => list.iter().any(|tool| tool == name),
    }
}

/// Whether a `deleted` flag vector matches the `files` vector. Empty applies
/// false to every file, a single boolean applies to all files, otherwise the
/// lengths must agree.
pub fn is_valid_deleted_shape(deleted_len: usize, files_len: usize) -> bool {
    deleted_len <= 1 || deleted_len == files_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::error_codes;

    #[test]
    fn allow_list_blocks_unlisted_tools_and_empty_blocks_all() {
        assert!(is_tool_allowed(None, "search"));
        assert!(is_tool_allowed(Some(&["search".to_string()]), "search"));
        assert!(!is_tool_allowed(Some(&["search".to_string()]), "fold"));
        assert!(!is_tool_allowed(Some(&[]), "search"));
    }

    #[test]
    fn deleted_shape_accepts_all_and_per_file_forms() {
        assert!(is_valid_deleted_shape(0, 3));
        assert!(is_valid_deleted_shape(1, 3));
        assert!(is_valid_deleted_shape(3, 3));
        assert!(!is_valid_deleted_shape(2, 3));
    }

    #[test]
    fn tool_failure_emits_parseable_structured_error() {
        let result = tool_failure(error_codes::INVALID_INPUT, "bad limit".to_string());
        let text = match &result.content[0] {
            ContentBlock::Text(text) => text.text.clone(),
            _ => panic!("tool error must carry a text block"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("error is JSON");
        assert_eq!(parsed["error"]["code"], error_codes::INVALID_INPUT);
        assert_eq!(parsed["error"]["message"], "bad limit");
        assert!(parsed["error"].get("details").is_none());
    }

    #[test]
    fn tool_failure_with_details_keeps_opaque_payload() {
        let result = tool_failure_with_details(
            error_codes::AMBIGUOUS_SYMBOL,
            "many matches".to_string(),
            "[{\"stable_id\":\"sym_a\"}]".to_string(),
        );
        let text = match &result.content[0] {
            ContentBlock::Text(text) => text.text.clone(),
            _ => panic!("tool error must carry a text block"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("error is JSON");
        assert_eq!(parsed["error"]["code"], error_codes::AMBIGUOUS_SYMBOL);
        assert!(
            parsed["error"]["details"]
                .as_str()
                .expect("details is a string")
                .contains("sym_a")
        );
    }

    #[test]
    fn relation_error_parts_agree_with_known_codes() {
        let (code, _, _) = relation_error_parts(&RelationServiceError::InvalidInput("x".into()));
        assert_eq!(code, error_codes::INVALID_REQUEST);
        let (code, _, _) = relation_error_parts(&RelationServiceError::Unavailable {
            state: cce_server_shared::runtime::RelationRuntimeState::Unloaded,
            epoch: 0,
        });
        assert_eq!(code, error_codes::SERVICE_UNAVAILABLE);
    }
}
