//! Index state tracker for unified indexing operations
//!
//! Provides centralized tracking of index states across all files and modules,
//! supporting full index, hot update, and incremental update operations.
//! Features independent retries per module and version control to prevent
//! old updates from overwriting newer ones.

use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use cce_storage_sqlite::SqliteClient;

use crate::hot_update::FileChangeType;
use crate::index_state::{
    Checkpoint, FileUpdateState, IndexOperationType, IndexPhase, IndexStateReport, ModuleType,
    ModuleUpdateState, StateTrackerError,
};

/// Update state tracker statistics
#[derive(Debug, Clone)]
pub struct UpdateStateStats {
    /// Total number of files being tracked
    pub total_files: usize,
    /// Number of files that are fully updated
    pub fully_updated: usize,
}

/// Update state tracker manages states for all files being updated
#[derive(Debug, Clone)]
pub struct UpdateStateTracker {
    /// Project ID for multi-project isolation
    project_id: i64,
    /// File path -> Update state mapping
    states: Arc<RwLock<HashMap<String, FileUpdateState>>>,
    /// Global version counter for generating unique version numbers
    version_counter: Arc<RwLock<u64>>,
    /// Operation ID for the current indexing operation
    current_operation_id: Arc<RwLock<Option<String>>>,
    /// Optional durable file-level state projection.
    database: Arc<RwLock<Option<Arc<SqliteClient>>>>,
}

