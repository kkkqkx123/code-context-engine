//! Update state tracking for lightweight eventual consistency (FILE-LEVEL ABSTRACTIONS)
//!
//! # Module Responsibility
//!
//! This module provides **file-level** abstractions for **detailed progress tracking**:
//! - Defines file-level states for all operations (Full, Hot, Incremental)
//! - Tracks per-module status (Relation, Summary, Embedding, Bm25, Export)
//! - Implements independent module retries with exponential backoff
//! - Supports dead-letter queue for permanently failed modules
//! - Enforces version control (prevents old updates from overwriting new)
//!
//! # Key Difference from operation
//!
//! **index_state module** (this file):
//! - ✓ File-level tracking: "What's the status of this file?"
//! - ✓ Module independence: Each module has separate state and retries
//! - ✓ Version control: Prevents concurrent update conflicts
//! - ✓ Eventual consistency: Partial failures still allow queries
//! - ✓ Used by: UpdateProcessor, module processors
//! - ✓ Persistence: In-memory with DB sync for failures
//!
//! **operation module** (see `operation/context.rs`):
//! - ✓ Operation-level management: "Can I run this operation?"
//! - ✓ Global constraints: Only one active operation
//! - ✓ Lifecycle management: Queued → Active → Completed
//! - ✓ Coarse-grained: Progress is "X% of all files done"
//! - ✓ Used by: OperationCoordinator, OperationQueue
//! - ✓ Persistence: Checkpoint manager for recovery
//!
//! # When to Use
//!
//! Use **index_state** types when:
//! - You need to check if a specific file/module is ready
//! - You need to update module status individually
//! - You need version control for the file
//! - You need to determine if a file is queryable
//!
//! Use **operation** types when:
//! - You need to check if an operation can be scheduled
//! - You need to persist operation state across crashes
//! - You need global constraints (prevent concurrent full-index)
//!
//! # Version Control Example
//!
//! ```text
//! File version = 1: Relation ✓, Summary ✓, Embedding ✗, Bm25 ✓, Export ✓
//! File version = 2: Relation ✗ (old update, ignored), Summary ✓, ...
//!
//! → Version 2's failure is recorded but doesn't affect version 1's state
//! → Query can use version 1 (partially complete but consistent)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::hot_update::FileChangeType;

/// Module types that participate in hot updates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleType {
    /// Relation index module
    Relation,
    /// Summary generation module
    Summary,
    /// Embedding vector module
    Embedding,
    /// BM25 full-text search module
    Bm25,
    /// Export module (natural language document export)
    Export,
}

impl ModuleType {
    /// Get string representation of the module type
    pub const fn as_str(&self) -> &'static str {
        match self {
            ModuleType::Relation => "relation",
            ModuleType::Summary => "summary",
            ModuleType::Embedding => "embedding",
            ModuleType::Bm25 => "bm25",
            ModuleType::Export => "export",
        }
    }

    /// Get all module types
    pub const fn all() -> [ModuleType; 5] {
        [
            ModuleType::Relation,
            ModuleType::Summary,
            ModuleType::Embedding,
            ModuleType::Bm25,
            ModuleType::Export,
        ]
    }
}

impl std::fmt::Display for ModuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Index operation type (FILE-LEVEL CONTEXT)
///
/// Defines the **context** in which a file is being processed from a file-level perspective.
/// This differs from `operation::OperationType` which is about **global scheduling**.
///
/// This enum includes context-specific metadata needed for file processing:
/// - Batch information (for full index resumption)
/// - Trigger information (why was this file updated?)
/// - Version information (for incremental updates)
///
/// # Relationship to operation::OperationType
///
/// | Aspect | operation::OperationType | index_state::IndexOperationType |
/// |--------|-------------------------|--------------------------------|
/// | **Focus** | Scheduling constraint | Processing context |
/// | **Scope** | Global (all files) | Local (single file) |
/// | **Metadata** | None (just the type) | Batch/trigger/version info |
/// | **Decision** | "Can I start?" | "How should I process this?" |
///
/// # Examples
///
/// ```ignore
/// // Scheduling decision (operation level)
/// if coordinator.has_active_full_index().await? {
///     return Err("Full index already running");  // OperationType level
/// }
///
/// // Processing decision (file level)
/// match &file_state.operation_type {
///     IndexOperationType::Full { total_batches, batch_size, .. } => {
///         // Use batch info to resume from checkpoint
///         checkpoint.batch_index = current_batch % total_batches;
///     }
///     IndexOperationType::Hot { trigger } => {
///         // Log what triggered this update
///         info!("Processing hot update triggered by {:?}", trigger);
///     }
///     _ => {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexOperationType {
    /// Full index of entire project
    Full {
        /// Total number of batches
        total_batches: usize,
        /// Batch size
        batch_size: usize,
        /// Root directory being indexed
        root_dir: String,
    },
    /// Hot update for changed files
    Hot {
        /// What triggered the update
        trigger: ChangeTrigger,
    },
    /// Incremental update based on version diff
    Incremental {
        /// Base version to diff against
        base_version: u64,
    },
}

