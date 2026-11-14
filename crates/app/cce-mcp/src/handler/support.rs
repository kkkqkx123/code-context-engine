//! Shared helpers and pure predicates for the MCP tool implementations.

use cce_server_shared::services::relation::RelationServiceError;
use rmcp::model::{CallToolResult, ContentBlock};

/// Render a shared-service failure as an MCP tool error.
///
/// Availability and lookup failures stay visible to the caller: the relation
/// service never substitutes an empty success payload for an unavailable
/// index.
pub fn tool_error(
    operation: &str,
    error: RelationServiceError,
) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(tool_failure(format!("{operation} failed: {error}")))
}

/// Build a tool error result from a message.
pub fn tool_failure(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
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
}
