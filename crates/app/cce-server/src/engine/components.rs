use std::sync::Arc;
use std::time::Duration;

use super::EngineError;
use super::project_cache::ProjectCache;
use cce_llm::Embedder;
use cce_metrics_infra::{
    MetricsAggregator, MetricsRegistry, ProgressTracker, QueueMetrics, RenderCache,
};
use cce_storage_bm25::Bm25Client;
use cce_storage_qdrant::QdrantClient;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::project_registry::ProjectRegistry;
use cce_storage_sqlite::repo::{CheckpointRepository, ProjectRepository};
use tokio::sync::Mutex;

impl super::CodeContextEngine {
    // --- Component access ---

    /// Get a reference to the project registry
    pub fn project_registry(&self) -> &Arc<ProjectRegistry> {
        &self.project_registry
    }

    /// Get a reference to the Qdrant client
    pub fn qdrant(&self) -> &Arc<QdrantClient> {
        &self.qdrant
    }

    /// Get a reference to the BM25 client
    pub fn bm25(&self) -> &Arc<Mutex<Bm25Client>> {
        &self.bm25
    }

    /// Get a reference to the embedder
    pub fn embedder(&self) -> &Arc<dyn Embedder> {
        &self.embedder
    }

    /// Get a reference to the SQLite metadata store
    pub fn metadata_store(&self) -> Option<&Arc<SqliteClient>> {
        self.metadata_store.as_ref()
    }

    /// Reload project configuration and recreate components
    ///
    /// This method clears all caches for the specified project and forces
    /// a reload from disk. Components will be recreated on next use with
    /// the new configuration.
    pub async fn reload_project_config(&self, project_id: i64) -> Result<(), EngineError> {
        tracing::info!(project_id, "Reloading project configuration");

        // Clear orchestrator cache
        if self.orchestrator_cache.remove(project_id).await.is_some() {
            tracing::debug!(project_id, "Cleared IndexOrchestrator cache");
        }

        // Clear searcher cache
        if self.searcher_cache.remove(project_id).await.is_some() {
            tracing::debug!(project_id, "Cleared Searcher cache");
        }

        // Clear hot update coordinator cache
        if self.hot_update_cache.remove(project_id).await.is_some() {
            tracing::debug!(project_id, "Cleared HotUpdateCoordinator cache");
        }

        // Clear operation coordinator cache
        if self
            .operation_coordinator_cache
            .remove(project_id)
            .await
            .is_some()
        {
            tracing::debug!(project_id, "Cleared OperationCoordinator cache");
        }

        // Clear relation runtime cache
        self.remove_relation_runtime(project_id).await;

        // Clear progress tracker for this project
        self.progress_tracker.remove(project_id).await;

        // Clear retry queue for this project
        self.retry_queue.remove(project_id).await;

        // Invalidate project registry cache to force reload from disk
        let _ = self
            .project_registry
            .invalidate_cache(Some(project_id))
            .await;

        tracing::info!(project_id, "Project configuration reloaded successfully");
        Ok(())
    }

    /// Get the per-project progress tracker cache
    pub fn progress_tracker(&self) -> &ProjectCache<ProgressTracker> {
        &self.progress_tracker
    }

    /// Get or create the progress tracker for a specific project
    pub async fn get_project_progress_tracker(&self, project_id: i64) -> Arc<ProgressTracker> {
        self.progress_tracker
            .get_or_insert_with(project_id, || Arc::new(ProgressTracker::new(0)))
            .await
    }

    /// Get a reference to the global metrics registry
    pub fn metrics_registry(&self) -> &Arc<MetricsRegistry> {
        &self.metrics_registry
    }

    /// Get a reference to the metrics aggregator (if enabled)
    pub fn metrics_aggregator(&self) -> Option<&Arc<MetricsAggregator<SqliteClient>>> {
        self.metrics_aggregator.as_ref()
    }