impl UpdateStateTracker {
    /// Create a new state tracker for a specific project
    pub fn new(project_id: i64) -> Self {
        Self {
            project_id,
            states: Arc::new(RwLock::new(HashMap::new())),
            version_counter: Arc::new(RwLock::new(0)),
            current_operation_id: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach the checkpoint database used for the durable state projection.
    pub fn set_database(&self, database: Arc<SqliteClient>) {
        if let Ok(mut value) = self.database.try_write() {
            *value = Some(database);
        }
    }

    /// Restore file states for an operation after a process restart.
    pub async fn restore_operation(&self, operation_id: &str) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let (restored, max_version) =
            Self::load_projection(&database, self.project_id, operation_id);

        if restored.is_empty() {
            return;
        }
        *self.states.write().await = restored;
        *self.version_counter.write().await = max_version;
        *self.current_operation_id.write().await = Some(operation_id.to_string());
        tracing::info!(
            project_id = self.project_id,
            operation_id,
            max_version,
            "Restored durable file-level index state"
        );
    }

    fn load_projection(
        database: &Arc<SqliteClient>,
        project_id: i64,
        operation_id: &str,
    ) -> (HashMap<String, FileUpdateState>, u64) {
        let client = database.as_ref();
        let Ok(conn) = client.read_connection() else {
            return (HashMap::new(), 0);
        };
        let mut statement = match conn.prepare(
            "SELECT file_path, version, state_json
             FROM index_state_projection
             WHERE project_id = ?1 AND operation_id = ?2",
        ) {
            Ok(statement) => statement,
            Err(error) => {
                tracing::warn!(%error, "Failed to prepare durable index state recovery");
                return (HashMap::new(), 0);
            }
        };
        let rows = match statement.query_map(rusqlite::params![project_id, operation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        }) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "Failed to query durable index state recovery");
                return (HashMap::new(), 0);
            }
        };

        let mut restored = HashMap::new();
        let mut max_version = 0u64;
        for row in rows {
            let (path, raw_version, json) = match row {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(%error, "Failed to read durable index state row");
                    continue;
                }
            };
            let Ok(version) = u64::try_from(raw_version) else {
                tracing::warn!(file = %path, raw_version, "Ignoring invalid durable index state version");
                continue;
            };
            match serde_json::from_str::<FileUpdateState>(&json) {
                Ok(state) if state.project_id == project_id && state.version == version => {
                    max_version = max_version.max(version);
                    restored.insert(path, state);
                }
                Ok(_) | Err(_) => {
                    tracing::warn!(file = %path, "Ignoring incompatible durable index state row");
                }
            }
        }
        (restored, max_version)
    }

    async fn persist_state(&self, state: &FileUpdateState) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let client = database.as_ref();
        let operation_id = self
            .current_operation_id
            .read()
            .await
            .clone()
            .unwrap_or_else(|| "state".to_string());
        let state_json = match serde_json::to_string(state) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, file = %state.file_path, "Failed to serialize index state");
                return;
            }
        };
        let Ok(version) = i64::try_from(state.version) else {
            tracing::warn!(file = %state.file_path, "Index state version exceeds SQLite range");
            return;
        };
        let result = client.with_transaction(|tx| {
            tx.execute(
                "INSERT INTO index_state_projection
                    (project_id, operation_id, file_path, version, state_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(project_id, operation_id, file_path) DO UPDATE SET
                    version = excluded.version, state_json = excluded.state_json,
                    updated_at = excluded.updated_at
                 WHERE excluded.version >= index_state_projection.version",
                rusqlite::params![
                    self.project_id,
                    operation_id,
                    state.file_path,
                    version,
                    state_json,
                    Utc::now().to_rfc3339(),
                ],
            )
            .map(|_| ())
            .map_err(|error| cce_types::StorageError::Insert(error.to_string()))
        });
        if let Err(error) = result {
            tracing::warn!(%error, file = %state.file_path, "Failed to persist index state");
        }
    }

    async fn delete_persisted_state(&self, file_path: &str) {
        let database = self.database.read().await.clone();
        let Some(database) = database else { return };
        let client = database.as_ref();
        let operation_id = self
            .current_operation_id
            .read()
            .await
            .clone()
            .unwrap_or_else(|| "state".to_string());
        let result = client.with_transaction(|tx| {
            tx.execute(
                "DELETE FROM index_state_projection
                 WHERE project_id = ?1 AND operation_id = ?2 AND file_path = ?3",
                rusqlite::params![self.project_id, operation_id, file_path],
            )
            .map(|_| ())
            .map_err(|error| cce_types::StorageError::Delete(error.to_string()))
        });
        if let Err(error) = result {
            tracing::warn!(%error, file = %file_path, "Failed to delete persisted index state");
        }
    }

    /// Start a new full index operation
    ///
    /// Clears existing states and initializes for a full index.
    pub async fn start_full_index(
        &self,
        total_files: usize,
        batch_size: usize,
        root_dir: String,
    ) -> String {
        let operation_id = format!("full_{}", chrono::Utc::now().timestamp_millis());
        self.start_full_index_for_operation(operation_id, total_files, batch_size, root_dir)
            .await
    }

    /// Start a full index using the operation id persisted by CheckpointManager.
    pub async fn start_full_index_for_operation(
        &self,
        operation_id: String,
        total_files: usize,
        batch_size: usize,
        root_dir: String,
    ) -> String {
        // Clear existing states
        {
            let mut states = self.states.write().await;
            states.clear();
        }

        // Reset version counter
        {
            let mut counter = self.version_counter.write().await;
            *counter = 0;
        }

        {
            let mut op_id = self.current_operation_id.write().await;
            *op_id = Some(operation_id.clone());
        }

        tracing::info!(
            operation_id = %operation_id,
            total_files = total_files,
            batch_size = batch_size,
            root_dir = %root_dir,
            "Started full index operation"
        );

        operation_id
    }

    /// Create states for a batch of files in full index
    pub async fn create_full_index_batch(
        &self,
        files: &[std::path::PathBuf],
        batch_index: usize,
        total_batches: usize,
        batch_size: usize,
        root_dir: String,
    ) -> Vec<FileUpdateState> {
        let mut states = Vec::new();

        for file_path in files {
            let version = {
                let mut counter = self.version_counter.write().await;
                *counter += 1;
                *counter
            };

            let mut state = FileUpdateState::for_full_index(
                file_path.to_string_lossy().to_string(),
                version,
                self.project_id,
                total_batches,
                batch_size,
                root_dir.clone(),
            );

            // Set initial checkpoint
            if let Some(ref mut checkpoint) = state.checkpoint {
                checkpoint.batch_index = batch_index;
            }

            // Store state
            {
                let mut state_map = self.states.write().await;
                state_map.insert(state.file_path.clone(), state.clone());
            }

            states.push(state.clone());
            self.persist_state(&state).await;
        }

        states
    }

    /// Create a new update state for a file (hot update)
    ///
    /// Generates a new version number and initializes the state for all modules.
    /// If a state already exists for this file, it will be overwritten with the new version.
    pub async fn create_update(
        &self,
        file_path: &Path,
        change_type: FileChangeType,
    ) -> FileUpdateState {
        let path_str = file_path.to_string_lossy().to_string();

        // Generate new version number
        let version = {
            let mut counter = self.version_counter.write().await;
            *counter += 1;
            *counter
        };

        let state = FileUpdateState::new(path_str.clone(), version, change_type, self.project_id);

        // Store the state
        {
            let mut states = self.states.write().await;
            states.insert(path_str, state.clone());
        }

        self.persist_state(&state).await;

        state
    }

    /// Get current operation ID
    pub async fn current_operation_id(&self) -> Option<String> {
        let op_id = self.current_operation_id.read().await;
        op_id.clone()
    }

    /// Get the current state for a file
    pub async fn get_state(&self, file_path: &Path) -> Option<FileUpdateState> {
        let states = self.states.read().await;
        states
            .get(&file_path.to_string_lossy().to_string())
            .cloned()
    }

    /// Check if a state exists for the given file
    pub async fn has_state(&self, file_path: &Path) -> bool {
        let states = self.states.read().await;
        states.contains_key(&file_path.to_string_lossy().to_string())
    }

    /// Update the state for a specific module
    ///
    /// # Arguments
    /// * `file_path` - Path to the file
    /// * `module` - Module type to update
    /// * `new_state` - New state to set
    pub async fn update_module_state(
        &self,
        file_path: &Path,
        module: ModuleType,
        new_state: ModuleUpdateState,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            state.update_module_state(module, new_state);
            let state = state.clone();
            drop(states);
            self.persist_state(&state).await;
            Ok(())
        } else {
            Err(StateTrackerError::StateNotFound(path_str))
        }
    }

    /// Mark a module as successfully updated
    pub async fn mark_success(
        &self,
        file_path: &Path,
        module: ModuleType,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            state.mark_module_success(module);
            let state = state.clone();
            drop(states);
            self.persist_state(&state).await;
            Ok(())
        } else {
            Err(StateTrackerError::StateNotFound(path_str))
        }
    }

    /// Mark a module as failed (triggers retry logic)
    pub async fn mark_failed(
        &self,
        file_path: &Path,
        module: ModuleType,
        error: String,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            let prev_retry_count = state.get_module_state(module).retry_count;
            state.mark_module_failed(module, error.clone());
            let new_state = state.get_module_state(module).state;

            match new_state {
                ModuleUpdateState::DeadLetter => {
                    tracing::error!(
                        file = %path_str,
                        module = %module,
                        retry_count = prev_retry_count + 1,
                        error = %error,
                        "Module entered dead letter queue"
                    );
                }
                ModuleUpdateState::Retrying { next_attempt } => {
                    let wait_secs = (next_attempt - Utc::now()).num_seconds().max(0) as u64;
                    tracing::warn!(
                        file = %path_str,
                        module = %module,
                        retry_count = prev_retry_count + 1,
                        wait_secs = wait_secs,
                        error = %error,
                        "Module failed, will retry"
                    );
                }
                _ => {}
            }

            let state = state.clone();
            drop(states);
            self.persist_state(&state).await;
            Ok(())
        } else {
            Err(StateTrackerError::StateNotFound(path_str))
        }
    }

    /// Check if the given version is current for the file
    ///
    /// Returns `true` if:
    /// - No state exists for this file (new file)
    /// - The existing state has the same version
    ///
    /// Returns `false` if the existing state has a newer version (prevents old updates)
    pub async fn check_version(
        &self,
        file_path: &Path,
        expected_version: u64,
    ) -> Result<bool, StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let states = self.states.read().await;

        if let Some(state) = states.get(&path_str) {
            if state.version == expected_version {
                Ok(true)
            } else if state.version > expected_version {
                // Old update trying to overwrite newer one
                tracing::warn!(
                    file = %path_str,
                    expected_version = expected_version,
                    current_version = state.version,
                    "Version mismatch: old update trying to overwrite newer one"
                );
                Ok(false)
            } else {
                // This shouldn't happen (version went backwards)
                Err(StateTrackerError::VersionMismatch {
                    file: path_str,
                    expected: expected_version,
                    found: state.version,
                })
            }
        } else {
            // No state exists, treat as valid (new file)
            Ok(true)
        }
    }

    /// Get all pending retries that are due
    ///
    /// Returns a list of (file_path, module, next_attempt_time) for retries
    /// that are ready to be executed (next_attempt <= now).
    pub async fn get_pending_retries(
        &self,
    ) -> Vec<(String, ModuleType, chrono::DateTime<chrono::Utc>)> {
        let states = self.states.read().await;
        let now = Utc::now();

        let mut retries = Vec::new();

        for (path, state) in states.iter() {
            for (module, record) in &state.module_states {
                if let ModuleUpdateState::Retrying { next_attempt } = record.state {
                    if next_attempt <= now {
                        retries.push((path.clone(), *module, next_attempt));
                    }
                }
            }
        }

        retries
    }

    /// Get all files in dead letter queue
    pub async fn get_dead_letters(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| {
                s.module_states
                    .values()
                    .any(|r| matches!(r.state, ModuleUpdateState::DeadLetter))
            })
            .cloned()
            .collect()
    }

    /// Get all files that are currently being updated
    pub async fn get_updating_files(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| s.is_updating())
            .cloned()
            .collect()
    }

    /// Get files that can be resumed (have checkpoints and not complete)
    pub async fn get_resumable_files(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| {
                s.checkpoint.is_some()
                    && !s.all_success()
                    && !matches!(
                        s.module_states.values().next().map(|r| r.state),
                        Some(ModuleUpdateState::DeadLetter)
                    )
            })
            .cloned()
            .collect()
    }

    /// Get files in a specific phase (for full index)
    pub async fn get_files_in_phase(&self, phase: IndexPhase) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| {
                s.checkpoint
                    .as_ref()
                    .map(|c| c.phase == phase)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Update checkpoint for a file
    pub async fn update_checkpoint(
        &self,
        file_path: &Path,
        checkpoint: Checkpoint,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            state.update_checkpoint(checkpoint);
            let state = state.clone();
            drop(states);
            self.persist_state(&state).await;
            Ok(())
        } else {
            Err(StateTrackerError::StateNotFound(path_str))
        }
    }

    /// Mark a phase as complete for all files in a batch
    pub async fn mark_phase_complete(
        &self,
        file_paths: &[PathBuf],
        phase: IndexPhase,
    ) -> Result<(), StateTrackerError> {
        let mut states = self.states.write().await;

        for file_path in file_paths {
            let path_str = file_path.to_string_lossy().to_string();
            if let Some(state) = states.get_mut(&path_str) {
                if let Some(ref mut checkpoint) = state.checkpoint {
                    checkpoint.phase = phase;
                    checkpoint.timestamp = Utc::now();
                }
                state.updated_at = Utc::now();
            }
        }
        let changed: Vec<_> = file_paths
            .iter()
            .filter_map(|file_path| {
                states
                    .get(&file_path.to_string_lossy().to_string())
                    .cloned()
            })
            .collect();
        drop(states);
        for state in changed {
            self.persist_state(&state).await;
        }
        Ok(())
    }
    /// Clean up completed updates (optional maintenance operation)
    ///
    /// Removes states that are complete (all modules Success) and older than max_age.
    /// Returns the number of states removed.
    pub async fn cleanup_completed(&self, max_age: Duration) -> usize {
        let mut states = self.states.write().await;

        let now = Utc::now();

        let to_remove: Vec<String> = states
            .iter()
            .filter(|(_, state)| {
                state.all_success()
                    && (now - state.updated_at)
                        .to_std()
                        .map(|d| d >= max_age)
                        .unwrap_or(false)
            })
            .map(|(path, _)| path.clone())
            .collect();

        let count = to_remove.len();

        let removed_paths = to_remove.clone();
        for path in &to_remove {
            states.remove(path);
        }
        drop(states);
        for path in removed_paths {
            self.delete_persisted_state(&path).await;
        }

        count
    }

    /// Get update state statistics
    pub async fn get_stats(&self) -> UpdateStateStats {
        let states = self.states.read().await;
        let total_files = states.len();
        let fully_updated = states.values().filter(|s| s.all_success()).count();

        UpdateStateStats {
            total_files,
            fully_updated,
        }
    }

    /// Remove a specific file state
    pub async fn remove_state(&self, file_path: &Path) -> bool {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;
        let removed = states.remove(&path_str).is_some();
        drop(states);
        if removed {
            self.delete_persisted_state(&path_str).await;
        }
        removed
    }

    /// Get count of tracked files
    pub async fn file_count(&self) -> usize {
        let states = self.states.read().await;
        states.len()
    }

    /// Check if all tracked files are complete (all modules Success)
    pub async fn all_complete(&self) -> bool {
        let states = self.states.read().await;
        if states.is_empty() {
            return true;
        }
        states.values().all(|s| s.all_success())
    }
    /// Get summary report of current update status
    pub async fn get_report(&self) -> IndexStateReport {
        let dead_letters = self.get_dead_letters().await;
        let updating = self.get_updating_files().await;
        let resumable = self.get_resumable_files().await;

        // Build operation breakdown
        let states = self.states.read().await;
        let mut operation_breakdown: HashMap<String, usize> = HashMap::new();
        let mut phase_distribution: HashMap<IndexPhase, usize> = HashMap::new();

        for state in states.values() {
            let op_key = match &state.operation_type {
                IndexOperationType::Full { .. } => "full".to_string(),
                IndexOperationType::Hot { trigger } => format!("hot_{}", trigger),
                IndexOperationType::Incremental { .. } => "incremental".to_string(),
            };
            *operation_breakdown.entry(op_key).or_insert(0) += 1;

            if let Some(ref checkpoint) = state.checkpoint {
                *phase_distribution.entry(checkpoint.phase).or_insert(0) += 1;
            }
        }

        IndexStateReport {
            operation_breakdown,
            dead_letters: dead_letters.iter().map(|s| s.file_path.clone()).collect(),
            updating_files: updating.iter().map(|s| s.file_path.clone()).collect(),
            resumable_files: resumable.iter().map(|s| s.file_path.clone()).collect(),
            phase_distribution,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::CheckpointManager;
    use crate::operation::checkpoint::CreateCheckpointParams;
    use cce_storage_sqlite::SqliteClient;
    use cce_types::OperationKind;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_create_update() {
        let tracker = UpdateStateTracker::new(1);
        let path = Path::new("test.rs");

        let state = tracker.create_update(path, FileChangeType::Added).await;

        assert_eq!(state.file_path, "test.rs");
        assert_eq!(state.version, 1);
        assert_eq!(state.change_type, FileChangeType::Added);

        // Check that state is stored
        let stored = tracker.get_state(path).await;
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().version, 1);
    }

    #[tokio::test]
    async fn test_version_increment() {
        let tracker = UpdateStateTracker::new(1);

        let state1 = tracker
            .create_update(Path::new("file1.rs"), FileChangeType::Added)
            .await;
        let state2 = tracker
            .create_update(Path::new("file2.rs"), FileChangeType::Modified)
            .await;

        assert_eq!(state1.version, 1);
        assert_eq!(state2.version, 2);
    }

    #[tokio::test]
    async fn test_mark_success_and_failure() {
        let tracker = UpdateStateTracker::new(1);
        let path = Path::new("test.rs");

        tracker.create_update(path, FileChangeType::Modified).await;

        // Mark success
        tracker
            .mark_success(path, ModuleType::Relation)
            .await
            .unwrap();

        let state = tracker.get_state(path).await.unwrap();
        assert!(matches!(
            state.get_module_state(ModuleType::Relation).state,
            ModuleUpdateState::Success
        ));

        // Mark failure
        tracker
            .mark_failed(path, ModuleType::Summary, "API error".to_string())
            .await
            .unwrap();

        let state = tracker.get_state(path).await.unwrap();
        assert!(matches!(
            state.get_module_state(ModuleType::Summary).state,
            ModuleUpdateState::Retrying { .. }
        ));
    }

    #[tokio::test]
    async fn test_check_version() {
        let tracker = UpdateStateTracker::new(1);
        let path = Path::new("test.rs");

        let state = tracker.create_update(path, FileChangeType::Modified).await;
        let version = state.version;

        // Check correct version
        assert!(tracker.check_version(path, version).await.unwrap());

        // Check wrong version (old update)
        assert!(!tracker.check_version(path, version - 1).await.unwrap());

        // Check non-existent file
        assert!(
            tracker
                .check_version(Path::new("nonexistent.rs"), 1)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_get_stats() {
        let tracker = UpdateStateTracker::new(1);

        tracker
            .create_update(Path::new("file1.rs"), FileChangeType::Added)
            .await;
        tracker
            .create_update(Path::new("file2.rs"), FileChangeType::Modified)
            .await;

        // Mark all modules as success for file1
        for module in ModuleType::all() {
            tracker
                .mark_success(Path::new("file1.rs"), module)
                .await
                .unwrap();
        }

        let stats = tracker.get_stats().await;
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.fully_updated, 1);
    }

    #[tokio::test]
    async fn test_cleanup_completed() {
        let tracker = UpdateStateTracker::new(1);

        tracker
            .create_update(Path::new("file1.rs"), FileChangeType::Added)
            .await;

        // Mark all as success
        for module in ModuleType::all() {
            tracker
                .mark_success(Path::new("file1.rs"), module)
                .await
                .unwrap();
        }

        // Cleanup with 0 max age should remove the state
        // Wait a bit to ensure the state is older than max_age
        tokio::time::sleep(Duration::from_millis(100)).await;
        let removed = tracker.cleanup_completed(Duration::from_millis(0)).await;
        assert_eq!(removed, 1);

        assert!(tracker.get_state(Path::new("file1.rs")).await.is_none());
    }

    #[tokio::test]
    async fn durable_projection_restores_file_state_after_restart() {
        let database = Arc::new(SqliteClient::in_memory().expect("in-memory database"));
        let checkpoint_manager = CheckpointManager::new_for_project(1, database.clone());
        checkpoint_manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id: "full-operation",
                operation_type: OperationKind::FullIndex,
                root_dir: "/tmp/project",
                total_files: 1,
                batch_size: 1,
                file_list_hash: "files",
            })
            .await
            .expect("checkpoint should be created");

        let tracker = UpdateStateTracker::new(1);
        tracker.set_database(database.clone());
        tracker
            .start_full_index_for_operation(
                "full-operation".to_string(),
                1,
                1,
                "/tmp/project".to_string(),
            )
            .await;
        tracker
            .create_full_index_batch(
                &[PathBuf::from("src/lib.rs")],
                0,
                1,
                1,
                "/tmp/project".to_string(),
            )
            .await;
        tracker
            .mark_success(Path::new("src/lib.rs"), ModuleType::Relation)
            .await
            .expect("state should update");

        let restored = UpdateStateTracker::new(1);
        restored.set_database(database);
        restored.restore_operation("full-operation").await;
        let state = restored
            .get_state(Path::new("src/lib.rs"))
            .await
            .expect("state should be restored");
        assert_eq!(state.version, 1);
        assert!(matches!(
            state.get_module_state(ModuleType::Relation).state,
            ModuleUpdateState::Success
        ));
    }

    #[tokio::test]
    async fn test_get_dead_letters() {
        let tracker = UpdateStateTracker::new(1);

        tracker
            .create_update(Path::new("file1.rs"), FileChangeType::Modified)
            .await;

        // Mark as failed 3 times to enter dead letter
        for _ in 0..3 {
            tracker
                .mark_failed(
                    Path::new("file1.rs"),
                    ModuleType::Summary,
                    "error".to_string(),
                )
                .await
                .unwrap();
        }

        let dead_letters = tracker.get_dead_letters().await;
        assert_eq!(dead_letters.len(), 1);
        assert_eq!(dead_letters[0].file_path, "file1.rs");
    }
}
