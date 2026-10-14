//! Commit entry running the index pipeline over staged files.

use axum::extract::{Path as AxumPath, State};
use cce_api::models::{ErrorResponse, IngestCommitResponse, error_codes};
use cce_storage_metadb_sqlite::{ProjectRepository, ProjectUpdateRecord};

use super::staging::{INGEST_STAGING_RETENTION, clear_project_staging, sweep_project_staging};
use crate::api::handlers::project::management::record_to_config;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

/// Run the existing full index pipeline over the staged project mirror.
#[utoipa::path(
    post, path = "/api/project/{project_id}/ingest/commit", tag = "Ingest",
    params(("project_id" = i64, Path, description = "Project id")),
    responses(
        (status = 200, body = IngestCommitResponse, description = "Success"),
        (status = 400, body = ErrorResponse, description = "Invalid request"),
        (status = 404, body = ErrorResponse, description = "Resource not found"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_ingest_commit(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<IngestCommitResponse> {
    let store = match state.engine.metadata_store() {
        Some(store) => store,
        None => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                "Metadata store not initialized",
            ));
        }
    };
    let record = match store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, id))
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                "Project does not exist",
            ));
        }
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::STORAGE_ERROR,
                format!("Failed to query project: {e}"),
            ));
        }
    };
    let config = record_to_config(&record);
    // Reclaim expired version staging before indexing so abandoned chunks
    // from interrupted pushes do not survive until the next full push.
    sweep_project_staging(id, INGEST_STAGING_RETENTION);
    let index_options = cce_orchestrator::IndexOptions::new(&config.root_path)
        .with_extensions(config.extensions)
        .with_exclude_dirs(config.exclude_dirs)
        .with_gitignore(config.respect_gitignore)
        .with_ignore_patterns(config.ignore_patterns);

    record_ingest_commit_shadow(&state, std::sync::Arc::clone(store), &index_options, id).await;
    state
        .engine
        .metrics_registry()
        .counter("ingest_commit_total", &[("project_id", &id.to_string())])
        .increment();
    let index_start = std::time::Instant::now();
    let result = match state.engine.index(id, index_options).await {
        Ok(result) => result,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Index execution failed: {e}"),
            ));
        }
    };
    state
        .engine
        .metrics_registry()
        .histogram_default(
            "ingest_commit_full_index_ms",
            &[("project_id", &id.to_string())],
        )
        .observe(index_start.elapsed().as_millis() as f64);
    let now = chrono::Utc::now().to_rfc3339();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::update(tx, id, &updates))
    {
        tracing::warn!("Failed to update last_indexed: {e}");
    }
    clear_project_staging(id);
    ApiResult::Success(IngestCommitResponse {
        success: result.is_success(),
        project_id: id,
        project_name: record.name,
        indexed_files: result.indexed_files,
        total_entities: result.total_entities,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
    })
}

/// Opt-in shadow measurement of the ingest-commit change set.
///
/// Reports, without applying anything, how large the change set the
/// incremental path would process is relative to the whole project, so
/// operators can gather the frequency and changed-versus-total evidence for
/// deciding whether this entry should route to the hot-update incremental
/// path. The measurement is a second full scan and re-hash because the full
/// index has no unchanged-file filter, so it is off by default and enabled
/// with `CCE_INGEST_COMMIT_SHADOW=1` during an evidence-gathering window.
/// Best-effort and non-fatal: any failure is logged at debug and ignored.
async fn record_ingest_commit_shadow(
    state: &crate::api::state::AppState,
    db: std::sync::Arc<cce_storage_metadb_sqlite::SqliteClient>,
    index_options: &cce_orchestrator::IndexOptions,
    project_id: i64,
) {
    let enabled = std::env::var("CCE_INGEST_COMMIT_SHADOW")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return;
    }
    // File hashes, entities and the active generation manifest live in the
    // per-project database — the same store the full index and the incremental
    // path read — so the measurement must be scoped to the project, not run
    // against the registry database that only holds the project list.
    let project_db = match db.as_ref().for_project(project_id) {
        Ok(project_db) => project_db,
        Err(e) => {
            tracing::debug!(
                project_id,
                error = %e,
                "ingest commit shadow measurement skipped: project store unavailable"
            );
            return;
        }
    };
    let changes = match cce_orchestrator::hot_update::measure_index_change_set(
        project_db,
        index_options,
        project_id,
    )
    .await
    {
        Ok(changes) => changes,
        Err(e) => {
            tracing::debug!(project_id, error = %e, "ingest commit shadow measurement failed");
            return;
        }
    };
    let changed = changes.total_changes();
    let total = changed + changes.unchanged.len();
    let ratio_permille = (changed * 1000).checked_div(total).unwrap_or(0);
    state
        .engine
        .metrics_registry()
        .histogram_default(
            "ingest_commit_change_ratio_permille",
            &[("project_id", &project_id.to_string())],
        )
        .observe(ratio_permille as f64);
    tracing::info!(
        project_id,
        added = changes.added.len(),
        modified = changes.modified.len(),
        removed = changes.removed.len(),
        unchanged = changes.unchanged.len(),
        changed,
        total,
        ratio_permille,
        "ingest commit shadow change-set measurement"
    );
}
