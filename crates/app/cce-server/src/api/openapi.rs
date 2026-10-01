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
        handlers::tools::fold::handle_fold_batch,
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
    use std::path::{Path, PathBuf};

    // ---- contract configuration (repo-specific) ----

    const SNAPSHOT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../frontend/openapi.json"
    );
    const REFRESH_ENV: &str = "CCE_REFRESH_OPENAPI";

    fn source_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// Mount prefix for `.route()` registrations in the file at `rel`
    /// (relative to the crate `src` root); `None` skips the file for route
    /// collection. The CCE router registers every endpoint with its final
    /// absolute `/api/...` path.
    fn route_prefix(rel: &str) -> Option<String> {
        (rel == "api/router.rs").then(String::new)
    }

    /// Dev-only documentation endpoint, not part of the contract.
    fn is_ignored_path(path: &str) -> bool {
        path.starts_with("/api-docs")
    }

    fn feature_enabled(name: &str) -> bool {
        panic!("unexpected feature gate in scanned route sources: {name}")
    }

    #[test]
    fn openapi_snapshot_matches() {
        let doc = openapi_json();
        if std::env::var_os(REFRESH_ENV).is_some() {
            std::fs::write(SNAPSHOT, format!("{doc}\n")).expect("snapshot must be writable");
            return;
        }
        let expected = std::fs::read_to_string(SNAPSHOT).expect("openapi snapshot must exist");
        assert_eq!(
            doc.trim_end(),
            expected.trim_end(),
            "snapshot drifted; refresh with {REFRESH_ENV}=1"
        );
    }

    #[test]
    fn routes_match_openapi_paths() {
        let routed = collect_routed();
        let annotated = collect_annotated();
        let documented = collect_documented();

        assert_sets_equal(
            &routed,
            &annotated,
            "router registrations vs #[utoipa::path] annotations",
        );
        assert_sets_equal(
            &annotated,
            &documented,
            "#[utoipa::path] annotations vs ApiDoc registration",
        );
        assert!(!routed.is_empty(), "expected a non-empty route set");
    }

    // ---- unified guard engine (shared across repos; keep verbatim) ----

    const HTTP_METHODS: [&str; 8] = [
        "get", "put", "post", "patch", "delete", "head", "options", "trace",
    ];

    fn assert_sets_equal(
        left: &BTreeSet<(String, String)>,
        right: &BTreeSet<(String, String)>,
        label: &str,
    ) {
        let only_left: Vec<_> = left.difference(right).collect();
        let only_right: Vec<_> = right.difference(left).collect();
        assert!(
            only_left.is_empty() && only_right.is_empty(),
            "{label} drift\nonly on the left: {only_left:?}\nonly on the right: {only_right:?}"
        );
    }

    fn walk_rs(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read source dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                out.extend(walk_rs(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    fn source_files() -> Vec<(String, String)> {
        let root = source_root();
        walk_rs(&root)
            .into_iter()
            .map(|path| {
                let rel = path
                    .strip_prefix(&root)
                    .expect("source under src root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).expect("read source file");
                (rel, text)
            })
            .collect()
    }

    fn collect_routed() -> BTreeSet<(String, String)> {
        let mut out = BTreeSet::new();
        for (rel, text) in source_files() {
            let Some(prefix) = route_prefix(&rel) else {
                continue;
            };
            for (method, path) in parse_route_registrations(&text) {
                let full = format!("{prefix}{path}");
                if !is_ignored_path(&full) {
                    out.insert((method, full));
                }
            }
        }
        out
    }

    fn collect_annotated() -> BTreeSet<(String, String)> {
        let mut out = BTreeSet::new();
        for (_rel, text) in source_files() {
            for (method, path) in parse_utoipa_annotations(&text) {
                out.insert((method, path));
            }
        }
        out
    }

    fn collect_documented() -> BTreeSet<(String, String)> {
        let doc: serde_json::Value =
            serde_json::from_str(&openapi_json()).expect("document must parse");
        let mut out = BTreeSet::new();
        if let Some(paths) = doc.get("paths").and_then(|p| p.as_object()) {
            for (path, item) in paths {
                if let Some(ops) = item.as_object() {
                    for method in ops.keys() {
                        out.insert((method.to_uppercase(), path.clone()));
                    }
                }
            }
        }
        out
    }

    /// Parse `#[utoipa::path(...)]` attributes into (method, full path).
    fn parse_utoipa_annotations(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut lines = text.lines().peekable();
        let mut buf: Vec<String> = Vec::new();
        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if let Some(tail) = trimmed.strip_prefix("#[utoipa::path(") {
                buf.clear();
                if tail != "(" {
                    buf.push(tail.to_string());
                }
                for line in lines.by_ref().take(60) {
                    let trimmed = line.trim();
                    if let Some(closer) = trimmed.strip_prefix(")]") {
                        buf.push(trimmed.to_string());
                        let _ = closer;
                        break;
                    }
                    buf.push(trimmed.to_string());
                }
                if let Some((method, path)) = annotation_head(&buf) {
                    out.push((method, path));
                }
            }
        }
        out
    }

    fn annotation_head(buf: &[String]) -> Option<(String, String)> {
        let method = buf.iter().find_map(|line| {
            HTTP_METHODS
                .iter()
                .find(|m| line.starts_with(&format!("{m},")))
                .map(|m| m.to_uppercase())
        })?;
        let path = buf
            .iter()
            .find_map(|line| {
                line.find("path = \"")
                    .map(|i| &line[i + "path = \"".len()..])
            })
            .and_then(|rest| rest.split('"').next())
            .map(str::to_string)?;
        Some((method, path))
    }

    /// Parse axum `.route("<path>", get(..).post(..))` registrations into
    /// (method, path) pairs. Line-based: rustfmt keeps these tables stable.
    /// Top-level `#[cfg(...)]` attributes gate whole functions so
    /// feature-disabled route bodies are ignored.
    fn parse_route_registrations(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut pending_attr: Option<String> = None;
        let mut fn_active = true;
        let mut entry: Option<(Option<String>, Vec<String>)> = None;

        let flush = |out: &mut Vec<(String, String)>,
                     entry: &mut Option<(Option<String>, Vec<String>)>| {
            if let Some((Some(path), methods)) = entry.take() {
                for method in methods {
                    out.push((method, path.clone()));
                }
            } else {
                entry.take();
            }
        };

        for line in text.lines() {
            let trimmed = line.trim();
            let top_level = !trimmed.is_empty() && !line.starts_with(char::is_whitespace);
            if top_level {
                if let Some(expr) = trimmed
                    .strip_prefix("#[cfg(")
                    .and_then(|rest| rest.strip_suffix(")]"))
                {
                    pending_attr = Some(expr.to_string());
                    continue;
                }
                if trimmed.starts_with("#[") || trimmed.starts_with("//") {
                    continue;
                }
                let is_fn = trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("pub(crate) fn ")
                    || trimmed.starts_with("async fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("pub(crate) async fn ");
                if is_fn {
                    fn_active = pending_attr.take().map_or(true, |expr| eval_cfg(&expr));
                    entry = None;
                    continue;
                }
                pending_attr = None;
            }
            if !fn_active {
                continue;
            }

            if let Some(pos) = line.find(".route(") {
                flush(&mut out, &mut entry);
                let tail = &line[pos + ".route(".len()..];
                entry = Some((first_string_literal(tail), method_tokens(tail)));
            } else if let Some((path, methods)) = entry.as_mut() {
                if path.is_none() {
                    *path = first_string_literal(trimmed);
                }
                methods.extend(method_tokens(trimmed));
            }

            let ends_entry = trimmed.starts_with(".nest(")
                || trimmed.starts_with(".layer(")
                || trimmed.starts_with(".route_layer(")
                || trimmed.starts_with(".merge(")
                || trimmed.starts_with(".fallback(")
                || trimmed.starts_with(".with_state(")
                || trimmed == ")"
                || trimmed == "),"
                || trimmed.ends_with(");");
            if ends_entry {
                flush(&mut out, &mut entry);
            }
        }
        flush(&mut out, &mut entry);
        out
    }

    fn first_string_literal(s: &str) -> Option<String> {
        let start = s.find('"')?;
        let rest = &s[start + 1..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    fn method_tokens(line: &str) -> Vec<String> {
        let cleaned = strip_string_literals(line);
        let b = cleaned.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if !(b[i].is_ascii_alphabetic() || b[i] == b'_') {
                i += 1;
                continue;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &cleaned[start..i];
            let mut j = i;
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'(' && HTTP_METHODS.contains(&word) {
                out.push(word.to_uppercase());
            }
        }
        out
    }

    fn strip_string_literals(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut in_string = false;
        for c in line.chars() {
            if c == '"' {
                in_string = !in_string;
                out.push(' ');
            } else if in_string {
                out.push(' ');
            } else {
                out.push(c);
            }
        }
        out
    }

    fn eval_cfg(expr: &str) -> bool {
        let expr = expr.trim();
        if let Some(inner) = expr.strip_prefix("not(").and_then(|s| s.strip_suffix(')')) {
            return !eval_cfg(inner);
        }
        if let Some(inner) = expr.strip_prefix("all(").and_then(|s| s.strip_suffix(')')) {
            return split_cfg_items(inner).iter().all(|item| eval_cfg(item));
        }
        if let Some(inner) = expr.strip_prefix("any(").and_then(|s| s.strip_suffix(')')) {
            return split_cfg_items(inner).iter().any(|item| eval_cfg(item));
        }
        match expr {
            "debug_assertions" => cfg!(debug_assertions),
            "test" => true,
            other => {
                if let Some(name) = other
                    .strip_prefix("feature")
                    .and_then(|s| s.trim().strip_prefix('='))
                    .and_then(|s| s.trim().split('"').nth(1))
                {
                    feature_enabled(name)
                } else {
                    panic!("unsupported cfg predicate: {expr}")
                }
            }
        }
    }

    fn split_cfg_items(expr: &str) -> Vec<&str> {
        let mut depth = 0usize;
        let mut items = Vec::new();
        let mut start = 0;
        for (i, c) in expr.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    items.push(expr[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        let last = expr[start..].trim();
        if !last.is_empty() {
            items.push(last);
        }
        items
    }
}
