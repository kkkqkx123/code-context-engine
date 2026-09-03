//! Operation coordinator for centralized operation management
//!
//! Provides a single entry point for all indexing operations:
//! - Coordinates full-index, hot-update, and incremental operations
//! - Manages operation queue and scheduling
//! - Handles checkpoint persistence and recovery
//! - Ensures no concurrent conflicts between operations
//! - Manages operation lifecycle and state transitions

use std::sync::Arc;
use tracing::{debug, info, trace, warn};

use cce_storage_sqlite::SqliteClient;
use cce_types::error::common::NotFoundError;
use cce_types::{OperationKind, StorageError};

use crate::index::FileIndexer;

use super::checkpoint::CheckpointManager;
use super::context::{InitializationPhase, InitializationResult, OperationPhase};
use super::queue::{ActiveOperation, OperationPriority, OperationQueue, PendingOperation};
use super::recovery::RecoveryManager;

/// Coordinator for all indexing operations
///
/// This coordinator manages the complete lifecycle of operations:
/// 1. **Enqueue**: Incoming operation requests
/// 2. **Activate**: Move from queue to active execution
/// 3. **Monitor**: Track heartbeats and detect stale operations
/// 4. **Complete**: Transition to terminal states
/// 5. **Recover**: Resume from crashes
pub struct OperationCoordinator {
    /// Project ID: 0 for global project, >0 for user projects
    project_id: i64,

    /// Operation queue for task dispatch
    queue: Arc<OperationQueue>,
    /// Checkpoint manager for persistence
    checkpoint_manager: Arc<CheckpointManager>,
    /// Recovery manager for resumption (optional)
    recovery_manager: Option<Arc<RecoveryManager>>,
    /// Heartbeat timeout in seconds (for detecting stale operations)
    heartbeat_timeout_secs: i64,
    /// Recovery freshness window in seconds.
    ///
    /// `recover_unfinished_operations` only replays in_progress checkpoints
    /// whose `updated_at` is newer than this window; older ones are marked
    /// Failed (`last_error = "stale recovery skipped"`) instead of being
    /// replayed. `None` replays every unfinished operation (legacy behavior).
    recovery_freshness_secs: Option<u64>,
}

impl OperationCoordinator {
    /// Create a new operation coordinator for a specific project
    /// project_id must be > 0 (no global/default projects allowed)
    pub fn new_for_project(
        project_id: i64,
        db: Arc<SqliteClient>,
    ) -> Result<Self, cce_types::error::ConfigError> {
        if project_id <= 0 {
            return Err(cce_types::error::ConfigError::invalid_project_id(
                project_id,
            ));
        }
        let checkpoint_manager =
            Arc::new(CheckpointManager::new_for_project(project_id, db.clone()));
        let queue = Arc::new(OperationQueue::new_for_project(project_id, db.clone()));

        Ok(Self {
            project_id,
            queue,
            checkpoint_manager,
            recovery_manager: None,
            heartbeat_timeout_secs: 300,
            recovery_freshness_secs: None,
        })
    }

    /// Create a new operation coordinator with recovery support
    /// project_id must be > 0 (no global/default projects allowed)
    pub fn with_recovery_for_project(
        project_id: i64,
        db: Arc<SqliteClient>,
        file_indexer: Arc<FileIndexer>,
    ) -> Result<Self, cce_types::error::ConfigError> {
        if project_id <= 0 {
            return Err(cce_types::error::ConfigError::invalid_project_id(
                project_id,
            ));
        }
        let checkpoint_manager =
            Arc::new(CheckpointManager::new_for_project(project_id, db.clone()));
        let recovery_manager = Arc::new(RecoveryManager::new(
            file_indexer,
            checkpoint_manager.clone(),
        ));
        let queue = Arc::new(OperationQueue::new_for_project(project_id, db.clone()));

        Ok(Self {
            project_id,
            queue,
            checkpoint_manager,
            recovery_manager: Some(recovery_manager),
            heartbeat_timeout_secs: 300,
            recovery_freshness_secs: None,
        })
    }

