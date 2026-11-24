//! Tool capability table and profile resolution for the MCP surface.
//!
//! A capability is declared once per tool in [`TOOL_CAPABILITIES`] instead of
//! hand-maintaining a read-only allow-list. A unit test in the handler pins the
//! table's key set to the router's registered tools, so adding or removing a
//! tool without updating the table fails the build. Resolution here is pure:
//! no IO, no `rmcp` types, fully unit-testable.

use std::collections::BTreeSet;

use cce_config::modules::McpProfile;

/// Whether a tool mutates the index or only reads project state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCapability {
    /// Read-only query; eligible for the wiki-read profile and the budget.
    Read,
    /// Changes index or project state; excluded from wiki-read and budget.
    Write,
}

/// Capability of every registered MCP tool, keyed by its wire name.
///
/// Keep this table in sync with `#[tool_router]`; the key-coverage test in the
/// handler is the enforcement point.
pub const TOOL_CAPABILITIES: &[(&str, ToolCapability)] = &[
    ("search", ToolCapability::Read),
    ("aggregated_search", ToolCapability::Read),
    ("file_search", ToolCapability::Read),
    ("keyword_search", ToolCapability::Read),
    ("entity_callees", ToolCapability::Read),
    ("entity_callers", ToolCapability::Read),
    ("call_chain", ToolCapability::Read),
    ("list_projects", ToolCapability::Read),
    ("get_project", ToolCapability::Read),
    ("index_project", ToolCapability::Write),
    ("incremental_index", ToolCapability::Write),
    ("fold", ToolCapability::Read),
    ("fold_batch", ToolCapability::Read),
    ("health", ToolCapability::Read),
    ("read_file", ToolCapability::Read),
    ("file_outline", ToolCapability::Read),
    ("entity_detail", ToolCapability::Read),
    ("graph_module", ToolCapability::Read),
    ("graph_ego", ToolCapability::Read),
];

/// Deterministic read tools whose repeated calls within one run and snapshot
/// epoch may be served from cache without consuming a step.
const CACHEABLE_TOOLS: &[&str] = &[
    "read_file",
    "file_outline",
    "entity_detail",
    "graph_module",
    "graph_ego",
    "keyword_search",
    "file_search",
];

/// Look up a tool's capability by wire name.
pub fn capability_of(tool: &str) -> Option<ToolCapability> {
    TOOL_CAPABILITIES
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, capability)| *capability)
}

/// Whether a tool is write-capable. Unknown tools are treated as write so a
/// registration missed by the capability table can never gain read access.
pub fn is_write_capable(tool: &str) -> bool {
    !matches!(capability_of(tool), Some(ToolCapability::Read))
}

/// Whether a deterministic tool may reuse a cached response within a run.
pub fn is_cacheable(tool: &str) -> bool {
    CACHEABLE_TOOLS.contains(&tool)
}

/// Resolve the effective tool set from a profile and an optional allow-list.
///
/// - `full` + `None` -> every tool (backward compatible).
/// - `full` + `Some(list)` -> exactly `list`.
/// - `wiki_read` + `None` -> every read-capable tool.
/// - `wiki_read` + `Some(list)` -> intersection of `list` and read tools; the
///   stricter side always wins, so a write tool can never be widened in.
pub fn resolve_allowed_tools(
    profile: McpProfile,
    enabled_tools: Option<&[String]>,
) -> BTreeSet<String> {
    let profile_set: BTreeSet<String> = match profile {
        McpProfile::Full => TOOL_CAPABILITIES
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect(),
        McpProfile::WikiRead => TOOL_CAPABILITIES
            .iter()
            .filter(|(_, capability)| *capability == ToolCapability::Read)
            .map(|(name, _)| (*name).to_string())
            .collect(),
    };
    match enabled_tools {
        None => profile_set,
        Some(list) => {
            let explicit: BTreeSet<&str> = list.iter().map(String::as_str).collect();
            profile_set
                .into_iter()
                .filter(|name| explicit.contains(name.as_str()))
                .collect()
        }
    }
}

/// Whether a tool passes the resolved set. `None` is not expected from
/// [`resolve_allowed_tools`] but is kept for the pure predicate tests.
pub fn is_allowed(allowed: &BTreeSet<String>, tool: &str) -> bool {
    allowed.contains(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_table_has_no_duplicate_names() {
        let names: BTreeSet<&str> = TOOL_CAPABILITIES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), TOOL_CAPABILITIES.len());
    }

    #[test]
    fn only_index_tools_are_write_capable() {
        let write: Vec<&str> = TOOL_CAPABILITIES
            .iter()
            .filter(|(_, c)| *c == ToolCapability::Write)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(write, vec!["index_project", "incremental_index"]);
        assert!(is_write_capable("unknown_tool"));
    }

    #[test]
    fn full_profile_without_list_allows_everything() {
        let set = resolve_allowed_tools(McpProfile::Full, None);
        assert_eq!(set.len(), TOOL_CAPABILITIES.len());
        assert!(is_allowed(&set, "index_project"));
    }

    #[test]
    fn wiki_read_excludes_write_tools() {
        let set = resolve_allowed_tools(McpProfile::WikiRead, None);
        assert_eq!(set.len(), TOOL_CAPABILITIES.len() - 2);
        assert!(!is_allowed(&set, "index_project"));
        assert!(!is_allowed(&set, "incremental_index"));
        assert!(is_allowed(&set, "read_file"));
    }

    #[test]
    fn wiki_read_intersects_explicit_list() {
        let explicit = vec!["read_file".to_string(), "index_project".to_string()];
        let set = resolve_allowed_tools(McpProfile::WikiRead, Some(&explicit));
        assert_eq!(set.len(), 1);
        assert!(is_allowed(&set, "read_file"));
        assert!(!is_allowed(&set, "index_project"));
    }

    #[test]
    fn full_profile_honours_explicit_list() {
        let explicit = vec!["fold".to_string()];
        let set = resolve_allowed_tools(McpProfile::Full, Some(&explicit));
        assert_eq!(set.len(), 1);
        assert!(is_allowed(&set, "fold"));
    }

    #[test]
    fn cacheable_set_is_subset_of_read_tools() {
        for tool in CACHEABLE_TOOLS {
            assert_eq!(capability_of(tool), Some(ToolCapability::Read));
            assert!(is_cacheable(tool));
        }
        assert!(!is_cacheable("search"));
    }
}
