//! Unified operation context and result types (OPERATION-LEVEL ABSTRACTIONS)
//!
//! # Module Responsibility
//!
//! This module provides **operation-level** abstractions for **global coordination**:
//! - Defines what types of operations exist (FullIndexing, HotUpdate, IncrementalUpdate)
//! - Manages operation lifecycle phases (Queued → Active → Paused → Completed/Failed)
//! - Controls whether operations can run concurrently (full-index has exclusive access)
//! - Tracks operation-level metrics and progress
//!
//! # Key Difference from index_state
//!
//! **operation module** (this file):
//! - ✓ Global constraints: "Can I run this operation?"
//! - ✓ Lifecycle management: Queued → Active → Completed
//! - ✓ Operation-level progress: X% done processing all files
//! - ✓ Used by: OperationCoordinator, OperationQueue
//! - ✓ Persistence: Checkpoint manager for recovery
//!
//! **index_state module** (see `index_state.rs`):
//! - ✓ File-level tracking: "What's the status of this file?"
//! - ✓ Module-level status: Each file has 5 modules (Relation, Summary, Embedding, Bm25, Export)
//! - ✓ Per-module retries: Independent retry and dead-letter tracking
//! - ✓ Used by: UpdateProcessor, hot update handlers
//! - ✓ Persistence: In-memory with DB sync on failures
//!
//! # When to Use
//!
//! Use **operation** types when:
//! - You need to check if an operation can proceed (scheduling)
//! - You need to persist operation state (recovery from crashes)
//! - You need global constraints (e.g., prevent concurrent full-index)
//!
//! Use **index_state** types when:
//! - You need to track a single file's processing progress
//! - You need module-level status or retries
//! - You need version control (prevent old updates overwriting new)
//!
//! # Collaboration Pattern
//!
//! ```text
//! OperationCoordinator (uses OperationType)
//!   ↓ coordinates
//! UpdateProcessor (tracks FileUpdateState from index_state)
//!   ↓ processes
//! ModuleProcessor (updates file state)
//!   ↓ on failure
//! ```

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

/// Operation lifecycle phase (explicit state machine)
///
/// Defines the complete lifecycle of an operation from creation to completion.
/// This ensures consistent state management across Queue, Coordinator, and CheckpointManager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationPhase {
    /// Operation queued, waiting for execution
    Queued,
    /// Operation started and actively executing
    Active,
    /// Operation paused (can be resumed)
    Paused,
    /// Operation completed successfully
    Completed,
    /// Operation failed (cannot recover)
    Failed,
}

impl OperationPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

/// Operation type classification (GLOBAL OPERATION LEVEL)
///
/// Defines types of operations from a **global coordination perspective**.
/// These determine scheduling constraints and resource allocation:
/// - FullIndexing: Requires exclusive access (blocks other operations)
/// - HotUpdate: Can run while Incremental is blocked
/// - IncrementalUpdate: Can run while HotUpdate is blocked
///
/// **Note**: For file-level context tracking, see `index_state::IndexOperationType`.
/// That enum includes additional metadata (batch info, triggers, versions) relevant to
/// how individual files are processed, not how operations are scheduled.
///
/// # Scheduling Rules
///
/// - FullIndexing: Blocks all other operations
/// - HotUpdate + Incremental: Cannot run concurrently (either one blocks the other)
/// - Only one operation can be Active at a time
///
/// # Example Usage
///
/// ```ignore
/// // Check if we can start a hot update
/// if !coordinator.has_active_full_index().await? {
///     coordinator.request_hot_update(op_id, root_dir).await?;
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Full index rebuild
    FullIndexing,
    /// Hot update (file watch mode)
    HotUpdate,
    /// Incremental update
    IncrementalUpdate,
    /// Configuration change (rebuild affected indexes via the full
    /// prepare/process/commit/abort protocol instead of a direct callback)
    ConfigChange,
}

impl OperationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullIndexing => "FullIndexing",
            Self::HotUpdate => "HotUpdate",
            Self::IncrementalUpdate => "IncrementalUpdate",
            Self::ConfigChange => "ConfigChange",
        }
    }

    /// Convert to index_state::IndexOperationType for file-level tracking
    ///
    /// Maps global operation type to file-level operation context.
    /// This creates a bridge between operation-level coordination and file-level state tracking.
    pub fn to_index_operation_type(
        &self,
        batch_size: usize,
        total_batches: usize,
        root_dir: String,
    ) -> crate::index_state::IndexOperationType {
        match self {
            Self::FullIndexing => crate::index_state::IndexOperationType::Full {
                total_batches,
                batch_size,
                root_dir,
            },
            Self::HotUpdate | Self::ConfigChange => crate::index_state::IndexOperationType::Hot {
                trigger: crate::index_state::ChangeTrigger::Api,
            },
            Self::IncrementalUpdate => {
                crate::index_state::IndexOperationType::Incremental { base_version: 0 }
            }
        }
    }
}

