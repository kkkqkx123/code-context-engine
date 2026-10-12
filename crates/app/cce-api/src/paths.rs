//! Single source of truth for HTTP route paths.
//!
//! Server routers register these templates while CLI and gateway clients
//! build concrete paths through the helpers below. Templates use the unified
//! project placeholder so routers and documentation never diverge.

/// Project identifier placeholder used by every project scoped template.
pub const PROJECT_ID_PLACEHOLDER: &str = "{project_id}";

pub const INDEX: &str = "/api/index";
pub const INDEX_INCREMENTAL: &str = "/api/index/incremental";
pub const PARSE: &str = "/api/parse";
pub const SUMMARY: &str = "/api/summary";
pub const INDEX_FILE_TEMPLATE: &str = "/api/index/file/{path}";
pub const INDEX_ENTITY_TEMPLATE: &str = "/api/index/entity/{id}";
pub const INDEX_BATCH: &str = "/api/index/batch";
pub const INDEX_STATS: &str = "/api/index/stats";
pub const STORAGE_STATUS: &str = "/api/storage/status";

pub const PROJECT: &str = "/api/project";
pub const PROJECT_DETAIL_TEMPLATE: &str = "/api/project/{project_id}";
pub const PROJECT_INDEX_TEMPLATE: &str = "/api/project/{project_id}/index";
pub const PROJECT_DEAD_LETTERS_TEMPLATE: &str = "/api/project/{project_id}/dead-letters";
pub const PROJECT_DEAD_LETTERS_RETRY_TEMPLATE: &str =
    "/api/project/{project_id}/dead-letters/retry";
pub const PROJECT_DEAD_LETTERS_ACK_TEMPLATE: &str =
    "/api/project/{project_id}/dead-letters/acknowledge";
pub const PROJECT_RELOAD_TEMPLATE: &str = "/api/project/{project_id}/reload";
pub const PROJECT_CONFIG_TEMPLATE: &str = "/api/project/{project_id}/config";

pub const FUNCTION_DETAIL_TEMPLATE: &str = "/api/project/{project_id}/function/{id}";
pub const FUNCTION_CALLS_TEMPLATE: &str = "/api/project/{project_id}/function/{id}/calls";
pub const FUNCTION_CALLERS_TEMPLATE: &str = "/api/project/{project_id}/function/{id}/callers";
pub const CALL_CHAIN_TEMPLATE: &str = "/api/project/{project_id}/call-chain/{id}";
pub const CALL_PATH_TEMPLATE: &str = "/api/project/{project_id}/call-path";
pub const CLASS_INHERITANCE_TEMPLATE: &str = "/api/project/{project_id}/class/{id}/inheritance";
pub const CLASS_IMPLEMENTATIONS_TEMPLATE: &str =
    "/api/project/{project_id}/class/{id}/implementations";
pub const CLASSIFICATION_STATS_TEMPLATE: &str =
    "/api/project/{project_id}/relations/classification/stats";
pub const CLASSIFICATION_DETAIL_TEMPLATE: &str =
    "/api/project/{project_id}/relations/classification/{classification}";

pub const GRAPH_EGO_TEMPLATE: &str = "/api/project/{project_id}/graph/ego";
pub const GRAPH_PATH_TEMPLATE: &str = "/api/project/{project_id}/graph/path";
pub const GRAPH_SUBGRAPH_TEMPLATE: &str = "/api/project/{project_id}/graph/subgraph";
pub const GRAPH_COMPONENTS_TEMPLATE: &str = "/api/project/{project_id}/graph/components";
pub const GRAPH_EXPORT_TEMPLATE: &str = "/api/project/{project_id}/graph/export";
pub const GRAPH_IMPACT_TEMPLATE: &str = "/api/project/{project_id}/graph/impact";
pub const GRAPH_ENTITY_IMPACT_TEMPLATE: &str = "/api/project/{project_id}/graph/entity-impact";
pub const GRAPH_CYCLES_TEMPLATE: &str = "/api/project/{project_id}/graph/cycles";
pub const GRAPH_STRUCTURAL_TEMPLATE: &str = "/api/project/{project_id}/graph/structural";
pub const GRAPH_MODULE_TEMPLATE: &str = "/api/project/{project_id}/graph/module";

pub const METRICS: &str = "/api/metrics";
pub const METRICS_JSON: &str = "/api/metrics/json";
pub const METRICS_HISTORY: &str = "/api/metrics/history";
pub const METRICS_CLEANUP: &str = "/api/metrics/cleanup";

