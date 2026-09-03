//! Operation queue for managing and scheduling indexing operations
//!
//! Coordinates full-index, hot-update, and incremental operations with:
//! - Operation queuing and priority-based dispatch
//! - Active operation tracking with database persistence
//! - Concurrent operation prevention (single active at a time)
//! - SQLite persistence for recovery after process crashes

use chrono::Utc;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, trace, warn};

use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::CheckpointRepository;
use cce_types::{OperationKind, StorageError};

/// Maximum number of pending (queued, not active) operations.
///
/// The pending queue is bounded so a flood of update requests cannot grow
/// memory without limit. When full, the lowest-priority pending operation is
/// dropped (it will simply be superseded by the next change event).
const MAX_PENDING_OPERATIONS: usize = 64;

/// Operation priority levels (higher = earlier execution)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationPriority {
    /// Full index operations (highest priority)
    FullIndex = 3,
    /// Incremental update operations
    Incremental = 2,
    /// Hot update operations (lowest priority)
    HotUpdate = 1,
}

impl OperationPriority {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }

    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::HotUpdate),
            2 => Some(Self::Incremental),
            3 => Some(Self::FullIndex),
            _ => None,
        }
    }
}

/// Represents an operation awaiting execution
#[derive(Debug, Clone)]
pub struct PendingOperation {
    pub project_id: i64,
    pub operation_id: String,
    pub operation_type: OperationKind,
    pub priority: OperationPriority,
    pub root_dir: String,
    pub created_at: String,
    pub status: String,
}

/// Represents an active operation
#[derive(Debug, Clone)]
pub struct ActiveOperation {
    pub project_id: i64,
    pub operation_id: String,
    pub operation_type: OperationKind,
    pub priority: OperationPriority,
    pub started_at: String,
    pub root_dir: String,
}

impl ActiveOperation {
    pub fn to_pending(&self) -> PendingOperation {
        PendingOperation {
            project_id: self.project_id,
            operation_id: self.operation_id.clone(),
            operation_type: self.operation_type,
            priority: self.priority,
            root_dir: self.root_dir.clone(),
            created_at: self.started_at.clone(),
            status: "active".to_string(),
        }
    }
}

/// Operation queue managing task dispatch and active operation tracking
///
/// This queue now persists the active operation to the database to survive
/// process crashes. The pending operations are kept in memory for performance.
pub struct OperationQueue {
    /// Project ID for multi-project support
    project_id: i64,
    /// Currently active operation (only one at a time), persisted in DB
    active: Arc<Mutex<Option<ActiveOperation>>>,
    /// Pending operations waiting for execution, ordered by priority
    pending: Arc<Mutex<VecDeque<PendingOperation>>>,
    /// SQLite database for persistence
    db: Arc<SqliteClient>,
}

impl OperationQueue {
    /// Create a new operation queue for a specific project
    ///
    /// Loads any persisted active operation from the database.
    /// project_id must be > 0 (no global/default projects allowed)
    pub fn new_for_project(project_id: i64, db: Arc<SqliteClient>) -> Self {
        assert!(
            project_id > 0,
            "project_id must be > 0 for explicit project"
        );
        Self {
            project_id,
            active: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            db,
        }
    }

    /// Load active operation from database (call on startup)
    pub async fn load_persisted_active(&self) -> Result<Option<ActiveOperation>, StorageError> {
        let active_op = {
            let conn = self.db.read_connection()?;
            if let Some(record) =
                CheckpointRepository::get_active_checkpoint(&conn, self.project_id)?
            {
                Some(ActiveOperation {
                    project_id: record.project_id,
                    operation_id: record.operation_id,
                    operation_type: record.operation_type.parse().map_err(|e: String| {
                        StorageError::validation(format!("Invalid operation_type: {e}"))
                    })?,
                    priority: OperationPriority::from_i32(record.priority)
                        .ok_or_else(|| StorageError::validation("Invalid priority value"))?,
                    started_at: record.created_at,
                    root_dir: record.root_dir,
                })
            } else {
                None
            }
        };

        if let Some(active_op) = active_op {
            let mut active = self.active.lock().await;
            *active = Some(active_op.clone());

            info!(
                operation_id = %active_op.operation_id,
                "Loaded persisted active operation from database"
            );

            Ok(Some(active_op))
        } else {
            Ok(None)
        }
    }

