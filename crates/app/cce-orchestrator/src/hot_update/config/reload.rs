//! Configuration reload management for hot update coordinator
//!
//! This module handles configuration file changes and reloads
//! for downstream processors with retry mechanism.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, MutexGuard};

use super::version::{ConfigVersion, ConfigVersionRegistry};

/// Configuration reload manager
///
/// Manages pending configuration changes and handles reload
/// operations for downstream processors.
///
/// The manager owns the pending-config-change queue and shares it with the
/// hot-update operation runtime: `handle_config_change` enqueues version-
/// checked paths, and the coordinator drains them into `ConfigChange`
/// operations that run through the full prepare/process/commit/abort
/// protocol (instead of invoking processor callbacks directly).
#[derive(Clone)]
pub struct ConfigReloadManager {
    /// Pending config changes (paths only; versions live in the registry)
    /// that need reload. Shared with the hot-update operation runtime so
    /// `run_operation` can drain them as `ConfigChange` operations.
    pending_config_changes: Arc<Mutex<Vec<PathBuf>>>,
    /// Version registry for preventing old configs from overwriting new ones
    version_registry: Arc<Mutex<ConfigVersionRegistry>>,
    /// Optional lock (wired by the coordinator) that serializes config-change
    /// processing against other operations.
    operation_lock: Option<Arc<Mutex<()>>>,
}

impl ConfigReloadManager {
    /// Create a new config reload manager
    pub fn new(_max_retries: usize) -> Self {
        Self {
            pending_config_changes: Arc::new(Mutex::new(Vec::new())),
            version_registry: Arc::new(Mutex::new(ConfigVersionRegistry::new())),
            operation_lock: None,
        }
    }

    /// Create with default settings
    pub fn new_default() -> Self {
        Self::new(3)
    }

    /// Wire an operation lock shared with the coordinator. Config-change
    /// processing acquires it to stay mutually exclusive with other
    /// coordinator-driven work.
    pub fn set_operation_lock(&mut self, lock: Arc<Mutex<()>>) {
        self.operation_lock = Some(lock);
    }

    /// Acquire the operation lock when one is wired, for the duration of
    /// config-change processing.
    pub async fn acquire_operation_lock(&self) -> Option<MutexGuard<'_, ()>> {
        match &self.operation_lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        }
    }

    /// Shared handle to the pending-change queue (used by the coordinator to
    /// wire it into the hot-update operation runtime).
    pub fn pending_config_changes(&self) -> Arc<Mutex<Vec<PathBuf>>> {
        self.pending_config_changes.clone()
    }

    /// Handle configuration file change with version check
    pub async fn handle_config_change(&self, config_path: &Path, content: &str) -> bool {
        tracing::info!(
            path = %config_path.display(),
            "Processing configuration file change"
        );

        let version = ConfigVersion::new(config_path.to_path_buf(), content);

        // Check if this is a new version
        let mut registry = self.version_registry.lock().await;
        if !registry.update(version.clone()) {
            tracing::debug!(
                path = %config_path.display(),
                "Ignoring outdated config change"
            );
            return false;
        }

        // Store the config change path for the operation pipeline.
        let mut pending = self.pending_config_changes.lock().await;
        pending.push(config_path.to_path_buf());

        tracing::info!(
            path = %config_path.display(),
            pending_count = pending.len(),
            "Configuration change queued for reload"
        );

        true
    }

    /// Check if there are pending config changes that need reload
    pub async fn has_pending_config_changes(&self) -> bool {
        let pending = self.pending_config_changes.lock().await;
        !pending.is_empty()
    }

    /// Get pending config changes
    pub async fn take_pending_config_changes(&self) -> Vec<PathBuf> {
        let mut pending = self.pending_config_changes.lock().await;
        std::mem::take(&mut *pending)
    }

    /// Re-queue config change paths after a failed processing cycle so they
    /// are retried on the next call.
    pub async fn requeue_pending(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let mut pending = self.pending_config_changes.lock().await;
        pending.extend(paths);
        tracing::info!(
            pending_count = pending.len(),
            "Re-queued failed configuration changes for retry"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_reload_manager_creation() {
        let _manager = ConfigReloadManager::new_default();
        // Manager should be created successfully with default settings
        // The actual functionality is tested in async tests below
    }

    #[tokio::test]
    async fn test_pending_config_changes() {
        let manager = ConfigReloadManager::new_default();
        assert!(!manager.has_pending_config_changes().await);

        manager
            .handle_config_change(Path::new("/test/config.toml"), "test content")
            .await;
        assert!(manager.has_pending_config_changes().await);

        let changes = manager.take_pending_config_changes().await;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], PathBuf::from("/test/config.toml"));
        assert!(!manager.has_pending_config_changes().await);
    }

    #[tokio::test]
    async fn test_outdated_config_change_is_dropped() {
        let manager = ConfigReloadManager::new_default();
        manager
            .handle_config_change(Path::new("/test/config.toml"), "v1")
            .await;
        // Same content again: the version registry rejects the duplicate.
        let accepted = manager
            .handle_config_change(Path::new("/test/config.toml"), "v1")
            .await;
        assert!(!accepted, "duplicate content must be version-rejected");
        let pending = manager.take_pending_config_changes().await;
        assert_eq!(pending.len(), 1, "only the first version is queued");
    }

    #[tokio::test]
    async fn test_requeue_pending_restores_pending_state() {
        let manager = ConfigReloadManager::new_default();

        manager
            .handle_config_change(Path::new("/test/config.toml"), "content")
            .await;
        assert!(manager.has_pending_config_changes().await);

        let pending = manager.take_pending_config_changes().await;
        assert!(!manager.has_pending_config_changes().await);

        // A failed processing cycle re-queues the paths for the next call.
        manager.requeue_pending(pending).await;
        assert!(
            manager.has_pending_config_changes().await,
            "pending changes must be re-queued"
        );

        let remaining = manager.take_pending_config_changes().await;
        assert_eq!(remaining.len(), 1, "exactly one pending change must remain");
    }

    #[tokio::test]
    async fn test_operation_lock_is_optional() {
        let manager = ConfigReloadManager::new_default();
        assert!(
            manager.acquire_operation_lock().await.is_none(),
            "no lock wired by default"
        );

        let lock = Arc::new(Mutex::new(()));
        let mut manager = ConfigReloadManager::new_default();
        manager.set_operation_lock(lock.clone());
        let guard = manager.acquire_operation_lock().await;
        assert!(guard.is_some(), "lock must be acquirable once wired");
        drop(guard);
    }
}