    /// Set heartbeat timeout (in seconds)
    pub fn with_heartbeat_timeout(mut self, timeout_secs: i64) -> Self {
        self.heartbeat_timeout_secs = timeout_secs;
        self
    }

    /// Set the recovery freshness window (in seconds).
    ///
    /// In_progress checkpoints whose `updated_at` falls outside the window
    /// are marked Failed instead of being replayed on startup. Typically
    /// wired from `orchestrator.checkpoint_ttl_seconds`.
    pub fn with_recovery_freshness(mut self, freshness_secs: u64) -> Self {
        self.recovery_freshness_secs = Some(freshness_secs);
        self
    }

    /// Get project ID
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Initialize coordinator on startup
    ///
    /// This should be called once when the application starts to:
    /// 1. Load any persisted active operation
    /// 2. Clean up stale operations
    /// 3. Recover unfinished operations
    pub async fn initialize(&self) -> Result<u32, StorageError> {
        info!("Initializing operation coordinator");

        // Step 1: Load persisted active operation
        if let Some(active) = self.queue.load_persisted_active().await? {
            info!(
                operation_id = %active.operation_id,
                "Loaded persisted active operation from database"
            );
        }

        // Step 2: Clean up stale operations (no heartbeat for specified seconds)
        let stale_count = self
            .queue
            .cleanup_stale_operations(self.heartbeat_timeout_secs)
            .await?;
        if stale_count > 0 {
            warn!(
                stale_count = stale_count,
                timeout_secs = self.heartbeat_timeout_secs,
                "Cleaned up stale operations from previous crashes"
            );
        }

        // Step 3: Recover unfinished operations
        let recovered_count = self.recover_unfinished_operations().await?;

        info!(
            recovered_count = recovered_count,
            stale_count = stale_count,
            "Operation coordinator initialization complete"
        );

        Ok(recovered_count)
    }

