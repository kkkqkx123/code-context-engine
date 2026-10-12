//! Shared helpers and pure predicates for the MCP tool implementations.

use cce_codegraph::index::snapshot_query::SnapshotSymbolQueryOps;
use cce_orchestrator::query::RelationQueryOptions;
use cce_types::EntityId;

/// Relation options for the call-semantics tools.
/// `entity_callees`, `entity_callers` and `call_chain` promise call
/// relationships, so only call-domain edges qualify: structural edges (impl
/// association, trait bound, inheritance) and dependency edges (import/use)
/// belong to other tools, and edges to unresolved external targets carry no
/// entity to navigate to.
pub fn call_domain_relation_options() -> RelationQueryOptions {
    RelationQueryOptions::new()
        .with_relation_domains(vec!["call".to_string()])
        .with_include_external(false)
}

/// Resolve a stable symbol ID to a runtime entity ID.
/// Runtime numeric IDs are never accepted: they are allocated per process and
/// are not reproducible across reindex runs.
pub fn resolve_stable_id(
    searcher: &cce_orchestrator::query::RelationSearcher,
    stable_id: &str,
) -> Option<EntityId> {
    searcher
        .query()
        .index()
        .get_entity_id_by_stable_symbol_id(stable_id)
}

/// Whether a tool name passes the configured allow-list. `None` allows all;
/// an empty list allows none.
pub fn is_tool_allowed(allowed: Option<&[String]>, name: &str) -> bool {
    match allowed {
        None => true,
        Some(list) => list.iter().any(|tool| tool == name),
    }
}

/// Canonicalize a call-chain direction spelling. Accepts the HTTP `down`/`up`
/// and MCP `forward`/`backward` spellings plus `callers`; returns the
/// canonical label and whether the traversal runs backward. Unrecognized
/// spellings fall back to forward traversal.
pub fn canonical_call_direction(raw: Option<&str>) -> (&'static str, bool) {
    let direction = raw.unwrap_or("forward").to_ascii_lowercase();
    let backward = matches!(direction.as_str(), "backward" | "up" | "callers");
    let canonical = if backward { "backward" } else { "forward" };
    (canonical, backward)
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
    fn call_domain_options_stay_call_only_without_external() {
        let options = call_domain_relation_options();
        assert_eq!(options.relation_domains, vec!["call".to_string()]);
        assert!(!options.include_external);
    }

    #[test]
    fn allow_list_blocks_unlisted_tools_and_empty_blocks_all() {
        assert!(is_tool_allowed(None, "search"));
        assert!(is_tool_allowed(Some(&["search".to_string()]), "search"));
        assert!(!is_tool_allowed(Some(&["search".to_string()]), "fold"));
        assert!(!is_tool_allowed(Some(&[]), "search"));
    }
}