    /// Get a reference to the runtime metrics collector (if available)
    pub fn runtime_metrics(&self) -> Option<&Arc<cce_metrics_infra::RuntimeMetrics>> {
        self.runtime_metrics.as_ref()
    }

    /// Get a reference to the system metrics collector (if available)
    pub fn system_metrics(&self) -> Option<&Arc<cce_metrics_infra::SystemMetrics>> {
        self.system_metrics.as_ref()
    }

    /// Start the metrics aggregation background task (if enabled)
    pub fn start_metrics_aggregation(&self) {
        if let Some(ref aggregator) = self.metrics_aggregator {
            tracing::info!("Starting metrics aggregation background task");
            aggregator.start();
        } else {
            tracing::debug!("Metrics aggregation is disabled");
        }
    }

    /// Start metrics aggregation with automatic TTL cleanup
    pub fn start_metrics_aggregation_with_cleanup(&self) {
        if let Some(ref aggregator) = self.metrics_aggregator {
            tracing::info!("Starting metrics aggregation with TTL cleanup");
            aggregator.start_with_cleanup();
        } else {
            tracing::debug!("Metrics aggregation is disabled");
        }
    }

    /// Start runtime metrics collection background task
    pub fn start_runtime_metrics_collection(&self, interval_secs: u64) {
        if let Some(ref runtime_metrics) = self.runtime_metrics {
            let rm = runtime_metrics.clone();
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_secs(interval_secs));
                loop {
                    interval.tick().await;
                    rm.collect();
                }
            });
            tracing::info!(
                "Started runtime metrics collection (interval: {}s)",
                interval_secs
            );
        }
    }

    /// Start system metrics collection background task
    pub fn start_system_metrics_collection(&self, interval_secs: u64) {
        if let Some(ref system_metrics) = self.system_metrics {
            let sm = system_metrics.clone();
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_secs(interval_secs));
                loop {
                    interval.tick().await;
                    sm.collect();
                }
            });
            tracing::info!(
                "Started system metrics collection (interval: {}s)",
                interval_secs
            );
        }
    }

    /// Start queue backpressure metrics collection background task
    ///
    /// Periodically samples internal queue depths and exports them as gauges.
    ///
    /// Sampled queues:
    /// - `operation_queue_depth` — per-project OperationQueue (active + pending)
    /// - `pending_watch_changes` — per-project pending watch event buffer
    /// - `retry_queue_depth` — per-project RetryQueue
    pub fn start_queue_metrics(&self, interval_secs: u64) {
        let queue_metrics = Arc::new(QueueMetrics::new(&self.metrics_registry));
        let hot_cache = self.hot_update_cache.clone();
        let op_cache = self.operation_coordinator_cache.clone();
        let rq_cache = self.retry_queue.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;

                // 1. OperationQueue depth (per-project)
                op_cache
                    .for_each(|pid, op_coord| {
                        let queue_metrics = queue_metrics.clone();
                        let op_coord = op_coord.clone();
                        tokio::spawn(async move {
                            if let Ok(depth) = op_coord.queue_size().await {
                                queue_metrics.set_operation_depth(pid, depth as u64);
                            }
                        });
                    })
                    .await;

                // 2. pending_watch_changes depth (per-project)
                hot_cache
                    .for_each(|pid, hot| {
                        let queue_metrics = queue_metrics.clone();
                        let hot = hot.clone();
                        tokio::spawn(async move {
                            let depth = hot.lock().await.pending_changes_len().await;
                            queue_metrics.set_pending_changes_depth(pid, depth as u64);
                        });
                    })
                    .await;

                // 3. RetryQueue depth (per-project)
                rq_cache
                    .for_each(|pid, rq| {
                        let queue_metrics = queue_metrics.clone();
                        let rq = rq.clone();
                        tokio::spawn(async move {
                            let depth = rq.len().await;
                            queue_metrics.set_retry_depth(pid, depth as u64);
                        });
                    })
                    .await;
            }
        });

        tracing::info!(
            "Started queue metrics collection (interval: {}s)",
            interval_secs
        );
    }

    /// Start the single-core metric render cache.
    ///
    /// All Prometheus/JSON metric rendering converges into one background
    /// task that refreshes a cached snapshot every `interval_secs`; HTTP
    /// handlers serve the cached text instead of traversing the registry
    /// per request. Returns the started cache (or the existing one).
    pub async fn start_render_cache(&self, interval_secs: u64) -> Arc<RenderCache> {
        {
            let guard = self.render_cache.read().await;
            if let Some(cache) = guard.as_ref() {
                return cache.clone();
            }
        }
        let cache = Arc::new(RenderCache::new(self.metrics_registry.clone()));
        cache
            .clone()
            .start(std::time::Duration::from_secs(interval_secs.max(1)));
        let mut guard = self.render_cache.write().await;
        if let Some(existing) = guard.as_ref() {
            return existing.clone();
        }
        *guard = Some(cache.clone());
        tracing::info!("Started metric render cache (interval: {}s)", interval_secs);
        cache
    }

    /// Get the render cache if started
    pub async fn render_cache(&self) -> Option<Arc<RenderCache>> {
        self.render_cache.read().await.clone()
    }

    /// Start periodic checkpoint TTL cleanup task
    ///
    /// Scans all projects and deletes checkpoints that have been
    /// completed/failed for longer than the TTL. The TTL is taken from each
    /// project's effective configuration
    /// (`orchestrator.checkpoint_ttl_seconds`), falling back to
    /// `default_ttl_seconds` when the project configuration is unavailable.
    ///
    /// Responsibility boundary: this task is about storage hygiene for
    /// terminal (Completed/Failed) checkpoints only and never touches
    /// in_progress operations. Liveness re-arming of crashed operations is
    /// handled separately by the operation queue's heartbeat cleanup
    /// (`OperationCoordinator::initialize` /
    /// `OperationQueue::cleanup_stale_operations`), which only clears the
    /// active flag so a crashed run can be recovered by checkpoint.
    pub fn start_checkpoint_cleanup_task(&self, interval_secs: u64, default_ttl_seconds: u64) {
        let metadata_store = self.metadata_store.clone();
        let project_registry = self.project_registry.clone();
        let operation_coordinator_cache = self.operation_coordinator_cache.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            // Skew first tick slightly to avoid thundering herd at startup
            interval.tick().await;

            loop {
                interval.tick().await;

                let store = match &metadata_store {
                    Some(store) => store.clone(),
                    None => continue,
                };

                let client = store.as_ref().clone();

                let project_ids: Vec<i64> = match client
                    .with_transaction(|tx| ProjectRepository::get_all(tx))
                {
                    Ok(records) => records.into_iter().map(|r| r.id).collect(),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to list projects for checkpoint cleanup");
                        continue;
                    }
                };

                for project_id in &project_ids {
                    let ttl_seconds = match project_registry.get_or_load(*project_id).await {
                        Ok(entry) => entry.config.orchestrator.checkpoint_ttl_seconds,
                        Err(e) => {
                            tracing::warn!(project_id = *project_id, error = %e, "Failed to load project config for checkpoint TTL, using default");
                            default_ttl_seconds
                        }
                    };

                    {
                        let project_client = match store.for_project(*project_id) {
                            Ok(client) => client,
                            Err(e) => {
                                tracing::warn!(project_id = *project_id, error = %e, "Failed to open project database for checkpoint cleanup");
                                continue;
                            }
                        };
                        let conn = match project_client.write_connection() {
                            Ok(conn) => conn,
                            Err(e) => {
                                tracing::warn!(project_id = *project_id, error = %e, "Failed to get connection for checkpoint cleanup");
                                continue;
                            }
                        };

                        match CheckpointRepository::delete_expired_checkpoints(
                            &conn,
                            *project_id,
                            ttl_seconds,
                        ) {
                            Ok(count) => {
                                if count > 0 {
                                    tracing::info!(
                                        project_id = *project_id,
                                        deleted = count,
                                        "Cleaned up expired checkpoints"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!(project_id = *project_id, error = %e, "Failed to clean up expired checkpoints");
                            }
                        }
                    }

                    // Periodic stale-active cleanup: reclaim operations whose
                    // heartbeat stopped more than `heartbeat_timeout_secs` ago.
                    // The interval is 60s and timeout is 300s (5x), so a live
                    // operation (heartbeat every 60s) is never mistaken for stale.
                    // Active state is owned solely by `OperationQueue`; the
                    // checkpoint table is only the durable backing store.
                    {
                        const HEARTBEAT_TIMEOUT_SECS: i64 = 300;
                        let coordinator_opt = operation_coordinator_cache.get(*project_id).await;
                        if let Some(coordinator) = coordinator_opt {
                            match coordinator
                                .queue()
                                .cleanup_stale_operations(HEARTBEAT_TIMEOUT_SECS)
                                .await
                            {
                                Ok(count) if count > 0 => {
                                    tracing::info!(
                                        project_id = *project_id,
                                        cleared = count,
                                        "Cleaned up stale active operations (periodic)"
                                    );
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    tracing::warn!(
                                        project_id = *project_id,
                                        error = %error,
                                        "Failed to cleanup stale active operations"
                                    );
                                }
                            }
                        } else {
                            // No coordinator yet for this project; use a
                            // short-lived queue as the single writer so the
                            // active flag is never mutated outside the queue.
                            if let Ok(project_client) = store.for_project(*project_id) {
                                let temp_queue =
                                    cce_orchestrator::operation::OperationQueue::new_for_project(
                                        *project_id,
                                        project_client,
                                    );
                                match temp_queue
                                    .cleanup_stale_operations(HEARTBEAT_TIMEOUT_SECS)
                                    .await
                                {
                                    Ok(count) if count > 0 => {
                                        tracing::info!(
                                            project_id = *project_id,
                                            cleared = count,
                                            "Cleaned up stale active operations (temp queue)"
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(error) => {
                                        tracing::warn!(
                                            project_id = *project_id,
                                            error = %error,
                                            "Failed to cleanup stale active operations (temp queue)"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Export backup GC: remove stale `.export-backup-*` directories
                    // that survived a crash between `commit` and `abort`. Only backup
                    // directories are removed; live documents are never touched.
                    // `prepare_operation` cleans the backup for its own operation;
                    // this periodic sweep reclaims leftovers from crashed runs that
                    // never reached abort.
                    // Scanning uses the project's effective export output directory
                    // (`ExportConfig::output_dir`) so a custom `export.output_dir`
                    // does not leak, and expiry is based on `checkpoint.updated_at`
                    // with file `modified` as fallback (stable against `touch` /
                    // clock skew).
                    if let Ok(entry) = project_registry.get_or_load(*project_id).await {
                        let output_dir = {
                            let cfg = cce_orchestrator::ExportConfig::from_module_config(
                                &entry.config.export,
                                std::path::PathBuf::from(&entry.metadata.root_path),
                                *project_id,
                            );
                            cfg.output_dir()
                        };
                        if output_dir.exists() {
                            let read_client = store.for_project(*project_id).ok();
                            let read_conn = read_client
                                .as_ref()
                                .and_then(|client| client.read_connection().ok());
                            if let Ok(read_dir) = std::fs::read_dir(&output_dir) {
                                for dir_entry in read_dir.flatten() {
                                    let path = dir_entry.path();
                                    let Some(name) = path.file_name().and_then(|n| n.to_str())
                                    else {
                                        continue;
                                    };
                                    if !name.starts_with(".export-backup-") {
                                        continue;
                                    }
                                    let operation_id = &name[".export-backup-".len()..];
                                    let is_expired = if let Some(ref conn) = read_conn {
                                        match cce_storage_sqlite::CheckpointRepository::get_checkpoint(
                                            conn, *project_id, operation_id,
                                        ) {
                                            Ok(Some(checkpoint)) => {
                                                match chrono::DateTime::parse_from_rfc3339(
                                                    &checkpoint.updated_at,
                                                ) {
                                                    Ok(updated_at) => {
                                                        let age_secs = (chrono::Utc::now()
                                                            - updated_at.with_timezone(
                                                                &chrono::Utc,
                                                            ))
                                                        .num_seconds()
                                                        .max(0)
                                                            as u64;
                                                        age_secs > ttl_seconds
                                                    }
                                                    Err(_) => {
                                                        match dir_entry
                                                            .metadata()
                                                            .and_then(|m| m.modified())
                                                        {
                                                            Ok(modified) => modified
                                                                .elapsed()
                                                                .map(|age| {
                                                                    age.as_secs() > ttl_seconds
                                                                })
                                                                .unwrap_or(false),
                                                            Err(_) => false,
                                                        }
                                                    }
                                                }
                                            }
                                            Ok(None) => {
                                                match dir_entry
                                                    .metadata()
                                                    .and_then(|m| m.modified())
                                                {
                                                    Ok(modified) => modified
                                                        .elapsed()
                                                        .map(|age| age.as_secs() > ttl_seconds)
                                                        .unwrap_or(false),
                                                    Err(_) => false,
                                                }
                                            }
                                            Err(_) => {
                                                match dir_entry
                                                    .metadata()
                                                    .and_then(|m| m.modified())
                                                {
                                                    Ok(modified) => modified
                                                        .elapsed()
                                                        .map(|age| age.as_secs() > ttl_seconds)
                                                        .unwrap_or(false),
                                                    Err(_) => false,
                                                }
                                            }
                                        }
                                    } else {
                                        match dir_entry.metadata().and_then(|m| m.modified()) {
                                            Ok(modified) => modified
                                                .elapsed()
                                                .map(|age| age.as_secs() > ttl_seconds)
                                                .unwrap_or(false),
                                            Err(_) => false,
                                        }
                                    };
                                    if !is_expired {
                                        continue;
                                    }
                                    match std::fs::remove_dir_all(&path) {
                                        Ok(()) => {
                                            tracing::info!(
                                                project_id = *project_id,
                                                backup = %path.display(),
                                                operation_id = %operation_id,
                                                "Cleaned up stale export backup"
                                            );
                                        }
                                        Err(error) => {
                                            tracing::warn!(
                                                project_id = *project_id,
                                                backup = %path.display(),
                                                error = %error,
                                                "Failed to clean up stale export backup"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        tracing::info!(
            "Started checkpoint cleanup task (interval: {}s, default ttl: {}s)",
            interval_secs,
            default_ttl_seconds,
        );
    }

    pub fn start_generation_gc_worker(
        &self,
        interval_secs: u64,
        keep_active_generations: usize,
        stale_after_secs: u64,
    ) {
        use crate::runtime::GenerationGcWorker;

        let metadata_store = self.metadata_store.clone();
        tokio::spawn(async move {
            let store = match &metadata_store {
                Some(store) => store.clone(),
                None => {
                    tracing::warn!("Generation GC worker: no metadata store available");
                    return;
                }
            };
            let client = store.as_ref().clone();
            let config = crate::runtime::GenerationGcWorkerConfig {
                scan_interval_secs: interval_secs,
                keep_active_generations,
                stale_after_secs,
            };
            let worker = Arc::new(GenerationGcWorker::new(client, config));
            tracing::info!(
                interval_secs = interval_secs,
                keep_active_generations = keep_active_generations,
                stale_after_secs = stale_after_secs,
                "Starting background generation GC worker"
            );
            worker.start();
        });
    }
}
