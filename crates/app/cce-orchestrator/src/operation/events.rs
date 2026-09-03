//! Operation event system for real-time monitoring
//!
//! Provides event-driven notifications for operation lifecycle events:
//! - Operation start/progress/completion/failure
//! - File-level processing events
//! - Error tracking with recovery suggestions

use std::sync::Arc;

/// Operation-level lifecycle events
#[derive(Debug, Clone)]
pub enum OperationEvent {
    /// Operation started execution
    Started {
        project_id: i64,
        operation_id: String,
        operation_type: String,
        total_files: usize,
        timestamp_ms: u64,
    },
    /// Operation progress updated
    ProgressUpdated {
        project_id: i64,
        operation_id: String,
        processed: usize,
        total: usize,
        current_file: String,
        progress_percent: f32,
        timestamp_ms: u64,
    },
    /// Single file processing completed
    ///
    /// This event includes detailed module-level information to support
    /// proper state tracking at the file level.
    FileCompleted {
        project_id: i64,
        operation_id: String,
        file_path: String,
        duration_ms: u128,
        /// Modules that completed successfully (empty = no info available, treat as all)
        successfully_processed_modules: Vec<String>,
        /// Modules that failed during processing
        failed_modules: Vec<(String, String)>, // (module_name, error)
        /// File version for version control
        file_version: u64,
        timestamp_ms: u64,
    },
    /// Single file processing failed
    FileFailed {
        project_id: i64,
        operation_id: String,
        file_path: String,
        module: String,
        error: String,
        retry_count: u32,
        timestamp_ms: u64,
    },
    /// Batch of files completed
    BatchCompleted {
        project_id: i64,
        operation_id: String,
        batch_number: u32,
        files_processed: usize,
        files_failed: usize,
        batch_duration_ms: u128,
        timestamp_ms: u64,
    },
    /// Operation completed successfully
    Completed {
        project_id: i64,
        operation_id: String,
        total_files_processed: usize,
        total_files_failed: usize,
        duration_ms: u128,
        throughput_files_per_sec: Option<f64>,
        timestamp_ms: u64,
    },
    /// Operation encountered failure
    Failed {
        project_id: i64,
        operation_id: String,
        error: String,
        phase: String,
        partial_count: usize,
        timestamp_ms: u64,
    },
    /// Operation paused (can resume)
    Paused {
        project_id: i64,
        operation_id: String,
        processed: usize,
        total: usize,
        timestamp_ms: u64,
    },
    /// Operation resumed from checkpoint
    Resumed {
        project_id: i64,
        operation_id: String,
        last_checkpoint: String,
        timestamp_ms: u64,
    },
}

impl OperationEvent {
    /// Get project ID from event
    pub fn project_id(&self) -> i64 {
        match self {
            Self::Started { project_id, .. } => *project_id,
            Self::ProgressUpdated { project_id, .. } => *project_id,
            Self::FileCompleted { project_id, .. } => *project_id,
            Self::FileFailed { project_id, .. } => *project_id,
            Self::BatchCompleted { project_id, .. } => *project_id,
            Self::Completed { project_id, .. } => *project_id,
            Self::Failed { project_id, .. } => *project_id,
            Self::Paused { project_id, .. } => *project_id,
            Self::Resumed { project_id, .. } => *project_id,
        }
    }

    /// Get operation ID from event
    pub fn operation_id(&self) -> &str {
        match self {
            Self::Started { operation_id, .. } => operation_id,
            Self::ProgressUpdated { operation_id, .. } => operation_id,
            Self::FileCompleted { operation_id, .. } => operation_id,
            Self::FileFailed { operation_id, .. } => operation_id,
            Self::BatchCompleted { operation_id, .. } => operation_id,
            Self::Completed { operation_id, .. } => operation_id,
            Self::Failed { operation_id, .. } => operation_id,
            Self::Paused { operation_id, .. } => operation_id,
            Self::Resumed { operation_id, .. } => operation_id,
        }
    }