/// Execution context for a single operation
///
/// Every operation has:
/// - Unique operation_id for tracking and recovery
/// - Project ID for multi-project support
/// - Type information for handling differences
/// - Metrics aggregator for cost tracking
#[derive(Clone)]
pub struct OperationContext {
    /// Project ID for multi-project support
    pub project_id: i64,

    /// Unique operation identifier
    pub operation_id: String,

    /// Operation type
    pub operation_type: OperationType,

    /// Start time (for duration tracking)
    pub start_time: DateTime<Utc>,

    /// Estimated total files (for progress percentage)
    pub total_files: usize,

    /// Current file being processed
    pub current_file_index: u32,

    /// True when this operation is resuming an interrupted operation rather
    /// than a fresh run. Storage-backed processors use this to adopt an
    /// existing candidate generation and skip already-completed files.
    pub resume: bool,

    /// Path of the configuration file that triggered a `ConfigChange`
    /// operation. `None` for all other operation types.
    pub config_path: Option<PathBuf>,
}

impl OperationContext {
    /// Create new operation context
    pub fn new(
        project_id: i64,
        operation_id: String,
        operation_type: OperationType,
        total_files: usize,
    ) -> Self {
        Self {
            project_id,
            operation_id,
            operation_type,
            start_time: Utc::now(),
            total_files,
            current_file_index: 0,
            resume: false,
            config_path: None,
        }
    }

    /// Builder: attach the configuration file path that drives this operation.
    pub fn with_config_path(mut self, config_path: PathBuf) -> Self {
        self.config_path = Some(config_path);
        self
    }

    /// Get project ID
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Get progress percentage (0-100)
    pub fn progress_percentage(&self) -> f32 {
        if self.total_files == 0 {
            0.0
        } else {
            (self.current_file_index as f32 / self.total_files as f32) * 100.0
        }
    }

    /// Get elapsed duration
    pub fn elapsed(&self) -> Duration {
        Utc::now() - self.start_time
    }

    /// Get elapsed milliseconds
    pub fn elapsed_ms(&self) -> i64 {
        self.elapsed().num_milliseconds()
    }

    /// Update current file index
    pub fn set_current_file_index(&mut self, index: u32) {
        self.current_file_index = index;
    }
}

/// Operation status classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationStatus {
    /// Operation completed successfully
    Completed,

    /// Operation completed with some module failures (can retry)
    PartiallyCompleted { failed_count: usize },

    /// Operation interrupted (can resume)
    Interrupted { last_checkpoint: String },

    /// Operation failed (cannot recover)
    Failed { reason: String },
}

/// Single module failure record
#[derive(Debug, Clone)]
pub struct ModuleFailure {
    pub file_path: String,
    pub module_name: String,
    pub error: String,
    pub retry_count: u32,
    pub next_retry_time: Option<DateTime<Utc>>,
}

/// Metrics for processor execution
#[derive(Debug, Clone, Default)]
pub struct OperationMetrics {
    pub duration_ms: i64,
    pub llm_tokens_used: Option<i64>,
    pub llm_cost_usd: Option<f64>,
    pub error_count: usize,
}

/// Result of processing a batch
///
/// This result bridges operation-level and file-level tracking:
/// - Reports what was processed at operation level
/// - Provides failed_modules for file-level state synchronization
/// - Enables UpdateStateTracker to be updated with failure details
#[derive(Debug, Clone)]
pub struct OperationProcessResult {
    pub operation_id: String,
    pub processed_files: usize,
    pub success_files: Vec<String>,
    pub failed_modules: Vec<ModuleFailure>,
    pub metrics: OperationMetrics,
}

impl OperationProcessResult {
    /// Get all unique files with failures from this batch
    pub fn files_with_failures(&self) -> Vec<&str> {
        let mut files: Vec<&str> = self
            .failed_modules
            .iter()
            .map(|f| f.file_path.as_str())
            .collect();
        files.sort();
        files.dedup();
        files
    }

    /// Check if a specific file had any failures
    pub fn has_file_failure(&self, file_path: &str) -> bool {
        self.failed_modules.iter().any(|f| f.file_path == file_path)
    }

    /// Get all failures for a specific file
    pub fn get_file_failures(&self, file_path: &str) -> Vec<&ModuleFailure> {
        self.failed_modules
            .iter()
            .filter(|f| f.file_path == file_path)
            .collect()
    }