    /// Initialize coordinator with detailed tracking and state machine
    ///
    /// This is an enhanced version of `initialize()` that provides:
    /// - Explicit state machine for initialization phases
    /// - Detailed progress tracking
    /// - Comprehensive error reporting
    ///
    /// Returns `InitializationResult` containing:
    /// - Final phase reached
    /// - Counts of loaded/cleaned/recovered operations
    /// - Detailed operation log
    /// - Error information if failed
    pub async fn initialize_with_tracking(&self) -> Result<InitializationResult, StorageError> {
        let start = std::time::Instant::now();
        let mut details = Vec::new();
        let mut error_msg: Option<String> = None;
        let mut final_phase;
        let mut loaded_count: usize = 0;
        let mut cleaned_count: usize = 0;
        let mut recovered_count: usize = 0;

        info!("Starting coordinator initialization with tracking");

        // Phase 1: Loading Persisted Operations
        final_phase = InitializationPhase::LoadingPersisted;
        match self.queue.load_persisted_active().await {
            Ok(Some(active)) => {
                loaded_count = 1;
                details.push(format!(
                    "✓ Loaded active operation: {} (type: {})",
                    active.operation_id, active.operation_type
                ));
                info!(
                    operation_id = %active.operation_id,
                    "Successfully loaded persisted active operation"
                );
            }
            Ok(None) => {
                details.push("✓ No persisted active operations found".to_string());
                debug!("No persisted active operations to load");
            }
            Err(e) => {
                error_msg = Some(format!("Failed to load persisted operations: {}", e));
                warn!(error = %e, "Failed to load persisted operations");
                final_phase = InitializationPhase::Failed;
            }
        }

        // If loading failed, return early
        if error_msg.is_some() {
            let duration_ms = start.elapsed().as_millis();
            return Ok(InitializationResult {
                final_phase,
                loaded_count: loaded_count as u32,
                cleaned_count: cleaned_count as u32,
                recovered_count: recovered_count as u32,
                duration_ms,
                details,
                error: error_msg,
            });
        }

        // Phase 2: Cleaning Stale Operations
        final_phase = InitializationPhase::CleaningStale;
        match self
            .queue
            .cleanup_stale_operations(self.heartbeat_timeout_secs)
            .await
        {
            Ok(count) => {
                cleaned_count = count;
                if count > 0 {
                    details.push(format!(
                        "✓ Cleaned {} stale operations (timeout: {}s)",
                        count, self.heartbeat_timeout_secs
                    ));
                    warn!(
                        stale_count = count,
                        timeout_secs = self.heartbeat_timeout_secs,
                        "Cleaned up stale operations from previous crashes"
                    );
                } else {
                    details.push("✓ No stale operations to clean".to_string());
                    debug!("No stale operations found");
                }
            }
            Err(e) => {
                error_msg = Some(format!("Failed to clean stale operations: {}", e));
                warn!(error = %e, "Failed to clean stale operations");
                final_phase = InitializationPhase::Failed;
            }
        }

        // If cleaning failed, return early
        if error_msg.is_some() {
            let duration_ms = start.elapsed().as_millis();
            return Ok(InitializationResult {
                final_phase,
                loaded_count: loaded_count as u32,
                cleaned_count: cleaned_count as u32,
                recovered_count: recovered_count as u32,
                duration_ms,
                details,
                error: error_msg,
            });
        }

        // Phase 3: Recovering Unfinished Operations
        final_phase = InitializationPhase::RecoveringUnfinished;
        match self.recover_unfinished_operations().await {
            Ok(count) => {
                recovered_count = count as usize;
                if count > 0 {
                    details.push(format!("✓ Recovered {} unfinished operations", count));
                    info!(
                        recovered_count = count,
                        "Successfully recovered unfinished operations"
                    );
                } else {
                    details.push("✓ No unfinished operations to recover".to_string());
                    debug!("No unfinished operations found");
                }
            }
            Err(e) => {
                error_msg = Some(format!("Failed to recover unfinished operations: {}", e));
                warn!(error = %e, "Failed to recover unfinished operations");
                final_phase = InitializationPhase::Failed;
            }
        }

        // Mark as completed if no error occurred
        if error_msg.is_none() {
            final_phase = InitializationPhase::Completed;
        }

        let duration_ms = start.elapsed().as_millis();

        let result = InitializationResult {
            final_phase,
            loaded_count: loaded_count as u32,
            cleaned_count: cleaned_count as u32,
            recovered_count: recovered_count as u32,
            duration_ms,
            details,
            error: error_msg,
        };

        match final_phase {
            InitializationPhase::Completed => {
                info!(?result, "Coordinator initialization completed successfully");
            }
            InitializationPhase::Failed => {
                warn!(
                    error = ?result.error,
                    "Coordinator initialization failed"
                );
            }
            _ => {
                warn!(phase = ?final_phase, "Unexpected initialization phase");
            }
        }

        Ok(result)
    }

    /// Request a full-index operation
    ///
    /// If a full-index is already active, this returns early.
    /// If a hot-update is active, this enqueues the full-index with highest priority.
    pub async fn request_full_index(
        &self,
        operation_id: String,
        root_dir: String,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            root_dir = %root_dir,
            "Requesting full-index operation"
        );

        // Check if already active
        if self.queue.has_active_full_index().await? {
            warn!(
                operation_id = %operation_id,
                "Full-index already active, ignoring request"
            );
            return Ok(());
        }

        // Enqueue with highest priority
        self.queue
            .enqueue(
                operation_id.clone(),
                OperationKind::FullIndex,
                root_dir.clone(),
                OperationPriority::FullIndex,
            )
            .await?;

        trace!(
            operation_id = %operation_id,
            "Full-index operation enqueued"
        );