pub const WATCH_START_TEMPLATE: &str = "/api/project/{project_id}/watch/start";
pub const WATCH_STOP_TEMPLATE: &str = "/api/project/{project_id}/watch/stop";
pub const WATCH_STATUS_TEMPLATE: &str = "/api/project/{project_id}/watch/status";

pub const CONFIG_RELOAD: &str = "/api/config/reload";
pub const CONFIG_INFO: &str = "/api/config";
pub const CONFIG_VALIDATE: &str = "/api/config/validate";

pub const QDRANT_PROCESS_STATUS: &str = "/api/qdrant/process/status";
pub const QDRANT_PROCESS_START: &str = "/api/qdrant/process/start";
pub const QDRANT_PROCESS_STOP: &str = "/api/qdrant/process/stop";
pub const QDRANT_PROCESS_RESTART: &str = "/api/qdrant/process/restart";

pub const SEARCH: &str = "/api/search";
pub const SEARCH_AGGREGATED: &str = "/api/search/aggregated";
pub const ENTITY_SEARCH: &str = "/api/entities/search";

pub const TOOLS_COMPRESS: &str = "/api/tools/compress";
pub const TOOLS_COMPRESS_BATCH: &str = "/api/tools/compress/batch";
pub const TOOLS_DIAGNOSE: &str = "/api/tools/diagnose";
pub const TOOLS_FOLD: &str = "/api/tools/fold";
pub const TOOLS_FOLD_BATCH: &str = "/api/tools/fold/batch";
pub const TOOLS_KEYWORD_SEARCH: &str = "/api/tools/keyword-search";
pub const TOOLS_SYMBOLS: &str = "/api/tools/symbols";
pub const TOOLS_REFERENCES: &str = "/api/tools/references";
pub const TOOLS_DEFINITION: &str = "/api/tools/definition";

pub const HEALTH: &str = "/api/health";
pub const HEALTH_QDRANT: &str = "/api/health/qdrant";
pub const HEALTH_EMBEDDING: &str = "/api/health/embedding";
pub const HEALTH_BM25: &str = "/api/health/bm25";

pub const RETRY_QUEUE: &str = "/api/retry-queue";
pub const RETRY_QUEUE_DEAD: &str = "/api/retry-queue/dead";
pub const RETRY_QUEUE_PROCESS: &str = "/api/retry-queue/process";

pub const INGEST_MANIFEST_TEMPLATE: &str = "/api/project/{project_id}/ingest/manifest";
pub const INGEST_BATCH_TEMPLATE: &str = "/api/project/{project_id}/ingest/batch";
pub const INGEST_COMMIT_TEMPLATE: &str = "/api/project/{project_id}/ingest/commit";
pub const INGEST_EVENT_TEMPLATE: &str = "/api/project/{project_id}/ingest/event";

pub const ADMISSION_STATS: &str = "/api/admission/stats";

/// Wire byte budget per gateway upload batch.
///
/// Accounting uses base64 wire length so a batch stays inside the admission
/// body limit after transport growth. The gateway filters with this bound
/// before pushing so oversized batches never consume transfer bandwidth.
pub const GATEWAY_BATCH_WIRE_BYTES: u64 = 4 * 1024 * 1024;

/// Build a project scoped path from a template holding the unified placeholder.
fn expand_project(template: &str, project_id: i64) -> String {
    template.replace(PROJECT_ID_PLACEHOLDER, &project_id.to_string())
}

/// Concrete project detail path.
pub fn project_detail(project_id: i64) -> String {
    expand_project(PROJECT_DETAIL_TEMPLATE, project_id)
}

/// Concrete project index path.
pub fn project_index(project_id: i64) -> String {
    expand_project(PROJECT_INDEX_TEMPLATE, project_id)
}

/// Concrete dead letter list path.
pub fn project_dead_letters(project_id: i64) -> String {
    expand_project(PROJECT_DEAD_LETTERS_TEMPLATE, project_id)
}

/// Concrete dead letter retry path.
pub fn project_dead_letters_retry(project_id: i64) -> String {
    expand_project(PROJECT_DEAD_LETTERS_RETRY_TEMPLATE, project_id)
}

/// Concrete dead letter acknowledge path.
pub fn project_dead_letters_acknowledge(project_id: i64) -> String {
    expand_project(PROJECT_DEAD_LETTERS_ACK_TEMPLATE, project_id)
}

/// Concrete project reload path.
pub fn project_reload(project_id: i64) -> String {
    expand_project(PROJECT_RELOAD_TEMPLATE, project_id)
}

/// Concrete project config path.
pub fn project_config(project_id: i64) -> String {
    expand_project(PROJECT_CONFIG_TEMPLATE, project_id)
}