/// What triggered a hot update
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeTrigger {
    /// File system event
    FileSystem,
    /// Manual trigger
    Manual,
    /// Scheduled/periodic check
    Scheduled,
    /// API request
    Api,
}

impl std::fmt::Display for ChangeTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeTrigger::FileSystem => write!(f, "filesystem"),
            ChangeTrigger::Manual => write!(f, "manual"),
            ChangeTrigger::Scheduled => write!(f, "scheduled"),
            ChangeTrigger::Api => write!(f, "api"),
        }
    }
}

/// Checkpoint for resumable indexing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Current batch index
    pub batch_index: usize,
    /// Current file offset within batch
    pub file_offset: usize,
    /// Modules completed for current phase
    pub completed_modules: Vec<ModuleType>,
    /// Current phase of indexing
    pub phase: IndexPhase,
    /// Timestamp when checkpoint was created
    pub timestamp: DateTime<Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(batch_index: usize, file_offset: usize, phase: IndexPhase) -> Self {
        Self {
            batch_index,
            file_offset,
            completed_modules: Vec::new(),
            phase,
            timestamp: Utc::now(),
        }
    }

    /// Mark a module as completed
    pub fn mark_module_complete(&mut self, module: ModuleType) {
        if !self.completed_modules.contains(&module) {
            self.completed_modules.push(module);
        }
    }

    /// Check if a module is completed
    pub fn is_module_complete(&self, module: ModuleType) -> bool {
        self.completed_modules.contains(&module)
    }

    /// Get progress percentage for current batch
    pub fn batch_progress(&self, batch_size: usize) -> f32 {
        if batch_size == 0 {
            return 0.0;
        }
        (self.file_offset as f32 / batch_size as f32) * 100.0
    }
}

/// Indexing phase for full index operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum IndexPhase {
    /// Scanning files
    #[default]
    Scanning,
    /// Parsing files
    Parsing,
    /// Building relation index
    RelationBuilding,
    /// Generating summaries
    SummaryGenerating,
    /// Generating embeddings
    Embedding,
    /// Storing to backends
    Storing,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

impl std::fmt::Display for IndexPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexPhase::Scanning => write!(f, "scanning"),
            IndexPhase::Parsing => write!(f, "parsing"),
            IndexPhase::RelationBuilding => write!(f, "relation_building"),
            IndexPhase::SummaryGenerating => write!(f, "summary_generating"),
            IndexPhase::Embedding => write!(f, "embedding"),
            IndexPhase::Storing => write!(f, "storing"),
            IndexPhase::Completed => write!(f, "completed"),
            IndexPhase::Failed => write!(f, "failed"),
        }
    }
}

/// Module update state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleUpdateState {
    /// Pending - initial state
    Pending,
    /// Updating - currently being processed
    Updating,
    /// Success - update completed successfully
    Success,
    /// Failed - update failed, will retry
    Failed,
    /// Retrying - waiting for next retry attempt
    Retrying { next_attempt: DateTime<Utc> },
    /// DeadLetter - max retries exceeded, needs manual intervention
    DeadLetter,
}

impl ModuleUpdateState {
    /// Check if this is a terminal state (Success or DeadLetter)
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            ModuleUpdateState::Success | ModuleUpdateState::DeadLetter
        )
    }

    /// Check if this state should be retried
    pub const fn should_retry(&self) -> bool {
        matches!(
            self,
            ModuleUpdateState::Failed | ModuleUpdateState::Retrying { .. }
        )
    }

    /// Check if this state allows querying (Success, Failed, or Retrying)
    /// Note: Failed/Retrying states allow querying old data (eventual consistency)
    pub const fn is_queryable(&self) -> bool {
        matches!(
            self,
            ModuleUpdateState::Success
                | ModuleUpdateState::Failed
                | ModuleUpdateState::Retrying { .. }
        )
    }

    /// Check if this state is currently updating
    pub const fn is_updating(&self) -> bool {
        matches!(self, ModuleUpdateState::Updating)
    }
}

