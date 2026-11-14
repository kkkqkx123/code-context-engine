//! Background generation GC worker
//!
//! Periodically scans all projects and removes stale generation data
//! (SQLite rows, Qdrant points, BM25 documents) that are outside the
//! retention window. Without this background task, expired generations
//! would only be cleaned up when a new index publication succeeds.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use tokio::time::sleep;
use tracing::{Level, debug, error, info, span};

use cce_orchestrator::index::{MetadataStore, in_memory_metadata_store};
use cce_storage_common::MetadataStorage;
use cce_types::StorageError;

use cce_orchestrator::index::StorageCoordinator;

/// Configuration for the generation GC worker.
#[derive(Debug, Clone)]
pub struct GenerationGcWorkerConfig {
    /// Interval between GC scans in seconds.
    pub scan_interval_secs: u64,
    /// Number of most recent active data generations to retain per project.
    pub keep_active_generations: usize,
    /// Stale threshold in seconds; generations older than this are eligible for cleanup.
    pub stale_after_secs: u64,
}

impl Default for GenerationGcWorkerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 3600,
            keep_active_generations: 2,
            stale_after_secs: 3600,
        }
    }
}

/// Background worker that periodically garbage-collects stale generations.
pub struct GenerationGcWorker {
    store: MetadataStore,
    config: GenerationGcWorkerConfig,
    running: Arc<AtomicBool>,
}