        Ok(())
    }

    /// Request a hot-update operation
    ///
    /// If a full-index is active, this enqueues the hot-update to run after.
    /// Otherwise, enqueues immediately.
    pub async fn request_hot_update(
        &self,
        operation_id: String,
        root_dir: String,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            root_dir = %root_dir,
            "Requesting hot-update operation"
        );

        // Enqueue with lower priority than full-index
        self.queue
            .enqueue(
                operation_id.clone(),
                OperationKind::HotUpdate,
                root_dir.clone(),
                OperationPriority::HotUpdate,
            )
            .await?;

        trace!(
            operation_id = %operation_id,
            "Hot-update operation enqueued"
        );

        Ok(())
    }

    /// Request an incremental update operation
    pub async fn request_incremental(
        &self,
        operation_id: String,
        root_dir: String,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            root_dir = %root_dir,
            "Requesting incremental operation"
        );

        self.queue
            .enqueue(
                operation_id.clone(),
                OperationKind::Incremental,
                root_dir.clone(),
                OperationPriority::Incremental,
            )
            .await?;

        trace!(
            operation_id = %operation_id,
            "Incremental operation enqueued"
        );

        Ok(())
    }

    /// Execute the next operation
    ///
    /// This is the primary scheduling method. It:
    /// 1. Checks if any operation is currently active
    /// 2. Validates the next operation can proceed
    /// 3. Dequeues and starts execution
    ///
    /// Returns the operation to execute, or None if queue is empty or operation is active.
    pub async fn execute_next_operation(&self) -> Result<Option<PendingOperation>, StorageError> {
        trace!("Attempting to fetch next operation for execution");

        // Check if operation can proceed (respects full-index exclusivity)
        if !self.can_execute_next().await? {
            trace!("Cannot execute next operation: constraints not met");
            return Ok(None);
        }

        self.queue.dequeue().await
    }

    /// Check if next operation can be executed based on active operation constraints
    ///
    /// Returns false if:
    /// - A full-index operation is active (blocks everything except itself)
    /// - Some other constraint prevents execution
    async fn can_execute_next(&self) -> Result<bool, StorageError> {
        if let Some(active) = self.queue.get_active().await? {
            if active.operation_type == OperationKind::FullIndex {
                trace!(
                    active_op = %active.operation_id,
                    "Cannot execute: full-index is active"
                );
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Send heartbeat for the currently active operation
    ///
    /// Call this periodically during operation execution to prevent timeout.
    pub async fn heartbeat(&self, operation_id: &str) -> Result<(), StorageError> {
        self.queue.heartbeat(operation_id).await?;
        trace!(operation_id = %operation_id, "Heartbeat sent");
        Ok(())
    }

    /// Mark the current operation as completed
    ///
    /// This updates both the operation queue and the checkpoint status in the database,
    /// preventing the operation from being re-recovered on process restart.
    ///
    /// # Errors
    /// Returns an error if either the queue update or checkpoint update fails.
    /// If checkpoint update fails after queue update succeeds, the operation queue
    /// will be marked as complete but checkpoint state may be inconsistent.
    pub async fn complete_operation(&self) -> Result<Option<ActiveOperation>, StorageError> {
        trace!("Marking active operation as completed");

        if let Some(completed) = self.queue.complete_active().await? {
            // Update checkpoint status in database to prevent re-recovery
            // Explicitly propagate errors to caller for proper error handling
            self.checkpoint_manager
                .mark_operation_completed(&completed.operation_id)
                .await?;

            // Clean up ParsedFile artifacts for this operation
            if let Some(ref recovery_manager) = self.recovery_manager {
                if let Err(e) = recovery_manager
                    .cleanup_operation_artifacts(&completed.operation_id)
                    .await
                {
                    warn!(
                        operation_id = %completed.operation_id,
                        error = %e,
                        "Failed to clean up operation artifacts after completion"
                    );
                    // Don't fail the operation completion on cleanup error
                }
            }

            trace!(
                operation_id = %completed.operation_id,
                "Operation successfully marked as completed"
            );
            Ok(Some(completed))
        } else {
            trace!("No active operation to complete");
            Ok(None)
        }
    }

    /// Get the currently active operation
    pub async fn get_active_operation(&self) -> Result<Option<ActiveOperation>, StorageError> {
        self.queue.get_active().await
    }

    /// Get the checkpoint manager for checkpoint operations
    pub fn checkpoint_manager(&self) -> Arc<CheckpointManager> {
        self.checkpoint_manager.clone()
    }

    /// Get the recovery manager for recovery operations
    pub fn recovery_manager(&self) -> Option<Arc<RecoveryManager>> {
        self.recovery_manager.clone()
    }

    /// Expose the underlying operation queue for direct active-flag management.
    pub fn queue(&self) -> Arc<OperationQueue> {
        self.queue.clone()
    }

    /// Clear active flag for a specific operation id (failure path).
    pub async fn clear_active_by_operation(&self, operation_id: &str) -> Result<(), StorageError> {
        self.queue.clear_active_by_operation(operation_id).await
    }

    /// Get queue statistics
    pub async fn queue_stats(
        &self,
    ) -> Result<(Option<ActiveOperation>, Vec<PendingOperation>), StorageError> {
        let active = self.queue.get_active().await?;
        let pending = self.queue.peek_pending().await?;
        Ok((active, pending))
    }

    pub async fn queue_size(&self) -> Result<usize, StorageError> {
        self.queue.queue_size().await
    }

    /// Check if there's a pending hot-update operation
    pub async fn has_pending_hot_update(&self) -> Result<bool, StorageError> {
        self.queue.has_pending_hot_update().await
    }

    /// Wait for all operations to complete (blocks until queue is empty)
    ///
    /// Used for graceful shutdown or synchronization.
    pub async fn wait_for_completion(&self, timeout_secs: u64) -> Result<(), StorageError> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            let size = self.queue.queue_size().await?;
            if size == 0 {
                info!("All operations completed");
                return Ok(());
            }

            if start.elapsed() > timeout {
                warn!(
                    queue_size = size,
                    timeout_secs = timeout_secs,
                    "Timeout waiting for operations to complete"
                );
                return Err(StorageError::connection(format!(
                    "Operation completion timeout after {} seconds",
                    timeout_secs
                )));
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    /// Recover unfinished operations from checkpoint (for process restart recovery)
    ///
    /// Only in_progress checkpoints whose `updated_at` is within the
    /// freshness window (see `with_recovery_freshness`) are replayed.
    /// Older ones are marked Failed with `last_error = "stale recovery
    /// skipped"` so they are never replayed on a later restart.
    pub async fn recover_unfinished_operations(&self) -> Result<u32, StorageError> {
        info!("Recovering unfinished operations from checkpoint");

        let unfinished = self.checkpoint_manager.get_unfinished_operations().await?;
        let mut recovered_count = 0;

        for checkpoint in unfinished {
            if self.is_stale_for_recovery(&checkpoint) {
                warn!(
                    operation_id = %checkpoint.operation_id,
                    operation_type = %checkpoint.operation_type,
                    updated_at = %checkpoint.updated_at,
                    "Checkpoint is older than the recovery freshness window, skipping replay"
                );
                self.checkpoint_manager
                    .mark_operation_failed(&checkpoint.operation_id, "stale recovery skipped")
                    .await?;
                continue;
            }

            let op_kind: OperationKind =
                checkpoint.operation_type.parse().map_err(|e: String| {
                    StorageError::validation(format!("Invalid operation_type: {e}"))
                })?;
            // HotUpdate and ConfigChange are recovered via their dedicated
            // checkpoint-direct paths, not the shared pending queue. Enqueueing
            // them here would create orphan pending entries that no consumer
            // drains, and could overflow the bounded queue.
            if matches!(
                op_kind,
                OperationKind::HotUpdate | OperationKind::ConfigChange
            ) {
                trace!(
                    operation_id = %checkpoint.operation_id,
                    operation_type = %checkpoint.operation_type,
                    "Skipping queue replay for hot-update/config-change checkpoint (direct recovery)"
                );
                continue;
            }
            let priority = match op_kind {
                OperationKind::FullIndex => OperationPriority::FullIndex,
                OperationKind::Incremental => OperationPriority::Incremental,
                OperationKind::HotUpdate => OperationPriority::HotUpdate,
                OperationKind::ConfigChange => OperationPriority::HotUpdate,
            };

            self.queue
                .enqueue(
                    checkpoint.operation_id.clone(),
                    op_kind,
                    checkpoint.root_dir.clone(),
                    priority,
                )
                .await?;

            recovered_count += 1;
            trace!(
                operation_id = %checkpoint.operation_id,
                operation_type = %checkpoint.operation_type,
                "Recovered unfinished operation"
            );
        }

        info!(
            recovered_count = recovered_count,
            "Finished recovering unfinished operations"
        );

        Ok(recovered_count)
    }

    /// Whether an in_progress checkpoint is outside the recovery freshness
    /// window and must be skipped (and marked Failed) instead of replayed.
    fn is_stale_for_recovery(
        &self,
        checkpoint: &cce_storage_sqlite::types::CheckpointRecord,
    ) -> bool {
        let Some(freshness_secs) = self.recovery_freshness_secs else {
            return false;
        };
        let Ok(updated_at) = chrono::DateTime::parse_from_rfc3339(&checkpoint.updated_at) else {
            // An unparseable timestamp cannot be proven fresh: treat it as
            // stale rather than replaying an operation of unknown age.
            return true;
        };
        (chrono::Utc::now() - updated_at.with_timezone(&chrono::Utc)).num_seconds()
            > freshness_secs as i64
    }

    /// Get current operation phase of given operation
    ///
    /// Useful for monitoring and debugging operation states.
    pub async fn get_operation_phase(
        &self,
        operation_id: &str,
    ) -> Result<OperationPhase, StorageError> {
        // Check if active
        if let Some(active) = self.queue.get_active().await? {
            if active.operation_id == operation_id {
                return Ok(OperationPhase::Active);
            }
        }

        // Check if pending
        let pending = self.queue.peek_pending().await?;
        if pending.iter().any(|op| op.operation_id == operation_id) {
            return Ok(OperationPhase::Queued);
        }

        // Check checkpoint for terminal states
        if let Ok(Some(checkpoint)) = self.checkpoint_manager.get_checkpoint(operation_id).await {
            use cce_storage_sqlite::types::CheckpointStatus;
            match checkpoint.status {
                CheckpointStatus::Completed => return Ok(OperationPhase::Completed),
                CheckpointStatus::Failed => return Ok(OperationPhase::Failed),
                CheckpointStatus::InProgress => return Ok(OperationPhase::Active),
            }
        }

        // Not found
        Err(NotFoundError::new(format!("Operation not found: {}", operation_id)).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_storage_sqlite::CheckpointRepository;
    use cce_storage_sqlite::types::{CheckpointRecord, CheckpointStatus};

    fn make_checkpoint(
        operation_id: &str,
        updated_at: chrono::DateTime<chrono::Utc>,
        root_dir: &str,
    ) -> CheckpointRecord {
        CheckpointRecord {
            id: None,
            project_id: 1,
            operation_id: operation_id.to_string(),
            operation_type: OperationKind::FullIndex.to_string(),
            root_dir: root_dir.to_string(),
            total_files: 10,
            batch_size: 5,
            current_batch_index: 1,
            current_phase: "Parsing".to_string(),
            file_list_hash: Some("hash".to_string()),
            created_at: updated_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            last_error: None,
            failure_count: 0,
            status: CheckpointStatus::InProgress,
            active_flag: true,
            priority: 3,
            last_heartbeat: Some(updated_at.to_rfc3339()),
            failed_at: None,
        }
    }

    fn insert_checkpoint(db: &Arc<SqliteClient>, record: &CheckpointRecord) {
        let mut conn = db
            .write_connection()
            .expect("Failed to get write connection");
        let tx = conn.transaction().expect("Failed to create transaction");
        CheckpointRepository::create_checkpoint(&tx, record.project_id, record)
            .expect("Failed to insert checkpoint");
        tx.commit().expect("Failed to commit transaction");
    }

    #[tokio::test]
    async fn test_recover_unfinished_operations_filters_stale() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let coordinator = OperationCoordinator::new_for_project(1, db.clone())
            .expect("Failed to create coordinator")
            .with_recovery_freshness(3600);

        let now = chrono::Utc::now();
        insert_checkpoint(
            &db,
            &make_checkpoint(
                "fresh-op",
                now - chrono::Duration::seconds(60),
                "/project-a",
            ),
        );
        insert_checkpoint(
            &db,
            &make_checkpoint(
                "stale-op",
                now - chrono::Duration::seconds(7200),
                "/project-b",
            ),
        );

        let recovered = coordinator
            .recover_unfinished_operations()
            .await
            .expect("Failed to recover operations");
        assert_eq!(recovered, 1, "Only the fresh operation must be replayed");

        // The stale operation is marked Failed and must not be replayed.
        let stale = coordinator
            .checkpoint_manager()
            .get_checkpoint("stale-op")
            .await
            .expect("Failed to load stale checkpoint")
            .expect("Stale checkpoint must exist");
        assert_eq!(stale.status, CheckpointStatus::Failed);
        assert_eq!(stale.last_error.as_deref(), Some("stale recovery skipped"));
        assert!(!stale.active_flag, "Stale checkpoint must not stay active");

        // The fresh operation is enqueued.
        let (_active, pending) = coordinator
            .queue_stats()
            .await
            .expect("Failed to read queue stats");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].operation_id, "fresh-op");
    }

    #[tokio::test]
    async fn test_recover_unfinished_operations_replays_all_without_window() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        // No freshness window configured: legacy behavior replays everything.
        let coordinator = OperationCoordinator::new_for_project(1, db.clone())
            .expect("Failed to create coordinator");

        let now = chrono::Utc::now();
        insert_checkpoint(
            &db,
            &make_checkpoint(
                "fresh-op",
                now - chrono::Duration::seconds(60),
                "/project-a",
            ),
        );
        insert_checkpoint(
            &db,
            &make_checkpoint("old-op", now - chrono::Duration::days(30), "/project-b"),
        );

        let recovered = coordinator
            .recover_unfinished_operations()
            .await
            .expect("Failed to recover operations");
        assert_eq!(recovered, 2, "Without a window every operation is replayed");

        let (_active, pending) = coordinator
            .queue_stats()
            .await
            .expect("Failed to read queue stats");
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_recover_unfinished_operations_rejects_unparseable_timestamp() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let coordinator = OperationCoordinator::new_for_project(1, db.clone())
            .expect("Failed to create coordinator")
            .with_recovery_freshness(3600);

        let mut record = make_checkpoint("corrupt-op", chrono::Utc::now(), "/project-a");
        record.updated_at = "not-a-timestamp".to_string();
        insert_checkpoint(&db, &record);

        let recovered = coordinator
            .recover_unfinished_operations()
            .await
            .expect("Failed to recover operations");
        assert_eq!(recovered, 0);

        let checkpoint = coordinator
            .checkpoint_manager()
            .get_checkpoint("corrupt-op")
            .await
            .expect("Failed to load checkpoint")
            .expect("Checkpoint must exist");
        assert_eq!(checkpoint.status, CheckpointStatus::Failed);
    }
}