    /// Get all failures for a specific module type
    pub fn get_module_failures(&self, module_name: &str) -> Vec<&ModuleFailure> {
        self.failed_modules
            .iter()
            .filter(|f| f.module_name == module_name)
            .collect()
    }

    /// Get all files that were processed successfully (without failures)
    pub fn files_without_failures(&self) -> Vec<String> {
        self.success_files.clone()
    }

    /// Check if a specific file/module combination was successful
    ///
    /// Returns true if the file exists but has no failure record for the module,
    /// OR if we explicitly know it was successful.
    pub fn is_module_success(&self, file_path: &str, module_name: &str) -> bool {
        !self
            .failed_modules
            .iter()
            .any(|f| f.file_path == file_path && f.module_name == module_name)
    }

    /// Get distinct list of modules that failed across all files
    pub fn failed_module_types(&self) -> Vec<&str> {
        let mut modules: Vec<&str> = self
            .failed_modules
            .iter()
            .map(|f| f.module_name.as_str())
            .collect();
        modules.sort();
        modules.dedup();
        modules
    }

    /// Check if any module of a given type failed
    pub fn has_module_failure(&self, module_name: &str) -> bool {
        self.failed_modules
            .iter()
            .any(|f| f.module_name == module_name)
    }
}

/// Result of processing a single module
#[derive(Debug, Clone)]
pub struct ModuleProcessResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Operation summary statistics
#[derive(Debug, Clone)]
pub struct OperationSummary {
    pub total_files_processed: usize,
    pub total_files_failed: usize,
    pub total_modules_retried: usize,
    pub total_duration_ms: i64,
    pub can_resume: bool,
}

/// Aggregated metrics across all processors
#[derive(Debug, Clone, Default)]
pub struct AggregatedMetrics {
    pub total_llm_tokens: i64,
    pub total_llm_cost_usd: f64,
    pub avg_file_duration_ms: f64,
    pub estimated_cost_per_file: f64,
}

/// Initialization phase tracking for startup sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitializationPhase {
    /// Preparing for initialization
    Preparing,
    /// Loading persisted active operations from database
    LoadingPersisted,
    /// Cleaning up stale operations
    CleaningStale,
    /// Recovering unfinished operations
    RecoveringUnfinished,
    /// Initialization completed successfully
    Completed,
    /// Initialization failed at a specific phase
    Failed,
}

impl InitializationPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::LoadingPersisted => "loading_persisted",
            Self::CleaningStale => "cleaning_stale",
            Self::RecoveringUnfinished => "recovering_unfinished",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Detailed result of initialization process
#[derive(Debug, Clone)]
pub struct InitializationResult {
    /// Final phase reached during initialization
    pub final_phase: InitializationPhase,
    /// Number of persisted operations loaded
    pub loaded_count: u32,
    /// Number of stale operations cleaned up
    pub cleaned_count: u32,
    /// Number of unfinished operations recovered
    pub recovered_count: u32,
    /// Total initialization duration in milliseconds
    pub duration_ms: u128,
    /// Detailed operation log
    pub details: Vec<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Complete operation result
#[derive(Debug, Clone)]
pub struct OperationResult {
    pub operation_id: String,
    pub status: OperationStatus,
    pub summary: OperationSummary,
    pub failed_modules: Vec<ModuleFailure>,
    pub metrics: AggregatedMetrics,
}

impl OperationResult {
    /// Create a successful empty operation result (no changes)
    pub fn empty_success(operation_id: &str) -> Self {
        Self {
            operation_id: operation_id.to_string(),
            status: OperationStatus::Completed,
            summary: OperationSummary {
                total_files_processed: 0,
                total_files_failed: 0,
                total_modules_retried: 0,
                total_duration_ms: 0,
                can_resume: false,
            },
            failed_modules: Vec::new(),
            metrics: AggregatedMetrics::default(),
        }
    }

    /// Check if operation was successful
    pub fn is_successful(&self) -> bool {
        matches!(self.status, OperationStatus::Completed)
    }

    /// Get number of failed modules
    pub fn failed_module_count(&self) -> usize {
        self.failed_modules.len()
    }