impl GenerationGcWorker {
    pub fn new(store: MetadataStore, config: GenerationGcWorkerConfig) -> Self {
        Self {
            store,
            config,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(self: Arc<Self>) {
        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            self.run().await;
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    async fn run(&self) {
        let span = span!(Level::DEBUG, "generation_gc_worker");
        let _enter = span.enter();

        info!("Starting background generation GC worker");

        while self.running.load(Ordering::SeqCst) {
            if let Err(e) = self.scan_and_cleanup().await {
                error!(error = %e, "Generation GC scan failed");
            }

            sleep(Duration::from_secs(self.config.scan_interval_secs)).await;
        }

        info!("Generation GC worker stopped");
    }

    async fn scan_and_cleanup(&self) -> Result<(), StorageError> {
        debug!("Scanning projects for generation cleanup");

        let projects = self.store.projects_all().await?;

        if projects.is_empty() {
            debug!("No projects found for generation GC");
            return Ok(());
        }

        let stale_before = chrono::Utc::now().timestamp() - self.config.stale_after_secs as i64;
        let mut cleaned = 0usize;

        for project in &projects {
            match self
                .cleanup_project(project.id, project.root_path.as_str(), stale_before)
                .await
            {
                Ok(()) => {
                    cleaned += 1;
                }
                Err(e) => {
                    warn!(
                        project_id = project.id,
                        error = %e,
                        "Failed to run generation GC for project"
                    );
                }
            }
        }

        debug!(project_count = cleaned, "Generation GC scan completed");
        Ok(())
    }

    async fn cleanup_project(
        &self,
        project_id: i64,
        root_path: &str,
        stale_before: i64,
    ) -> Result<(), StorageError> {
        let database = self.store.for_project(project_id)?;
        let group_id = cce_storage_common::generate_project_group_id(project_id, root_path);

        let coordinator = StorageCoordinator::new(project_id)
            .map_err(|e| StorageError::Query(e.to_string()))?
            .with_metadata_store(database)
            .with_project_group_id(group_id);

        coordinator
            .gc_generations(self.config.keep_active_generations, stale_before)
            .await
            .map_err(|e| StorageError::Query(e.to_string()))?;

        Ok(())
    }
}

// Required for the warn! macro used in scan_and_cleanup
use tracing::warn;

#[cfg(test)]
mod tests {
    use super::*;
    use cce_storage_common::metadb::{FileEnsureEntry, NewProjectRecord};

    async fn setup_test_db() -> MetadataStore {
        let store = in_memory_metadata_store().expect("Failed to create store");
        store
            .project_insert(&NewProjectRecord::new(
                "test".to_string(),
                "/tmp/test".to_string(),
            ))
            .await
            .expect("setup should succeed");
        for (epoch, operation) in [(1, "op-1"), (2, "op-2"), (3, "op-3")] {
            store
                .manifest_activate(1, epoch, 0, operation, None)
                .await
                .expect("setup should succeed");
        }
        for epoch in [1, 2, 3] {
            store
                .files_ensure(
                    1,
                    epoch,
                    0,
                    &[FileEnsureEntry {
                        path: format!("epoch{epoch}.rs"),
                        language: "rust".to_string(),
                        category: 0,
                        last_modified: 0,
                    }],
                )
                .await
                .expect("setup should succeed");
        }
        store
    }

    #[test]
    fn test_gc_worker_config_default() {
        let config = GenerationGcWorkerConfig::default();
        assert_eq!(config.scan_interval_secs, 3600);
        assert_eq!(config.keep_active_generations, 2);
        assert_eq!(config.stale_after_secs, 3600);
    }

    #[test]
    fn test_gc_worker_creation() {
        let store = in_memory_metadata_store().expect("Failed to create store");
        let config = GenerationGcWorkerConfig::default();
        let worker = Arc::new(GenerationGcWorker::new(store, config));
        assert!(!worker.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_gc_worker_stops_cleanly() {
        let store = in_memory_metadata_store().expect("Failed to create store");
        let config = GenerationGcWorkerConfig {
            scan_interval_secs: 1,
            ..Default::default()
        };
        let worker = Arc::new(GenerationGcWorker::new(store, config));
        assert!(!worker.running.load(Ordering::SeqCst));

        worker.clone().start();
        assert!(worker.running.load(Ordering::SeqCst));

        sleep(Duration::from_millis(100)).await;
        worker.stop();
        assert!(!worker.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_cleanup_removes_stale_generations_sqlite_only() {
        let store = setup_test_db().await;
        let group_id = cce_storage_common::generate_project_group_id(1, "/tmp/test");

        let coordinator = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(store.clone())
            .with_project_group_id(group_id);

        // stale_before = i64::MAX means everything before "now + huge" is stale,
        // which effectively targets the oldest generation since we keep 2 active.
        coordinator
            .gc_generations(2, i64::MAX)
            .await
            .expect("GC should succeed");

        let active = store
            .manifest_active(1)
            .await
            .expect("manifest read should succeed")
            .expect("an active manifest should remain");
        assert_eq!(
            active.data_epoch, 3,
            "Newest generation manifest should be retained"
        );
        assert!(
            store
                .files_page(1, 1, 100, 0)
                .await
                .expect("file page should be readable")
                .is_empty(),
            "Oldest generation rows should be recycled"
        );

        // Verify the oldest generation's data was removed
        let stale_files = store
            .files_page(1, 1, 100, 0)
            .await
            .expect("file page should be readable");
        assert!(
            stale_files.is_empty(),
            "Stale generation data should be cleaned up"
        );
        let kept_files = store
            .files_page(1, 3, 100, 0)
            .await
            .expect("file page should be readable");
        assert_eq!(
            kept_files.len(),
            1,
            "Newest generation rows should be retained"
        );
    }

    #[tokio::test]
    async fn test_cleanup_respects_retention_window_with_stale_threshold() {
        let store = setup_test_db().await;
        let group_id = cce_storage_common::generate_project_group_id(1, "/tmp/test");

        let coordinator = StorageCoordinator::new(1)
            .expect("valid project ID")
            .with_metadata_store(store.clone())
            .with_project_group_id(group_id);

        // keep=3 with stale_before=0: all 3 active generations fit within retention window,
        // so none are eligible for GC even with a permissive stale threshold.
        coordinator
            .gc_generations(3, 0)
            .await
            .expect("GC should succeed");

        let active = store
            .manifest_active(1)
            .await
            .expect("manifest read should succeed")
            .expect("an active manifest should remain");
        assert_eq!(
            active.data_epoch, 3,
            "With keep=3 the newest generation stays active"
        );
        for epoch in [1, 2, 3] {
            assert!(
                !store
                    .files_page(1, epoch, 100, 0)
                    .await
                    .expect("file page should be readable")
                    .is_empty(),
                "With keep=3 and 3 active generations, epoch {epoch} rows should be retained"
            );
        }
    }
}
