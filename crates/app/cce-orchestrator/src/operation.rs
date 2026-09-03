//! Operation management subsystem
//!
//! Provides centralized coordination for all indexing operations:
//! - **context**: Operation context and type definitions
//! - **queue**: Operation queue and priority management
//! - **checkpoint**: Checkpoint persistence and progress tracking
//!   - **module_retry**: Module-level retry management with exponential backoff
//!   - **file_diff**: File difference tracking for incremental updates
//! - **recovery**: Recovery from interrupted operations
//! - **coordinator**: Central operation coordinator
//! - **state**: Operation execution state tracking
//! - **events**: Operation event system for real-time monitoring
//! - **error_context**: Enhanced error diagnostics

pub mod checkpoint;
pub mod context;
pub mod coordinator;
pub mod error_context;
pub mod event_bus;
pub mod events;
pub mod file_diff;
pub mod listener_priority;
pub mod queue;
pub mod recovery;
pub mod state;
pub mod state_tracker_listener;

pub use checkpoint::{CheckpointManager, ParsedCheckpointEnvelope};
pub use context::{
    AggregatedMetrics, InitializationPhase, InitializationResult, ModuleFailure,
    ModuleProcessResult, OperationContext, OperationMetrics, OperationPhase,
    OperationProcessResult, OperationResult, OperationStatus, OperationSummary, OperationType,
};
pub use coordinator::OperationCoordinator;
pub use error_context::{ErrorCategory, OperationErrorContext, RecoverySuggestion};
pub use event_bus::{EventListener, EventType, OperationEventBus};
pub use events::OperationEvent;
pub use file_diff::FileDiffManager;
pub use queue::{ActiveOperation, OperationPriority, OperationQueue, PendingOperation};
pub use recovery::RecoveryManager;
pub use state::{OperationState, OperationStateSnapshot};
pub use state_tracker_listener::StateTrackerListener;