    /// Enqueue an operation (full-index, hot-update, or incremental)
    ///
    /// If an operation with the same type and root_dir already exists in the queue,
    /// it will be replaced (deduplication).
    pub async fn enqueue(
        &self,
        operation_id: String,
        operation_type: OperationKind,
        root_dir: String,
        priority: OperationPriority,
    ) -> Result<(), StorageError> {
        trace!(
            operation_id = %operation_id,
            operation_type = %operation_type,
            priority = ?priority,
            "Enqueueing operation"
        );

        let new_op = PendingOperation {
            project_id: self.project_id,
            operation_id: operation_id.clone(),
            operation_type,
            priority,
            root_dir: root_dir.clone(),
            created_at: Utc::now().to_rfc3339(),
            status: "pending".to_string(),
        };

        let mut pending = self.pending.lock().await;

        // Deduplication: remove any existing operation with same type and root_dir
        pending
            .retain(|op| !(op.operation_type == new_op.operation_type && op.root_dir == root_dir));

        // Insert in priority order (highest priority first)
        let mut inserted = false;
        for (idx, existing_op) in pending.iter().enumerate() {
            if new_op.priority > existing_op.priority {
                pending.insert(idx, new_op.clone());
                inserted = true;
                break;
            }
        }
        if !inserted {
            pending.push_back(new_op);
        }

        // Bounded queue: when at capacity, drop the *oldest* operation
        // of the lowest-priority group (the queue is sorted with highest
        // priority first, so the lowest-priority entries form the suffix; the
        // first member of that suffix is the oldest). Newer operations carry
        // fresher state and supersede older ones, so they are preferred.
        if pending.len() > MAX_PENDING_OPERATIONS {
            if let Some(lowest_priority) = pending.back().map(|op| op.priority) {
                if let Some(dropped_pos) =
                    pending.iter().position(|op| op.priority == lowest_priority)
                {
                    if let Some(dropped) = pending.remove(dropped_pos) {
                        warn!(
                            dropped_operation = %dropped.operation_id,
                            dropped_type = ?dropped.operation_type,
                            queue_len = pending.len(),
                            "Pending operation queue at capacity, dropping oldest lowest-priority entry"
                        );
                    }
                }
            }
        }

        trace!(
            operation_id = %operation_id,
            queue_size = pending.len(),
            "Operation enqueued"
        );

        Ok(())
    }

    /// Dequeue and start the next pending operation
    ///
    /// Returns None if queue is empty or an operation is already active.
    /// **IMPORTANT**: This also persists the operation to the database.
    ///
    /// The two queue mutexes are only held to check/reserve/pop (a short
    /// critical section with no I/O). The SQLite transaction runs after both
    /// locks are released; if it fails, the in-memory reservation is rolled
    /// back so the operation is neither lost nor left permanently active.
    pub async fn dequeue(&self) -> Result<Option<PendingOperation>, StorageError> {
        // Phase 1: check the active slot and pop the next pending operation.
        // The active slot is reserved in the same critical section so two
        // concurrent dequeues cannot both proceed.
        let next_op = {
            let mut active = self.active.lock().await;

            if active.is_some() {
                trace!("Cannot dequeue: operation already active");
                return Ok(None);
            }

            let mut pending = self.pending.lock().await;

            let Some(next_op) = pending.pop_front() else {
                trace!("Queue is empty");
                return Ok(None);
            };

            let active_op = ActiveOperation {
                project_id: self.project_id,
                operation_id: next_op.operation_id.clone(),
                operation_type: next_op.operation_type,
                priority: next_op.priority,
                started_at: Utc::now().to_rfc3339(),
                root_dir: next_op.root_dir.clone(),
            };

            *active = Some(active_op);
            next_op
        };

        trace!(
            operation_id = %next_op.operation_id,
            "Dequeueing operation from queue"
        );

        // Phase 2: persist the active flag without holding either queue lock.
        // A failure rolls back the in-memory reservation made above.
        let persist_result: Result<(), StorageError> = (|| {
            let conn = self.db.write_connection()?;
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;
            CheckpointRepository::set_active_flag(
                &tx,
                self.project_id,
                &next_op.operation_id,
                next_op.priority.as_i32(),
            )?;
            tx.commit().map_err(|e| StorageError::Sqlite(e.to_string()))
        })();

        if let Err(error) = persist_result {
            // Roll back the reservation: restore the popped operation and free
            // the active slot so a later dequeue can pick it up again.
            let mut active = self.active.lock().await;
            *active = None;
            let mut pending = self.pending.lock().await;
            pending.push_front(next_op.clone());
            trace!(
                operation_id = %next_op.operation_id,
                error = %error,
                "Dequeue persisted active flag failed, restoring pending operation"
            );
            return Err(error);
        }

        let queue_size = self.pending.lock().await.len();
        trace!(
            operation_id = %next_op.operation_id,
            queue_size,
            "Operation dequeued and persisted as active"
        );

        Ok(Some(next_op))
    }

