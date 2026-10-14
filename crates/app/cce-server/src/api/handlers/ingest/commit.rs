//! Commit entry applying staged files through the incremental pipeline.

use axum::extract::{Path as AxumPath, State};
use cce_api::models::{ErrorResponse, IngestCommitResponse, error_codes};
use cce_storage_common::VectorStorage;
use cce_storage_metadb_sqlite::{ProjectRepository, ProjectUpdateRecord};

use super::staging::{INGEST_STAGING_RETENTION, clear_project_staging, sweep_project_staging};
use crate::api::handlers::project::management::record_to_config;
use crate::api::response::ApiResult;
use crate::api::state::AppState;

/// Apply staged project files through the hot-update incremental path.
///
/// The first commit for a project still runs the full index to establish the
/// baseline. Later commits sync the resident detector with the commit file
/// universe and run one incremental operation, so a one-file change only
/// reparses and re-embeds that file instead of the whole project.
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
    // from interrupted pushes do not survive until the next push.
    sweep_project_staging(id, INGEST_STAGING_RETENTION);
    let index_options = cce_orchestrator::IndexOptions::new(&config.root_path)
        .with_extensions(config.extensions)
        .with_exclude_dirs(config.exclude_dirs)
        .with_gitignore(config.respect_gitignore)
        .with_ignore_patterns(config.ignore_patterns);

    state
        .engine
        .metrics_registry()
        .counter(
            "ingest_commit_total",
            &[("project_id", id.to_string().as_str())],
        )
        .increment();
    let index_start = std::time::Instant::now();

    let coordinator = match state.engine.get_hot_update_coordinator(id).await {
        Ok(coordinator) => coordinator,
        Err(e) => {
            tracing::warn!(
                project_id = id,
                error = %e,
                "Hot-update coordinator unavailable, falling back to full index"
            );
            return run_full_commit(
                &state,
                store.as_ref(),
                &record,
                index_options,
                id,
                index_start,
            )
            .await;
        }
    };

    let mut guard = coordinator.lock().await;
    guard
        .sync_scan_universe(index_options.to_scan_options())
        .await;
    let stored_files = match guard.change_detection_stats().await {
        Ok(stats) => stats.stored_files,
        Err(e) => {
            tracing::warn!(
                project_id = id,
                error = %e,
                "Change-detection stats unavailable, falling back to full index"
            );
            drop(guard);
            return run_full_commit(
                &state,
                store.as_ref(),
                &record,
                index_options,
                id,
                index_start,
            )
            .await;
        }
    };
    if stored_files == 0 {
        drop(guard);
        return run_full_commit(
            &state,
            store.as_ref(),
            &record,
            index_options,
            id,
            index_start,
        )
        .await;
    }

    let operation = match guard.run_commit_update().await {
        Ok(operation) => operation,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Incremental commit failed: {e}"),
            ));
        }
    };
    let changed = operation.summary.total_files_processed;
    let success = operation.is_successful();
    drop(guard);

    let total_files = match state.engine.get_hot_update_coordinator(id).await {
        Ok(coordinator) => coordinator
            .lock()
            .await
            .change_detection_stats()
            .await
            .map(|stats| stats.stored_files)
            .unwrap_or(changed),
        Err(_) => changed,
    };
    let total_for_ratio = total_files.max(changed).max(1);
    let ratio_permille = (changed * 1000) / total_for_ratio;
    let elapsed_ms = index_start.elapsed().as_millis() as f64;
    let project_id_label = id.to_string();
    let labels: [(&str, &str); 1] = [("project_id", project_id_label.as_str())];
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_duration_ms", &labels)
        .observe(elapsed_ms);
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_changed_files", &labels)
        .observe(changed as f64);
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_change_ratio_permille", &labels)
        .observe(ratio_permille as f64);
    tracing::info!(
        project_id = id,
        changed,
        total_files,
        ratio_permille,
        elapsed_ms,
        success,
        "ingest commit applied through the incremental path"
    );

    let (total_entities, total_vectors) = read_index_totals(&state, id).await;
    let now = chrono::Utc::now().to_rfc3339();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::update(tx, id, &updates))
    {
        tracing::warn!("Failed to update last_indexed: {e}");
    }
    if success {
        clear_project_staging(id);
    }
    ApiResult::Success(IngestCommitResponse {
        success,
        project_id: id,
        project_name: record.name,
        indexed_files: changed,
        total_entities,
        total_vectors,
        elapsed_ms: index_start.elapsed().as_millis() as u64,
    })
}

/// Bootstrap path running the full index to establish the baseline.
async fn run_full_commit(
    state: &AppState,
    store: &cce_storage_metadb_sqlite::SqliteClient,
    record: &cce_storage_metadb_sqlite::ProjectRecord,
    index_options: cce_orchestrator::IndexOptions,
    id: i64,
    index_start: std::time::Instant,
) -> ApiResult<IngestCommitResponse> {
    let result = match state.engine.index(id, index_options).await {
        Ok(result) => result,
        Err(e) => {
            return ApiResult::Error(ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                format!("Index execution failed: {e}"),
            ));
        }
    };
    let elapsed_ms = index_start.elapsed().as_millis() as f64;
    let project_id_label = id.to_string();
    let labels: [(&str, &str); 1] = [("project_id", project_id_label.as_str())];
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_duration_ms", &labels)
        .observe(elapsed_ms);
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_changed_files", &labels)
        .observe(result.indexed_files as f64);
    state
        .engine
        .metrics_registry()
        .histogram_default("ingest_commit_change_ratio_permille", &labels)
        .observe(1000.0);
    tracing::info!(
        project_id = id,
        indexed_files = result.indexed_files,
        elapsed_ms,
        "ingest commit established the baseline through a full index"
    );
    let now = chrono::Utc::now().to_rfc3339();
    let updates = ProjectUpdateRecord::default().with_last_indexed(now);
    if let Err(e) = store.with_transaction(|tx| ProjectRepository::update(tx, id, &updates)) {
        tracing::warn!("Failed to update last_indexed: {e}");
    }
    clear_project_staging(id);
    ApiResult::Success(IngestCommitResponse {
        success: result.is_success(),
        project_id: id,
        project_name: record.name.clone(),
        indexed_files: result.indexed_files,
        total_entities: result.total_entities,
        total_vectors: result.total_vectors,
        elapsed_ms: result.elapsed_ms,
    })
}

/// Authoritative post-run totals matching the incremental index handler.
async fn read_index_totals(state: &AppState, project_id: i64) -> (usize, usize) {
    let mut total_entities = 0usize;
    if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
        let orchestrator = orchestrator.lock().await;
        if let Some(snapshot) = orchestrator.get_published_relation_snapshot() {
            total_entities = snapshot.function_count();
        } else if let Ok(runtime) = state.engine.get_relation_runtime(project_id).await
            && let Some(snapshot) = runtime.get_snapshot().await
        {
            total_entities = snapshot.entity_count();
        }
    }
    let mut total_vectors = 0usize;
    let group_id = crate::api::handlers::storage::resolve_group_id(state, project_id).await;
    if let Some(gid) = group_id {
        total_vectors = state
            .engine
            .vector()
            .count_points_by_group(&gid)
            .await
            .unwrap_or(0);
    }
    (total_entities, total_vectors)
}