    /// Check if operation can be resumed
    pub fn can_retry(&self) -> bool {
        matches!(
            self.status,
            OperationStatus::PartiallyCompleted { .. } | OperationStatus::Interrupted { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_type_to_index_operation_type_mapping() {
        // Test FullIndexing -> Full
        let op_type = OperationType::FullIndexing;
        let index_op = op_type.to_index_operation_type(32, 10, "/project".into());

        match index_op {
            crate::index_state::IndexOperationType::Full {
                total_batches,
                batch_size,
                root_dir,
            } => {
                assert_eq!(total_batches, 10);
                assert_eq!(batch_size, 32);
                assert_eq!(root_dir, "/project");
            }
            _ => panic!("Expected Full variant"),
        }

        // Test HotUpdate -> Hot
        let op_type = OperationType::HotUpdate;
        let index_op = op_type.to_index_operation_type(32, 10, "/project".into());

        match index_op {
            crate::index_state::IndexOperationType::Hot { trigger } => {
                assert_eq!(trigger, crate::index_state::ChangeTrigger::Api);
            }
            _ => panic!("Expected Hot variant"),
        }

        // Test IncrementalUpdate -> Incremental
        let op_type = OperationType::IncrementalUpdate;
        let index_op = op_type.to_index_operation_type(32, 10, "/project".into());

        match index_op {
            crate::index_state::IndexOperationType::Incremental { base_version } => {
                assert_eq!(base_version, 0);
            }
            _ => panic!("Expected Incremental variant"),
        }

        // Test ConfigChange -> Hot (file-level tracking treats a config
        // rebuild as a hot update trigger)
        let op_type = OperationType::ConfigChange;
        let index_op = op_type.to_index_operation_type(32, 10, "/project".into());

        match index_op {
            crate::index_state::IndexOperationType::Hot { trigger } => {
                assert_eq!(trigger, crate::index_state::ChangeTrigger::Api);
            }
            _ => panic!("Expected Hot variant"),
        }
    }

    #[test]
    fn test_operation_process_result_failure_queries() {
        let result = OperationProcessResult {
            operation_id: "op_123".into(),
            processed_files: 10,
            success_files: vec!["/path/file3.rs".into(), "/path/file4.rs".into()],
            failed_modules: vec![
                ModuleFailure {
                    file_path: "/path/file1.rs".into(),
                    module_name: "relation".into(),
                    error: "Parse error".into(),
                    retry_count: 0,
                    next_retry_time: None,
                },
                ModuleFailure {
                    file_path: "/path/file1.rs".into(),
                    module_name: "embedding".into(),
                    error: "Embedding failed".into(),
                    retry_count: 0,
                    next_retry_time: None,
                },
                ModuleFailure {
                    file_path: "/path/file2.rs".into(),
                    module_name: "relation".into(),
                    error: "Parse error in file2".into(),
                    retry_count: 0,
                    next_retry_time: None,
                },
            ],
            metrics: OperationMetrics::default(),
        };

        // Test files_with_failures
        let files = result.files_with_failures();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"/path/file1.rs"));
        assert!(files.contains(&"/path/file2.rs"));

        // Test has_file_failure
        assert!(result.has_file_failure("/path/file1.rs"));
        assert!(result.has_file_failure("/path/file2.rs"));
        assert!(!result.has_file_failure("/path/file3.rs"));

        // Test get_file_failures
        let file1_failures = result.get_file_failures("/path/file1.rs");
        assert_eq!(file1_failures.len(), 2);

        let file2_failures = result.get_file_failures("/path/file2.rs");
        assert_eq!(file2_failures.len(), 1);

        // Test get_module_failures
        let relation_failures = result.get_module_failures("relation");
        assert_eq!(relation_failures.len(), 2);

        let embedding_failures = result.get_module_failures("embedding");
        assert_eq!(embedding_failures.len(), 1);

        // Test files_without_failures
        let success = result.files_without_failures();
        assert_eq!(success.len(), 2);
        assert!(success.contains(&"/path/file3.rs".to_string()));
        assert!(success.contains(&"/path/file4.rs".to_string()));
    }

    #[test]
    fn test_operation_type_as_str() {
        assert_eq!(OperationType::FullIndexing.as_str(), "FullIndexing");
        assert_eq!(OperationType::HotUpdate.as_str(), "HotUpdate");
        assert_eq!(
            OperationType::IncrementalUpdate.as_str(),
            "IncrementalUpdate"
        );
        assert_eq!(OperationType::ConfigChange.as_str(), "ConfigChange");
    }

    #[test]
    fn test_operation_context_config_path_builder() {
        let ctx = OperationContext::new(1, "op".into(), OperationType::ConfigChange, 0);
        assert!(ctx.config_path.is_none());

        let ctx = ctx.with_config_path("/root/Cargo.toml".into());
        assert_eq!(
            ctx.config_path.as_deref(),
            Some(std::path::Path::new("/root/Cargo.toml"))
        );
        assert_eq!(ctx.operation_type, OperationType::ConfigChange);
    }

    #[test]
    fn test_operation_phase_is_terminal() {
        assert!(!OperationPhase::Queued.is_terminal());
        assert!(!OperationPhase::Active.is_terminal());
        assert!(!OperationPhase::Paused.is_terminal());
        assert!(OperationPhase::Completed.is_terminal());
        assert!(OperationPhase::Failed.is_terminal());
    }
}
