//! Shared watch orchestration for local and HTTP transports.

use crate::context::SharedContext;
use cce_api::models::{
    SUPPLY_MODE_GATEWAY, SUPPLY_MODE_KEY, StartWatchRequest, StartWatchResponse, StopWatchResponse,
    WatchStatus, WatchStatusResponse,
};
use cce_orchestrator::hot_update::HotUpdateCoordinator;
use cce_orchestrator::hot_update::watcher::WatchStatusTracker;
use std::path::PathBuf;

#[derive(Debug)]
pub enum WatchStartError {
    InvalidProjectId,
    GatewaySupplied,
    ProjectLoad(String),
    RootNotAccessible(String),
    WatchNotAccessible(String),
    NotWithinRoot { watch: String, root: String },
    PathNotFound(String),
    HotCoordinator(String),
    StartWatch(String),
    EventLoop(String),
}

#[derive(Debug)]
pub enum WatchStopError {
    InvalidProjectId,
    HotCoordinator(String),
    StopWatch(String),
}

#[derive(Debug)]
pub enum WatchStatusError {
    InvalidProjectId,
}

pub async fn watch_status(
    state: &SharedContext,
    project_id: i64,
) -> Result<WatchStatusResponse, WatchStatusError> {
    if project_id <= 0 {
        return Err(WatchStatusError::InvalidProjectId);
    }
    let status_map = state.watch_status.read().await;
    let tracker = status_map.get(&project_id).cloned().unwrap_or_default();
    drop(status_map);

    let events_processed = match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coord) => coord.lock().await.total_events(),
        Err(_) => tracker.events_processed as usize,
    };
    let relation_epoch = match state.engine.get_relation_runtime(project_id).await {
        Ok(runtime) => {
            let epoch = runtime.get_relation_epoch().await;
            (epoch > 0).then_some(epoch)
        }
        Err(_) => None,
    };
    let data_epoch = state
        .metadata_store_clone()
        .and_then(|sqlite| {
            sqlite
                .with_transaction(|tx| {
                    cce_storage_metadb_sqlite::repo::ProjectIndexManifestRepository::get_active(
                        tx, project_id,
                    )
                })
                .ok()
                .flatten()
                .map(|manifest| manifest.data_epoch)
                .or_else(|| {
                    sqlite
                        .project_meta_get_int_optional(project_id, "active_epoch")
                        .ok()
                        .flatten()
                })
        })
        .filter(|epoch| *epoch > 0);

    Ok(WatchStatusResponse {
        success: true,
        status: WatchStatus {
            active: tracker.active,
            watched_dirs: tracker.watched_dirs.clone(),
            events_processed,
            started_at: tracker.started_at.map(|t| t.to_rfc3339()),
            relation_epoch,
            data_epoch,
        },
    })
}

pub async fn watch_start(
    state: &SharedContext,
    project_id: i64,
    request: &StartWatchRequest,
) -> Result<StartWatchResponse, WatchStartError> {
    if project_id <= 0 {
        return Err(WatchStartError::InvalidProjectId);
    }
    if let Some(store) = state.engine.metadata_store()
        && let Ok(mode) = store.as_ref().with_transaction(|tx| {
            cce_storage_metadb_sqlite::ProjectRepository::meta_get_string_optional(
                tx,
                project_id,
                SUPPLY_MODE_KEY,
            )
        })
        && mode.as_deref() == Some(SUPPLY_MODE_GATEWAY)
    {
        return Err(WatchStartError::GatewaySupplied);
    }

    let project_entry = state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map_err(|e| WatchStartError::ProjectLoad(e.to_string()))?;

    let project_root = PathBuf::from(&project_entry.metadata.root_path);
    let watch_path = PathBuf::from(&request.path);
    let canonical_root = project_root
        .canonicalize()
        .map_err(|e| WatchStartError::RootNotAccessible(e.to_string()))?;
    let canonical_watch = watch_path
        .canonicalize()
        .map_err(|e| WatchStartError::WatchNotAccessible(e.to_string()))?;
    if !canonical_watch.starts_with(&canonical_root) {
        return Err(WatchStartError::NotWithinRoot {
            watch: canonical_watch.display().to_string(),
            root: project_root.display().to_string(),
        });
    }
    if !canonical_watch.exists() {
        return Err(WatchStartError::PathNotFound(request.path.clone()));
    }

    let hot_update = state
        .engine
        .get_hot_update_coordinator(project_id)
        .await
        .map_err(|e| WatchStartError::HotCoordinator(e.to_string()))?;

    let mut coordinator = hot_update.lock().await;
    coordinator
        .start_watch(&canonical_watch)
        .await
        .map_err(|e| WatchStartError::StartWatch(e.to_string()))?;
    coordinator
        .start_event_loop()
        .await
        .map_err(|e| WatchStartError::EventLoop(e.to_string()))?;
    drop(coordinator);

    HotUpdateCoordinator::start_background_processor_from_arc(hot_update.clone()).await;

    let mut status_map = state.watch_status.write().await;
    let tracker = status_map
        .entry(project_id)
        .or_insert_with(WatchStatusTracker::new);
    tracker.start(&canonical_watch);
    drop(status_map);

    Ok(StartWatchResponse {
        success: true,
        message: "File watching started".to_string(),
        project_id,
        path: canonical_watch.to_string_lossy().into_owned(),
        extensions: request.extensions.clone(),
        debounce_ms: request.debounce_ms,
    })
}

pub async fn watch_stop(
    state: &SharedContext,
    project_id: i64,
) -> Result<StopWatchResponse, WatchStopError> {
    if project_id <= 0 {
        return Err(WatchStopError::InvalidProjectId);
    }
    let hot_update = state
        .engine
        .get_hot_update_coordinator(project_id)
        .await
        .map_err(|e| WatchStopError::HotCoordinator(e.to_string()))?;
    let mut coordinator = hot_update.lock().await;
    coordinator
        .stop_watch()
        .await
        .map_err(|e| WatchStopError::StopWatch(e.to_string()))?;
    drop(coordinator);

    let mut status_map = state.watch_status.write().await;
    if let Some(tracker) = status_map.get_mut(&project_id) {
        tracker.stop();
    }
    drop(status_map);

    Ok(StopWatchResponse {
        success: true,
        message: "File watching stopped".to_string(),
        project_id,
    })
}