/// Single module update record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleUpdateRecord {
    /// Current state
    pub state: ModuleUpdateState,
    /// Last attempt timestamp
    pub last_attempt: Option<DateTime<Utc>>,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

impl Default for ModuleUpdateRecord {
    fn default() -> Self {
        Self {
            state: ModuleUpdateState::Pending,
            last_attempt: None,
            retry_count: 0,
            error_message: None,
        }
    }
}

/// Maximum retry count before entering dead letter queue
pub const MAX_RETRY_COUNT: u32 = 3;

/// Base retry delay in seconds
pub const BASE_RETRY_DELAY_SECS: u64 = 5;

/// Calculate retry delay using exponential backoff
pub fn calculate_retry_delay(retry_count: u32) -> Duration {
    let base_delay = Duration::from_secs(BASE_RETRY_DELAY_SECS);
    let multiplier = 2_u32.pow(retry_count.saturating_sub(1));
    base_delay.saturating_mul(multiplier)
}

/// File update state tracking all modules for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUpdateState {
    /// File path
    pub file_path: String,
    /// Project ID for multi-project isolation
    pub project_id: i64,
    /// Update version number (prevents old updates from overwriting new ones)
    pub version: u64,
    /// Change type (Added, Modified, Deleted)
    pub change_type: FileChangeType,
    /// Operation type (Full, Hot, Incremental)
    pub operation_type: IndexOperationType,
    /// Module-specific update states
    pub module_states: HashMap<ModuleType, ModuleUpdateRecord>,
    /// Checkpoint for resumable indexing (only used for full index)
    pub checkpoint: Option<Checkpoint>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl FileUpdateState {
    /// Create a new file update state for hot updates
    pub fn new(
        file_path: String,
        version: u64,
        change_type: FileChangeType,
        project_id: i64,
    ) -> Self {
        Self::with_operation_type(
            file_path,
            version,
            change_type,
            project_id,
            IndexOperationType::Hot {
                trigger: ChangeTrigger::Manual,
            },
        )
    }

    /// Create a new file update state with specific operation type
    pub fn with_operation_type(
        file_path: String,
        version: u64,
        change_type: FileChangeType,
        project_id: i64,
        operation_type: IndexOperationType,
    ) -> Self {
        let now = Utc::now();
        let mut module_states = HashMap::new();

        // Initialize all modules to Pending
        for module in ModuleType::all() {
            module_states.insert(module, ModuleUpdateRecord::default());
        }

        Self {
            file_path,
            project_id,
            version,
            change_type,
            operation_type,
            module_states,
            checkpoint: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new file update state for full index operations
    pub fn for_full_index(
        file_path: String,
        version: u64,
        project_id: i64,
        total_batches: usize,
        batch_size: usize,
        root_dir: String,
    ) -> Self {
        let change_type = FileChangeType::Added;
        let operation_type = IndexOperationType::Full {
            total_batches,
            batch_size,
            root_dir,
        };
        let mut state =
            Self::with_operation_type(file_path, version, change_type, project_id, operation_type);
        state.checkpoint = Some(Checkpoint::new(0, 0, IndexPhase::Scanning));
        state
    }

    /// Update checkpoint
    pub fn update_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoint = Some(checkpoint);
        self.updated_at = Utc::now();
    }

    /// Get current checkpoint
    pub fn checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoint.as_ref()
    }

    /// Check if this is a full index operation
    pub fn is_full_index(&self) -> bool {
        matches!(self.operation_type, IndexOperationType::Full { .. })
    }

    /// Check if this is a hot update
    pub fn is_hot_update(&self) -> bool {
        matches!(self.operation_type, IndexOperationType::Hot { .. })
    }

    /// Get batch info if this is a full index
    pub fn batch_info(&self) -> Option<(usize, usize)> {
        match &self.operation_type {
            IndexOperationType::Full {
                total_batches,
                batch_size,
                ..
            } => Some((*total_batches, *batch_size)),
            _ => None,
        }
    }

    /// Get module state (returns default if not found)
    pub fn get_module_state(&self, module: ModuleType) -> &ModuleUpdateRecord {
        self.module_states.get(&module).unwrap_or_else(|| {
            // Return a static default for missing entries
            // This is safe because the default has no references
            static DEFAULT: std::sync::OnceLock<ModuleUpdateRecord> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(ModuleUpdateRecord::default)
        })
    }

    /// Update module state
    pub fn update_module_state(&mut self, module: ModuleType, state: ModuleUpdateState) {
        if let Some(record) = self.module_states.get_mut(&module) {
            record.state = state;
            record.last_attempt = Some(Utc::now());
            self.updated_at = Utc::now();
        }
    }

    /// Mark module as successfully updated
    pub fn mark_module_success(&mut self, module: ModuleType) {
        if let Some(record) = self.module_states.get_mut(&module) {
            record.state = ModuleUpdateState::Success;
            record.last_attempt = Some(Utc::now());
            record.error_message = None;
            self.updated_at = Utc::now();
        }
    }

    /// Mark module as failed (auto-increments retry count)
    pub fn mark_module_failed(&mut self, module: ModuleType, error: String) {
        if let Some(record) = self.module_states.get_mut(&module) {
            record.retry_count += 1;
            record.error_message = Some(error);

            // Determine next state based on retry count
            if record.retry_count >= MAX_RETRY_COUNT {
                record.state = ModuleUpdateState::DeadLetter;
            } else {
                let delay = calculate_retry_delay(record.retry_count);
                record.state = ModuleUpdateState::Retrying {
                    next_attempt: Utc::now() + chrono::Duration::seconds(delay.as_secs() as i64),
                };
            }

            record.last_attempt = Some(Utc::now());
            self.updated_at = Utc::now();
        }
    }

    /// Check if all modules are in Success state
    pub fn all_success(&self) -> bool {
        self.module_states
            .values()
            .all(|r| matches!(r.state, ModuleUpdateState::Success))
    }

    /// Check if any module has failures (Failed, Retrying, or DeadLetter)
    pub fn has_failures(&self) -> bool {
        self.module_states.values().any(|r| {
            matches!(
                r.state,
                ModuleUpdateState::Failed
                    | ModuleUpdateState::Retrying { .. }
                    | ModuleUpdateState::DeadLetter
            )
        })
    }

    /// Check if any module is currently updating
    pub fn is_updating(&self) -> bool {
        self.module_states
            .values()
            .any(|r| matches!(r.state, ModuleUpdateState::Updating))
    }

    /// Get modules that need retry (with their next attempt time)
    pub fn get_modules_to_retry(&self) -> Vec<(ModuleType, DateTime<Utc>)> {
        self.module_states
            .iter()
            .filter_map(|(module, record)| {
                if let ModuleUpdateState::Retrying { next_attempt } = record.state {
                    Some((*module, next_attempt))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if this file version is queryable
    /// Rule: at least one module must be in a queryable state
    pub fn is_queryable(&self) -> bool {
        self.module_states.values().any(|r| r.state.is_queryable())
    }

    /// Get summary of update status for this file
    pub fn get_status_summary(&self) -> FileUpdateStatusSummary {
        let mut success_count = 0;
        let mut failed_count = 0;
        let mut pending_count = 0;
        let mut updating_count = 0;
        let mut dead_letter_count = 0;

        for record in self.module_states.values() {
            match record.state {
                ModuleUpdateState::Success => success_count += 1,
                ModuleUpdateState::Failed | ModuleUpdateState::Retrying { .. } => failed_count += 1,
                ModuleUpdateState::Pending => pending_count += 1,
                ModuleUpdateState::Updating => updating_count += 1,
                ModuleUpdateState::DeadLetter => dead_letter_count += 1,
            }
        }

        FileUpdateStatusSummary {
            total_modules: self.module_states.len(),
            success_count,
            failed_count,
            pending_count,
            updating_count,
            dead_letter_count,
            is_complete: self.all_success(),
            has_failures: self.has_failures(),
        }
    }
}

/// Summary of file update status
#[derive(Debug, Clone, Default)]
pub struct FileUpdateStatusSummary {
    /// Total number of modules
    pub total_modules: usize,
    /// Number of successful modules
    pub success_count: usize,
    /// Number of failed/retrying modules
    pub failed_count: usize,
    /// Number of pending modules
    pub pending_count: usize,
    /// Number of updating modules
    pub updating_count: usize,
    /// Number of dead letter modules
    pub dead_letter_count: usize,
    /// Whether all modules are complete (success)
    pub is_complete: bool,
    /// Whether any module has failures
    pub has_failures: bool,
}
/// Unified index state query interface
///
/// This trait provides a unified way to query index state across
/// both full index and hot update operations.
pub trait IndexStateQuery {
    /// Get state for a specific file
    fn get_file_state(&self, file_path: &Path) -> Option<FileUpdateState>;

    /// Get all files in a specific phase (for full index)
    fn get_files_in_phase(&self, phase: IndexPhase) -> Vec<FileUpdateState>;

    /// Get files that can be resumed (have checkpoints)
    fn get_resumable_files(&self) -> Vec<FileUpdateState>;

    /// Check if all files are complete
    fn all_complete(&self) -> bool;

    /// Get summary report
    fn get_report(&self) -> IndexStateReport;
}

/// Comprehensive index state report
#[derive(Debug, Clone, Default)]
pub struct IndexStateReport {
    /// Operation type breakdown
    pub operation_breakdown: HashMap<String, usize>,
    /// Files in dead letter queue
    pub dead_letters: Vec<String>,
    /// Files currently updating
    pub updating_files: Vec<String>,
    /// Files that can be resumed
    pub resumable_files: Vec<String>,
    /// Current phase distribution (for full index)
    pub phase_distribution: HashMap<IndexPhase, usize>,
}

impl IndexStateReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self::default()
    }
    /// Check if there are any issues
    pub fn has_issues(&self) -> bool {
        !self.dead_letters.is_empty()
    }

    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Update Status: {} files in dead letter",
            self.dead_letters.len()
        )
    }
}

