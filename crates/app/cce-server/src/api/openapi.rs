//! OpenAPI document assembly and drift guards.
//!
//! `ApiDoc` collects every `#[utoipa::path]` annotation into a single
//! document. Referenced schemas are pulled in automatically by the derive,
//! so this module only lists paths and tags. Two tests pin the contract:
//! the serialized snapshot under `frontend/openapi.json` must match, and
//! every route registered in `router.rs` must have a matching annotation.

use std::sync::OnceLock;

use utoipa::OpenApi;

use super::handlers;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "CCE API",
        description = "Code context engine: indexing, search, relations and tool endpoints",
        version = "1.0.0"
    ),
    paths(
        handlers::index::execute::handle_index,
        handlers::index::incremental::handle_incremental,
        handlers::index::parse::handle_parse,
        handlers::summary::handle_summary,
        handlers::storage::handle_clear_index,
        handlers::storage::handle_delete_file,
        handlers::storage::handle_delete_entity,
        handlers::storage::handle_batch_delete,
        handlers::storage::handle_index_stats,
        handlers::storage::handle_storage_status,
        handlers::project::management::handle_create_project,
        handlers::project::query::handle_list_projects,
        handlers::project::query::handle_get_project,
        handlers::project::management::handle_update_project,
        handlers::project::management::handle_delete_project,
        handlers::project::indexing::handle_project_index,
        handlers::project::indexing::handle_dead_letter_retry,
        handlers::project::config::handle_reload_project_config,
        handlers::project::config::handle_update_project_config,
        handlers::entity::detail::handle_function_detail,
        handlers::entity::calls::handle_function_calls,
        handlers::entity::calls::handle_function_callers,
        handlers::entity::relation::handle_call_chain,
        handlers::entity::relation::handle_call_path,
        handlers::entity::relation::handle_class_inheritance,
        handlers::entity::relation::handle_class_implementations,
        handlers::entity::classification::get_classification_stats,
        handlers::entity::classification::get_relations_by_classification,
        handlers::graph::handle_graph_ego,
        handlers::graph::handle_graph_path,
        handlers::graph::handle_graph_subgraph,
        handlers::graph::handle_graph_components,
        handlers::graph::handle_graph_export,
        handlers::graph::handle_graph_impact,
        handlers::metrics::handle_get_metrics,
        handlers::metrics::handle_get_metrics_json,
        handlers::metrics::handle_get_metrics_history,
        handlers::metrics::handle_cleanup_metrics,
        handlers::watch::handle_start_watch,
        handlers::watch::handle_stop_watch,
        handlers::watch::handle_watch_status,
        handlers::config::handle_config_reload,
        handlers::config::handle_config_info,
        handlers::config::handle_config_validate,
        handlers::qdrant_admin::handle_qdrant_process_status,
        handlers::qdrant_admin::handle_qdrant_process_start,
        handlers::qdrant_admin::handle_qdrant_process_stop,
        handlers::qdrant_admin::handle_qdrant_process_restart,
        handlers::search::handle_search,
        handlers::search::handle_aggregated_search,
        handlers::entity_search::handle_entity_search,
        handlers::tools::compression::handle_compress,
        handlers::tools::compression::handle_compress_batch,
        handlers::tools::diagnosis::handle_diagnose,
        handlers::tools::fold::handle_fold,
        handlers::tools::keyword::handle_keyword_search,
        handlers::tools::symbol::handle_get_symbols,
        handlers::tools::symbol::handle_find_references,
        handlers::tools::symbol::handle_goto_definition,
        handlers::health::handle_health,
        handlers::health::handle_qdrant_health,
        handlers::health::handle_embedding_health,
        handlers::health::handle_bm25_health,
        handlers::health::handle_retry_queue_status,
        handlers::health::handle_retry_queue_process,
        handlers::health::handle_retry_queue_clear,
    ),
    tags(
        (name = "Index", description = "Indexing and parsing operations"),
        (name = "Summary", description = "Ephemeral summary generation"),
        (name = "Storage", description = "Index storage lifecycle management"),
        (name = "Project", description = "Project registry and project-scoped indexing"),
        (name = "Entity", description = "Entity detail and relation queries"),
        (name = "Graph", description = "Graph traversal over the relation snapshot"),
        (name = "Metrics", description = "Metrics export and retention"),
        (name = "Watch", description = "Hot-reload file watching"),
        (name = "Config", description = "Server configuration management"),
        (name = "Qdrant", description = "Embedded Qdrant process lifecycle"),
        (name = "Search", description = "Vector, BM25 and aggregated search"),
        (name = "Tools", description = "Programming-task tools with in-band errors"),
        (name = "Health", description = "Health checks and retry-queue management"),
    )
)]
struct ApiDoc;

/// Serialize the OpenAPI document, cached after the first call.
pub fn openapi_json() -> String {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            serde_json::to_string_pretty(&ApiDoc::openapi())
                .expect("OpenAPI document must serialize")
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const SNAPSHOT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../frontend/openapi.json"
    );

    #[test]
    fn openapi_snapshot_matches() {
        let doc = openapi_json();
        if std::env::var("CCE_REFRESH_OPENAPI").as_deref() == Ok("1") {
            std::fs::write(SNAPSHOT, &doc).expect("snapshot must be writable");
            return;
        }
        let expected = std::fs::read_to_string(SNAPSHOT).expect("openapi snapshot must exist");
        assert_eq!(doc.trim_end(), expected.trim_end());
    }

    #[test]
    fn routes_match_openapi_paths() {
        fn first_quoted(line: &str) -> Option<String> {
            let start = line.find('"')? + 1;
            let end = line[start..].find('"')? + start;
            Some(line[start..end].to_string())
        }
        fn find_method(line: &str) -> Option<String> {
            for method in ["get", "post", "put", "delete"] {
                if line.contains(&format!("{method}(")) {
                    return Some(method.to_uppercase());
                }
            }
            None
        }

        let router_src = include_str!("router.rs");
        let mut routed: BTreeSet<(String, String)> = BTreeSet::new();
        let mut in_route = false;
        let mut path: Option<String> = None;
        for line in router_src.lines() {
            let line = line.trim();
            if line.starts_with(".route(") {
                in_route = true;
                path = first_quoted(line);
                if let (Some(p), Some(m)) = (path.clone(), find_method(line)) {
                    routed.insert((m, p));
                    in_route = false;
                    path = None;
                }
                continue;
            }
            if !in_route {
                continue;
            }
            if path.is_none() {
                path = first_quoted(line);
                continue;
            }
            if let Some(m) = find_method(line) {
                routed.insert((m, path.take().expect("path captured above")));
                in_route = false;
            } else if line == ")" || line.starts_with(".") {
                in_route = false;
                path = None;
            }
        }

        let doc: serde_json::Value =
            serde_json::from_str(&openapi_json()).expect("document must parse");
        let mut documented: BTreeSet<(String, String)> = BTreeSet::new();
        if let Some(paths) = doc.get("paths").and_then(|p| p.as_object()) {
            for (path, item) in paths {
                if let Some(ops) = item.as_object() {
                    for method in ops.keys() {
                        documented.insert((method.to_uppercase(), path.clone()));
                    }
                }
            }
        }

        let routed_only: Vec<_> = routed.difference(&documented).collect();
        let documented_only: Vec<_> = documented.difference(&routed).collect();
        assert!(
            routed_only.is_empty() && documented_only.is_empty(),
            "router/document drift: routed-but-undocumented={routed_only:?} documented-but-unrouted={documented_only:?}"
        );
    }
}
