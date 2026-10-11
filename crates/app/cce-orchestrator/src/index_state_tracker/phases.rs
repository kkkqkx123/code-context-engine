//! Phase checkpoints, cleanup, statistics, and reporting for the state tracker.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;

use crate::index_state::{
    Checkpoint, IndexOperationType, IndexPhase, IndexStateReport, StateTrackerError,
};

use super::{UpdateStateStats, UpdateStateTracker};

impl UpdateStateTracker {
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
    use crate::hot_update::FileChangeType;
    use crate::index_state::ModuleType;

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
}