    /// Update heartbeat of active operation (for stale detection)
    ///
    /// Call this periodically to indicate the operation is still running.
    pub async fn heartbeat(&self, operation_id: &str) -> Result<(), StorageError> {
        let conn = self.db.write_connection()?;
        CheckpointRepository::update_heartbeat(&conn, self.project_id, operation_id)
    }

    /// Mark the active operation as completed and remove it
    pub async fn complete_active(&self) -> Result<Option<ActiveOperation>, StorageError> {
        let mut active = self.active.lock().await;

        if let Some(completed) = active.take() {
            // Clear active flag in database
            let conn = self.db.write_connection()?;
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;
            CheckpointRepository::clear_active_flag(&tx, self.project_id, &completed.operation_id)?;
            tx.commit()
                .map_err(|e| StorageError::Sqlite(e.to_string()))?;

            trace!(
                operation_id = %completed.operation_id,
                "Operation completed and removed from active and database"
            );
            Ok(Some(completed))
        } else {
            warn!("No active operation to complete");
            Ok(None)
        }
    }

    /// Clear active flag for a specific operation id regardless of in-memory state.
    ///
    /// Failure paths cannot rely on `complete_active` which takes the current
    /// in-memory `active` slot: a `dequeue` transaction may have rolled back
    /// the DB `active_flag` while leaving the in-memory slot set, or a crash
    /// may have left DB `active_flag=1` with `last_heartbeat IS NULL`. Clearing
    /// by `operation_id` is idempotent and always reconciles both sides so the
    /// queue never deadlocks on a failed operation.
    pub async fn clear_active_by_operation(&self, operation_id: &str) -> Result<(), StorageError> {
        {
            let mut active = self.active.lock().await;
            if let Some(op) = active.as_ref() {
                if op.operation_id == operation_id {
                    *active = None;
                    trace!(
                        operation_id = %operation_id,
                        "Cleared in-memory active flag by operation id"
                    );
                }
            }
        }
        let conn = self.db.write_connection()?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        CheckpointRepository::clear_active_flag(&tx, self.project_id, operation_id)?;
        tx.commit()
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        trace!(
            operation_id = %operation_id,
            "Cleared DB active flag by operation id"
        );
        Ok(())
    }

    /// Get the currently active operation (if any)
    pub async fn get_active(&self) -> Result<Option<ActiveOperation>, StorageError> {
        let active = self.active.lock().await;
        Ok(active.clone())
    }

    /// Get pending operations without removing them
    pub async fn peek_pending(&self) -> Result<Vec<PendingOperation>, StorageError> {
        let pending = self.pending.lock().await;
        Ok(pending.iter().cloned().collect())
    }

    /// Get queue size (active + pending)
    pub async fn queue_size(&self) -> Result<usize, StorageError> {
        let active = self.active.lock().await;
        let pending = self.pending.lock().await;
        let active_count = if active.is_some() { 1 } else { 0 };
        Ok(active_count + pending.len())
    }

    /// Cancel a pending operation by operation_id
    pub async fn cancel_pending(
        &self,
        operation_id: &str,
    ) -> Result<Option<PendingOperation>, StorageError> {
        let mut pending = self.pending.lock().await;

        if let Some(pos) = pending
            .iter()
            .position(|op| op.operation_id == operation_id)
        {
            let cancelled = pending.remove(pos);
            trace!(
                operation_id = %operation_id,
                "Pending operation cancelled"
            );
            Ok(cancelled)
        } else {
            trace!(
                operation_id = %operation_id,
                "Operation not found in queue"
            );
            Ok(None)
        }
    }