/// Error types for state tracking operations
#[derive(Debug, thiserror::Error)]
pub enum StateTrackerError {
    /// State not found for the given file
    #[error("State not found for file: {0}")]
    StateNotFound(String),

    /// Version mismatch (old update trying to overwrite newer one)
    #[error("Version mismatch for file {file}: expected {expected}, found {found}")]
    VersionMismatch {
        /// File path
        file: String,
        /// Expected version
        expected: u64,
        /// Found version
        found: u64,
    },

    /// Invalid state transition
    #[error("Invalid state transition for {module} on {file}: from {from:?} to {to:?}")]
    InvalidTransition {
        /// File path
        file: String,
        /// Module type
        module: ModuleType,
        /// From state
        from: ModuleUpdateState,
        /// To state
        to: ModuleUpdateState,
    },
}

/// Trait for version checking
pub trait VersionChecker {
    /// Check if the given version is current for the file
    fn check_version(&self, file_path: &Path, expected_version: u64) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_module_type_display() {
        assert_eq!(ModuleType::Relation.to_string(), "relation");
        assert_eq!(ModuleType::Summary.to_string(), "summary");
        assert_eq!(ModuleType::Embedding.to_string(), "embedding");
    }

    #[test]
    fn test_module_update_state_is_terminal() {
        assert!(!ModuleUpdateState::Pending.is_terminal());
        assert!(!ModuleUpdateState::Updating.is_terminal());
        assert!(ModuleUpdateState::Success.is_terminal());
        assert!(!ModuleUpdateState::Failed.is_terminal());
        assert!(
            !ModuleUpdateState::Retrying {
                next_attempt: Utc::now()
            }
            .is_terminal()
        );
        assert!(ModuleUpdateState::DeadLetter.is_terminal());
    }

