//! Per-file and per-module state transitions and file queries.

use std::path::Path;

use chrono::Utc;

use crate::index_state::{
    FileUpdateState, IndexPhase, ModuleType, ModuleUpdateState, StateTrackerError, TrackerFailure,
};

use super::UpdateStateTracker;

impl UpdateStateTracker {
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

    /// Mark a module as failed with its classified failure
    ///
    /// Retryable failures enter the exponential-backoff retry schedule and
    /// only reach the dead letter queue after the retry budget is exhausted;
    /// a permanent failure dead-letters immediately.
    pub async fn mark_failed(
        &self,
        file_path: &Path,
        module: ModuleType,
        failure: TrackerFailure,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            let prev_retry_count = state.get_module_state(module).retry_count;
            let message = failure.message.clone();
            state.mark_module_failed(module, failure);
            let new_state = state.get_module_state(module).state;

            match new_state {
                ModuleUpdateState::DeadLetter => {
                    tracing::error!(
                        file = %path_str,
                        module = %module,
                        retry_count = prev_retry_count + 1,
                        error = %message,
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
                        error = %message,
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
    ///
    /// A dead letter on any module makes the file non-resumable; every module
    /// state must be inspected because hash-map iteration order is random.
    pub async fn get_resumable_files(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| {
                s.checkpoint.is_some()
                    && !s.all_success()
                    && !s
                        .module_states
                        .values()
                        .any(|r| matches!(r.state, ModuleUpdateState::DeadLetter))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hot_update::FileChangeType;

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
            .mark_failed(
                path,
                ModuleType::Summary,
                TrackerFailure::transient("API error"),
            )
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
}