/// Concrete ingest manifest path.
pub fn ingest_manifest(project_id: i64) -> String {
    expand_project(INGEST_MANIFEST_TEMPLATE, project_id)
}

/// Concrete ingest batch path.
pub fn ingest_batch(project_id: i64) -> String {
    expand_project(INGEST_BATCH_TEMPLATE, project_id)
}

/// Concrete ingest commit path.
pub fn ingest_commit(project_id: i64) -> String {
    expand_project(INGEST_COMMIT_TEMPLATE, project_id)
}

/// Concrete ingest event path.
pub fn ingest_event(project_id: i64) -> String {
    expand_project(INGEST_EVENT_TEMPLATE, project_id)
}

/// Concrete watch start path.
pub fn watch_start(project_id: i64) -> String {
    expand_project(WATCH_START_TEMPLATE, project_id)
}

/// Concrete watch stop path.
pub fn watch_stop(project_id: i64) -> String {
    expand_project(WATCH_STOP_TEMPLATE, project_id)
}

/// Concrete watch status path.
pub fn watch_status(project_id: i64) -> String {
    expand_project(WATCH_STATUS_TEMPLATE, project_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn templates_use_unified_project_placeholder() {
        let templates = [
            PROJECT_DETAIL_TEMPLATE,
            PROJECT_INDEX_TEMPLATE,
            PROJECT_DEAD_LETTERS_TEMPLATE,
            PROJECT_DEAD_LETTERS_RETRY_TEMPLATE,
            PROJECT_DEAD_LETTERS_ACK_TEMPLATE,
            PROJECT_RELOAD_TEMPLATE,
            PROJECT_CONFIG_TEMPLATE,
            FUNCTION_DETAIL_TEMPLATE,
            GRAPH_EGO_TEMPLATE,
            WATCH_START_TEMPLATE,
            INGEST_MANIFEST_TEMPLATE,
        ];
        for template in templates {
            assert!(
                template.contains(PROJECT_ID_PLACEHOLDER),
                "template missing unified placeholder: {template}"
            );
            assert!(
                !template.contains("{id}/ingest") && !template.contains("/project/{id}"),
                "legacy placeholder leaked: {template}"
            );
        }
    }

    #[test]
    fn static_paths_have_no_placeholders() {
        let statics = [
            INDEX,
            SEARCH,
            SEARCH_AGGREGATED,
            ENTITY_SEARCH,
            TOOLS_FOLD,
            TOOLS_FOLD_BATCH,
            HEALTH,
            RETRY_QUEUE,
            ADMISSION_STATS,
        ];
        for path in statics {
            assert!(!path.contains('{'), "static path must not template: {path}");
        }
    }

    #[test]
    fn builders_expand_project_placeholder() {
        assert_eq!(project_detail(7), "/api/project/7");
        assert_eq!(ingest_manifest(7), "/api/project/7/ingest/manifest");
        assert_eq!(watch_status(7), "/api/project/7/watch/status");
    }

    #[test]
    fn all_static_paths_are_unique() {
        let all = [
            INDEX,
            INDEX_INCREMENTAL,
            PARSE,
            SUMMARY,
            INDEX_BATCH,
            INDEX_STATS,
            STORAGE_STATUS,
            PROJECT,
            METRICS,
            METRICS_JSON,
            METRICS_HISTORY,
            METRICS_CLEANUP,
            CONFIG_RELOAD,
            CONFIG_INFO,
            CONFIG_VALIDATE,
            QDRANT_PROCESS_STATUS,
            QDRANT_PROCESS_START,
            QDRANT_PROCESS_STOP,
            QDRANT_PROCESS_RESTART,
            SEARCH,
            SEARCH_AGGREGATED,
            ENTITY_SEARCH,
            TOOLS_COMPRESS,
            TOOLS_COMPRESS_BATCH,
            TOOLS_DIAGNOSE,
            TOOLS_FOLD,
            TOOLS_FOLD_BATCH,
            TOOLS_KEYWORD_SEARCH,
            TOOLS_SYMBOLS,
            TOOLS_REFERENCES,
            TOOLS_DEFINITION,
            HEALTH,
            HEALTH_QDRANT,
            HEALTH_EMBEDDING,
            HEALTH_BM25,
            RETRY_QUEUE,
            RETRY_QUEUE_DEAD,
            RETRY_QUEUE_PROCESS,
            ADMISSION_STATS,
        ];
        let mut seen = HashSet::new();
        for path in all {
            assert!(seen.insert(path), "duplicate static path: {path}");
        }
    }
}