    #[test]
    fn test_module_update_state_should_retry() {
        assert!(!ModuleUpdateState::Pending.should_retry());
        assert!(!ModuleUpdateState::Updating.should_retry());
        assert!(!ModuleUpdateState::Success.should_retry());
        assert!(ModuleUpdateState::Failed.should_retry());
        assert!(
            ModuleUpdateState::Retrying {
                next_attempt: Utc::now()
            }
            .should_retry()
        );
        assert!(!ModuleUpdateState::DeadLetter.should_retry());
    }

    #[test]
    fn test_calculate_retry_delay() {
        // First retry: 5 seconds
        let delay1 = calculate_retry_delay(1);
        assert_eq!(delay1, Duration::from_secs(5));

        // Second retry: 10 seconds
        let delay2 = calculate_retry_delay(2);
        assert_eq!(delay2, Duration::from_secs(10));

        // Third retry: 20 seconds
        let delay3 = calculate_retry_delay(3);
        assert_eq!(delay3, Duration::from_secs(20));
    }

    #[test]
    fn test_file_update_state_new() {
        let state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Added, 1);

        assert_eq!(state.file_path, "test.rs");
        assert_eq!(state.version, 1);
        assert_eq!(state.change_type, FileChangeType::Added);
        assert_eq!(state.module_states.len(), 5);

