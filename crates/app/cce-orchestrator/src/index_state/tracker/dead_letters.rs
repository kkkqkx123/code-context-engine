//! Dead-letter queue and truncate-retry handling for the state tracker.

use std::path::Path;

use chrono::Utc;

use crate::index_state::{FileUpdateState, ModuleType, ModuleUpdateState, StateTrackerError};

use super::UpdateStateTracker;

impl UpdateStateTracker {
    /// Get all files in dead letter queue
    ///
    /// Files whose dead-letter modules are all acknowledged are excluded:
    /// they were consciously written off by an operator.
    pub async fn get_dead_letters(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|s| {
                s.module_states
                    .values()
                    .any(|r| matches!(r.state, ModuleUpdateState::DeadLetter) && !r.acknowledged)
            })
            .cloned()
            .collect()
    }

    /// Acknowledge the dead letter(s) of a file so they stop participating in
    /// retry passes. `module = None` acknowledges every dead-lettered module.
    /// Returns the number of module records that were updated.
    pub async fn acknowledge_dead_letter(
        &self,
        file_path: &Path,
        module: Option<ModuleType>,
    ) -> Result<usize, StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        let Some(state) = states.get_mut(&path_str) else {
            return Err(StateTrackerError::StateNotFound(path_str));
        };

        let mut updated = 0;
        for (m, record) in state.module_states.iter_mut() {
            if matches!(record.state, ModuleUpdateState::DeadLetter)
                && !record.acknowledged
                && module.is_none_or(|target| *m == target)
            {
                record.acknowledged = true;
                updated += 1;
            }
        }
        state.updated_at = Utc::now();
        Ok(updated)
    }

    /// Reset a dead-letter module back to `Failed` so the regular retry path
    /// picks it up again (e.g. after an external cause was fixed). Returns an
    /// error when the file or the module is not currently dead-lettered.
    pub async fn reset_dead_letter(
        &self,
        file_path: &Path,
        module: ModuleType,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        let Some(state) = states.get_mut(&path_str) else {
            return Err(StateTrackerError::StateNotFound(path_str));
        };

        let Some(record) = state.module_states.get_mut(&module) else {
            return Err(StateTrackerError::StateNotFound(path_str));
        };
        if !matches!(record.state, ModuleUpdateState::DeadLetter) {
            return Err(StateTrackerError::StateNotFound(path_str));
        }

        record.state = ModuleUpdateState::Failed;
        record.acknowledged = false;
        state.updated_at = Utc::now();
        Ok(())
    }

    /// Get files whose Embedding module is eligible for a lossy truncate-retry
    /// (dead letter, never truncated before).
    pub async fn get_truncate_retry_candidates(&self) -> Vec<FileUpdateState> {
        let states = self.states.read().await;

        states
            .values()
            .filter(|state| {
                state
                    .module_states
                    .get(&ModuleType::Embedding)
                    .is_some_and(|r| r.is_truncate_candidate() && !r.acknowledged)
            })
            .cloned()
            .collect()
    }

    /// Record that a module already went through a lossy truncate-retry so it
    /// is excluded from future truncate attempts.
    pub async fn set_module_truncated(
        &self,
        file_path: &Path,
        module: ModuleType,
    ) -> Result<(), StateTrackerError> {
        let path_str = file_path.to_string_lossy().to_string();
        let mut states = self.states.write().await;

        if let Some(state) = states.get_mut(&path_str) {
            state.set_module_truncated(module);
            let state = state.clone();
            drop(states);
            self.persist_state(&state).await;
            Ok(())
        } else {
            Err(StateTrackerError::StateNotFound(path_str))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hot_update::FileChangeType;
    use crate::index_state::{TOKEN_LIMIT_ERROR_CODE, TrackerFailure};

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
                    TrackerFailure::transient("error"),
                )
                .await
                .unwrap();
        }

        let dead_letters = tracker.get_dead_letters().await;
        assert_eq!(dead_letters.len(), 1);
        assert_eq!(dead_letters[0].file_path, "file1.rs");
    }

    #[tokio::test]
    async fn test_truncate_retry_candidates() {
        let tracker = UpdateStateTracker::new(1);
        let path = Path::new("file1.rs");

        tracker.create_update(path, FileChangeType::Modified).await;
        assert!(tracker.get_truncate_retry_candidates().await.is_empty());

        // A permanent token-limit dead letter -> becomes a candidate at once
        tracker
            .mark_failed(
                path,
                ModuleType::Embedding,
                TrackerFailure {
                    message: "Token limit exceeded: 9000 > 8192".to_string(),
                    code: Some(TOKEN_LIMIT_ERROR_CODE.to_string()),
                    retryable: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            tracker
                .get_truncate_retry_candidates()
                .await
                .iter()
                .map(|s| s.file_path.clone())
                .collect::<Vec<_>>(),
            vec!["file1.rs".to_string()]
        );

        // A token-limit dead letter on a non-Embedding module -> never a candidate
        tracker
            .create_update(Path::new("file2.rs"), FileChangeType::Modified)
            .await;
        for _ in 0..3 {
            tracker
                .mark_failed(
                    Path::new("file2.rs"),
                    ModuleType::Bm25,
                    TrackerFailure {
                        message: "Token limit exceeded: 9000 > 8192".to_string(),
                        code: Some(TOKEN_LIMIT_ERROR_CODE.to_string()),
                        retryable: false,
                    },
                )
                .await
                .expect("state exists");
        }
        assert_eq!(
            tracker
                .get_truncate_retry_candidates()
                .await
                .iter()
                .map(|s| s.file_path.clone())
                .collect::<Vec<_>>(),
            vec!["file1.rs".to_string()]
        );

        // A non-token-limit dead letter is never a truncate candidate
        tracker
            .create_update(Path::new("file3.rs"), FileChangeType::Modified)
            .await;
        for _ in 0..3 {
            tracker
                .mark_failed(
                    Path::new("file3.rs"),
                    ModuleType::Embedding,
                    TrackerFailure::transient("connection reset"),
                )
                .await
                .expect("state exists");
        }
        assert_eq!(
            tracker
                .get_truncate_retry_candidates()
                .await
                .iter()
                .map(|s| s.file_path.clone())
                .collect::<Vec<_>>(),
            vec!["file1.rs".to_string()]
        );

        // After marking truncated it leaves the candidate set permanently
        tracker
            .set_module_truncated(path, ModuleType::Embedding)
            .await
            .expect("state exists");
        assert!(tracker.get_truncate_retry_candidates().await.is_empty());
    }
}