    /// Get event timestamp in milliseconds
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::Started { timestamp_ms, .. } => *timestamp_ms,
            Self::ProgressUpdated { timestamp_ms, .. } => *timestamp_ms,
            Self::FileCompleted { timestamp_ms, .. } => *timestamp_ms,
            Self::FileFailed { timestamp_ms, .. } => *timestamp_ms,
            Self::BatchCompleted { timestamp_ms, .. } => *timestamp_ms,
            Self::Completed { timestamp_ms, .. } => *timestamp_ms,
            Self::Failed { timestamp_ms, .. } => *timestamp_ms,
            Self::Paused { timestamp_ms, .. } => *timestamp_ms,
            Self::Resumed { timestamp_ms, .. } => *timestamp_ms,
        }
    }

    /// Format event as human-readable string
    pub fn format(&self) -> String {
        match self {
            Self::Started {
                operation_id,
                operation_type,
                total_files,
                ..
            } => {
                format!(
                    "📊 Operation {} ({}) started with {} files",
                    operation_id, operation_type, total_files
                )
            }
            Self::ProgressUpdated {
                operation_id,
                processed,
                total,
                progress_percent,
                ..
            } => {
                format!(
                    "⏳ {} Progress: {:.1}% ({}/{})",
                    operation_id, progress_percent, processed, total
                )
            }
            Self::FileCompleted {
                file_path,
                duration_ms,
                successfully_processed_modules,
                failed_modules,
                ..
            } => {
                let success_count = successfully_processed_modules.len();
                let failure_count = failed_modules.len();
                format!(
                    "✅ File completed: {} ({:.0}ms, {} success, {} failed)",
                    file_path, duration_ms, success_count, failure_count
                )
            }
            Self::FileFailed {
                file_path,
                module,
                error,
                retry_count,
                ..
            } => {
                format!(
                    "⚠️  File failed: {} [{}] {} (retry: {})",
                    file_path, module, error, retry_count
                )
            }
            Self::BatchCompleted {
                batch_number,
                files_processed,
                files_failed,
                batch_duration_ms,
                ..
            } => {
                format!(
                    "📦 Batch {} completed: {} processed, {} failed ({:.0}ms)",
                    batch_number, files_processed, files_failed, batch_duration_ms
                )
            }
            Self::Completed {
                total_files_processed,
                duration_ms,
                throughput_files_per_sec,
                ..
            } => {
                let throughput = throughput_files_per_sec
                    .map(|t| format!(" ({:.2} files/sec)", t))
                    .unwrap_or_default();
                format!(
                    "✨ Operation completed: {} files in {:.1}s{}",
                    total_files_processed,
                    *duration_ms as f64 / 1000.0,
                    throughput
                )
            }
            Self::Failed {
                error,
                phase,
                partial_count,
                ..
            } => {
                format!(
                    "❌ Operation failed at {}: {} ({} partial)",
                    phase, error, partial_count
                )
            }
            Self::Paused {
                processed, total, ..
            } => {
                format!("⏸️  Operation paused: {}/{}", processed, total)
            }
            Self::Resumed {
                last_checkpoint, ..
            } => {
                format!("▶️  Operation resumed from checkpoint: {}", last_checkpoint)
            }
        }
    }
}

/// Event listener callback type
pub type EventListener = Arc<dyn Fn(&OperationEvent) + Send + Sync>;

/// Event bus for operation event distribution
#[derive(Clone)]
pub struct OperationEventBus {
    listeners: Arc<parking_lot::RwLock<Vec<EventListener>>>,
}

impl OperationEventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }

    /// Subscribe a listener to events
    pub fn subscribe(&self, listener: EventListener) {
        let mut listeners = self.listeners.write();
        listeners.push(listener);
    }

    /// Emit an event to all subscribers
    pub fn emit(&self, event: OperationEvent) {
        let listeners = self.listeners.read();
        for listener in listeners.iter() {
            listener(&event);
        }
    }

    /// Get number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.listeners.read().len()
    }

    /// Clear all subscribers
    pub fn clear(&self) {
        self.listeners.write().clear();
    }
}

impl Default for OperationEventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter for selective event handling
pub trait EventFilter: Send + Sync {
    /// Check if event should be processed
    fn should_process(&self, event: &OperationEvent) -> bool;
}

/// Filter events by operation ID
#[derive(Clone)]
pub struct OperationIdFilter {
    operation_id: String,
}

impl OperationIdFilter {
    pub fn new(operation_id: String) -> Self {
        Self { operation_id }
    }
}

impl EventFilter for OperationIdFilter {
    fn should_process(&self, event: &OperationEvent) -> bool {
        event.operation_id() == self.operation_id
    }
}

/// Filter events by event type
#[derive(Clone)]
pub struct EventTypeFilter {
    include_started: bool,
    include_progress: bool,
    include_completed: bool,
    include_failed: bool,
}

impl EventTypeFilter {
    pub fn new() -> Self {
        Self {
            include_started: true,
            include_progress: true,
            include_completed: true,
            include_failed: true,
        }
    }

    pub fn only_completion(mut self) -> Self {
        self.include_progress = false;
        self
    }

    pub fn without_progress(mut self) -> Self {
        self.include_progress = false;
        self
    }
}

impl Default for EventTypeFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventFilter for EventTypeFilter {
    fn should_process(&self, event: &OperationEvent) -> bool {
        match event {
            OperationEvent::Started { .. } => self.include_started,
            OperationEvent::ProgressUpdated { .. } => self.include_progress,
            OperationEvent::Completed { .. } => self.include_completed,
            OperationEvent::Failed { .. } => self.include_failed,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_creation() {
        let bus = OperationEventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn test_event_format() {
        let event = OperationEvent::Started {
            project_id: 1,
            operation_id: "op1".to_string(),
            operation_type: "full_index".to_string(),
            total_files: 100,
            timestamp_ms: 1000,
        };

        let formatted = event.format();
        assert!(formatted.contains("op1"));
        assert!(formatted.contains("full_index"));
        assert!(formatted.contains("100"));
    }

    #[test]
    fn test_event_operation_id() {
        let event = OperationEvent::Completed {
            project_id: 1,
            operation_id: "test_op".to_string(),
            total_files_processed: 50,
            total_files_failed: 0,
            duration_ms: 5000,
            throughput_files_per_sec: Some(10.0),
            timestamp_ms: 1000,
        };

        assert_eq!(event.operation_id(), "test_op");
    }
}