        // All modules should be pending
        for module in ModuleType::all() {
            let record = state.get_module_state(module);
            assert!(matches!(record.state, ModuleUpdateState::Pending));
            assert_eq!(record.retry_count, 0);
        }
    }

    #[test]
    fn test_mark_module_success() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        state.mark_module_success(ModuleType::Relation);

        let record = state.get_module_state(ModuleType::Relation);
        assert!(matches!(record.state, ModuleUpdateState::Success));
        assert!(record.last_attempt.is_some());
    }

    #[test]
    fn test_mark_module_failed() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        // First failure
        state.mark_module_failed(ModuleType::Summary, "API timeout".to_string());

        let record = state.get_module_state(ModuleType::Summary);
        assert_eq!(record.retry_count, 1);
        assert!(matches!(record.state, ModuleUpdateState::Retrying { .. }));
        assert_eq!(record.error_message.as_ref().unwrap(), "API timeout");

        // Second failure
        state.mark_module_failed(ModuleType::Summary, "API timeout again".to_string());
        let record = state.get_module_state(ModuleType::Summary);
        assert_eq!(record.retry_count, 2);
        assert!(matches!(record.state, ModuleUpdateState::Retrying { .. }));

        // Third failure - should enter dead letter
        state.mark_module_failed(ModuleType::Summary, "API timeout third".to_string());
        let record = state.get_module_state(ModuleType::Summary);
        assert_eq!(record.retry_count, 3);
        assert!(matches!(record.state, ModuleUpdateState::DeadLetter));

        // Fourth failure - should stay in dead letter
        state.mark_module_failed(ModuleType::Summary, "API timeout fourth".to_string());
        let record = state.get_module_state(ModuleType::Summary);
        assert_eq!(record.retry_count, 4);
        assert!(matches!(record.state, ModuleUpdateState::DeadLetter));
    }

    #[test]
    fn test_all_success() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        assert!(!state.all_success());

        state.mark_module_success(ModuleType::Relation);
        assert!(!state.all_success());

        state.mark_module_success(ModuleType::Summary);
        assert!(!state.all_success());

        state.mark_module_success(ModuleType::Embedding);
        assert!(!state.all_success());

        state.mark_module_success(ModuleType::Bm25);
        assert!(!state.all_success());

        state.mark_module_success(ModuleType::Export);
        assert!(state.all_success());
    }

    #[test]
    fn test_has_failures() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        assert!(!state.has_failures());

        state.mark_module_failed(ModuleType::Summary, "error".to_string());
        assert!(state.has_failures());
    }

    #[test]
    fn test_is_queryable() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        // Initially not queryable (all pending)
        assert!(!state.is_queryable());

        // After one success, should be queryable
        state.mark_module_success(ModuleType::Relation);
        assert!(state.is_queryable());
    }

    #[test]
    fn test_get_modules_to_retry() {
        let mut state = FileUpdateState::new("test.rs".to_string(), 1, FileChangeType::Modified, 1);

        // Initially no retries needed
        assert!(state.get_modules_to_retry().is_empty());

        // Mark as failed (should enter retrying state)
        state.mark_module_failed(ModuleType::Summary, "error".to_string());

        let retries = state.get_modules_to_retry();
        assert_eq!(retries.len(), 1);
        assert_eq!(retries[0].0, ModuleType::Summary);
    }

    #[test]
    fn test_update_stats() {
        let mut stats = UpdateStats {
            total_files: 10,
            ..Default::default()
        };
        stats.fully_updated = 5;
        stats.has_failures = 3;

        stats.module_success.insert(ModuleType::Relation, 8);
        stats.module_success.insert(ModuleType::Summary, 7);

        assert_eq!(stats.get_module_success(ModuleType::Relation), 8);
        assert_eq!(stats.get_module_success(ModuleType::Summary), 7);
        assert_eq!(stats.get_module_success(ModuleType::Embedding), 0);
    }
}

/// Update statistics for hot update operations
#[derive(Debug, Clone, Default)]
pub struct UpdateStats {
    /// Total number of files to update
    pub total_files: usize,
    /// Number of fully updated files
    pub fully_updated: usize,
    /// Number of files with failures
    pub has_failures: usize,
    /// Module-specific success counts
    pub module_success: HashMap<ModuleType, usize>,
}

impl UpdateStats {
    /// Get success count for a specific module
    pub fn get_module_success(&self, module: ModuleType) -> usize {
        *self.module_success.get(&module).unwrap_or(&0)
    }

    /// Increment success count for a specific module
    pub fn increment_module_success(&mut self, module: ModuleType) {
        *self.module_success.entry(module).or_insert(0) += 1;
    }
}