    /// Clear all pending operations (but not active)
    pub async fn clear_pending(&self) -> Result<(), StorageError> {
        let mut pending = self.pending.lock().await;
        let count = pending.len();
        pending.clear();
        info!(cleared_count = count, "All pending operations cleared");
        Ok(())
    }

    /// Check if a full-index operation is active
    pub async fn has_active_full_index(&self) -> Result<bool, StorageError> {
        let active = self.active.lock().await;
        Ok(active
            .as_ref()
            .map(|op| op.operation_type == OperationKind::FullIndex)
            .unwrap_or(false))
    }

    /// Check if any hot-update operation is pending or active
    pub async fn has_pending_hot_update(&self) -> Result<bool, StorageError> {
        let active = self.active.lock().await;
        let has_active = active
            .as_ref()
            .map(|op| op.operation_type == OperationKind::HotUpdate)
            .unwrap_or(false);

        if has_active {
            return Ok(true);
        }

        let pending = self.pending.lock().await;
        Ok(pending
            .iter()
            .any(|op| op.operation_type == OperationKind::HotUpdate))
    }

    /// Cleanup stale active operations from database (call on startup)
    ///
    /// Detects operations that crashed without proper cleanup.
    /// These are operations that haven't sent a heartbeat for specified seconds.
    pub async fn cleanup_stale_operations(
        &self,
        stale_threshold_secs: i64,
    ) -> Result<usize, StorageError> {
        let (cleared, db_active) = {
            let conn = self.db.write_connection()?;
            let cleared = CheckpointRepository::cleanup_stale_active(
                &conn,
                self.project_id,
                stale_threshold_secs,
            )?;
            let db_active = if cleared > 0 {
                CheckpointRepository::get_active_checkpoint(&conn, self.project_id)?
            } else {
                None
            };
            (cleared, db_active)
        };
        if cleared > 0 {
            let mut active = self.active.lock().await;
            if let Some(mem) = active.as_ref() {
                let still_active = db_active
                    .as_ref()
                    .is_some_and(|record| record.operation_id == mem.operation_id);
                if !still_active {
                    warn!(
                        operation_id = %mem.operation_id,
                        "Clearing stale in-memory active operation after DB cleanup"
                    );
                    *active = None;
                }
            }
        }
        Ok(cleared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_ordering() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let queue = OperationQueue::new_for_project(1, db);

        // Enqueue in reverse priority order
        queue
            .enqueue(
                "hot1".to_string(),
                OperationKind::HotUpdate,
                "/path1".to_string(),
                OperationPriority::HotUpdate,
            )
            .await
            .expect("Failed to enqueue hot update");

        queue
            .enqueue(
                "full1".to_string(),
                OperationKind::FullIndex,
                "/path2".to_string(),
                OperationPriority::FullIndex,
            )
            .await
            .expect("Failed to enqueue full index");

        queue
            .enqueue(
                "inc1".to_string(),
                OperationKind::Incremental,
                "/path3".to_string(),
                OperationPriority::Incremental,
            )
            .await
            .expect("Failed to enqueue incremental");

        // Verify order when dequeuing
        let first = queue
            .dequeue()
            .await
            .expect("Failed to dequeue")
            .expect("Queue is empty");
        assert_eq!(first.operation_id, "full1");

        queue.complete_active().await.expect("Failed to complete");

        let second = queue
            .dequeue()
            .await
            .expect("Failed to dequeue")
            .expect("Queue is empty");
        assert_eq!(second.operation_id, "inc1");

        queue.complete_active().await.expect("Failed to complete");

        let third = queue
            .dequeue()
            .await
            .expect("Failed to dequeue")
            .expect("Queue is empty");
        assert_eq!(third.operation_id, "hot1");
    }

    #[tokio::test]
    async fn test_deduplication() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let queue = OperationQueue::new_for_project(1, db);

        // Enqueue same operation twice
        queue
            .enqueue(
                "op1".to_string(),
                OperationKind::HotUpdate,
                "/path1".to_string(),
                OperationPriority::HotUpdate,
            )
            .await
            .expect("Failed to enqueue");

        queue
            .enqueue(
                "op2".to_string(),
                OperationKind::HotUpdate,
                "/path1".to_string(),
                OperationPriority::HotUpdate,
            )
            .await
            .expect("Failed to enqueue");

        let pending = queue.peek_pending().await.expect("Failed to peek");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].operation_id, "op2");
    }

    #[tokio::test]
    async fn test_active_prevents_dequeue() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let queue = OperationQueue::new_for_project(1, db);

        queue
            .enqueue(
                "op1".to_string(),
                OperationKind::FullIndex,
                "/path1".to_string(),
                OperationPriority::FullIndex,
            )
            .await
            .expect("Failed to enqueue");

        queue
            .enqueue(
                "op2".to_string(),
                OperationKind::HotUpdate,
                "/path2".to_string(),
                OperationPriority::HotUpdate,
            )
            .await
            .expect("Failed to enqueue");

        // Dequeue first operation
        let first = queue
            .dequeue()
            .await
            .expect("Failed to dequeue")
            .expect("Queue is empty");
        assert_eq!(first.operation_id, "op1");

        // Try to dequeue while active - should return None
        let should_be_none = queue.dequeue().await.expect("Failed to dequeue");
        assert!(should_be_none.is_none());

        // Complete first and dequeue second
        queue.complete_active().await.expect("Failed to complete");
        let second = queue
            .dequeue()
            .await
            .expect("Failed to dequeue")
            .expect("Queue is empty");
        assert_eq!(second.operation_id, "op2");
    }

    /// The pending queue stays bounded; lowest-priority entries are
    /// dropped when the queue exceeds its capacity.
    #[tokio::test]
    async fn test_pending_queue_bounded_drops_lowest_priority() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let queue = OperationQueue::new_for_project(1, db);

        // Distinct root dirs avoid deduplication across enqueues.
        for i in 0..(MAX_PENDING_OPERATIONS + 20) {
            queue
                .enqueue(
                    format!("op-{i}"),
                    OperationKind::HotUpdate,
                    format!("/path-{i}"),
                    OperationPriority::HotUpdate,
                )
                .await
                .expect("Failed to enqueue");
        }

        let pending = queue.peek_pending().await.expect("Failed to peek");
        assert_eq!(
            pending.len(),
            MAX_PENDING_OPERATIONS,
            "pending queue must stay bounded"
        );

        // The oldest (lowest-priority, pushed first) entries are the dropped
        // ones; the newest operations survive.
        assert!(
            pending
                .iter()
                .any(|op| op.operation_id == format!("op-{}", MAX_PENDING_OPERATIONS + 19)),
            "newest operations must survive the bound"
        );
        assert!(
            pending.iter().all(|op| op.operation_id != "op-0"),
            "oldest operations must be dropped"
        );
    }

    /// The DB persistence runs outside the queue locks, but the active
    /// reservation is made atomically with the pop, so a second concurrent
    /// dequeue observes the reserved slot and returns None instead of running
    /// two operations at once.
    #[tokio::test]
    async fn test_dequeue_reservation_blocks_concurrent_dequeue() {
        let db = Arc::new(SqliteClient::in_memory().expect("Failed to create memory database"));
        let queue = Arc::new(OperationQueue::new_for_project(1, db));

        queue
            .enqueue(
                "op1".to_string(),
                OperationKind::FullIndex,
                "/path1".to_string(),
                OperationPriority::FullIndex,
            )
            .await
            .expect("Failed to enqueue");

        // Race two concurrent dequeues; exactly one must win.
        let q1 = queue.clone();
        let q2 = queue.clone();
        let (r1, r2) = tokio::join!(q1.dequeue(), q2.dequeue());
        let winners = r1.expect("first dequeue").map(|_| 1).unwrap_or(0)
            + r2.expect("second dequeue").map(|_| 1).unwrap_or(0);
        assert_eq!(
            winners, 1,
            "only one of two concurrent dequeues may claim the active slot"
        );

        // Complete the winner and dequeue the restored/lost operation only if
        // the active slot is free; the queue must not be stuck.
        queue
            .complete_active()
            .await
            .expect("Failed to complete active");
    }
}
