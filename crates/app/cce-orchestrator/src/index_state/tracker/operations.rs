//! Full-index and hot-update operation lifecycle for the state tracker.

use std::path::Path;

use crate::hot_update::FileChangeType;
use crate::index_state::FileUpdateState;

use super::UpdateStateTracker;

impl UpdateStateTracker {
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
        self.start_full_index_for_operation(
            operation_id.clone(),
            total_files,
            batch_size,
            root_dir,
        )
        .await;
        operation_id
    }

    /// Start a full index using the operation id persisted by CheckpointManager.
    ///
    /// The tracker adopts the caller-supplied id verbatim as the projection
    /// key; there is nothing to hand back.
    pub async fn start_full_index_for_operation(
        &self,
        operation_id: String,
        total_files: usize,
        batch_size: usize,
        root_dir: String,
    ) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
