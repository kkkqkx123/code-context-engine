//! State tracker listener for event-driven state synchronization
//!
//! Bridges operation-level events to file-level state tracking.
//! Implements the EventListener trait to synchronize processor results
//! to UpdateStateTracker through the event bus.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use super::event_bus::{EventListener, EventType};
use super::events::OperationEvent;
use super::listener_priority;
use crate::index_state::ModuleType;
use crate::index_state_tracker::UpdateStateTracker;

/// StateTracker adapter for event-driven architecture
///
/// This listener bridges operation-level events to file-level state updates.
/// It synchronizes processor failures and successes to UpdateStateTracker
/// through the event bus, providing loose coupling between processors and state tracking.
pub struct StateTrackerListener {
    /// Project ID for multi-project isolation
    project_id: i64,

    /// The state tracker to update
    state_tracker: Arc<UpdateStateTracker>,

    /// Whether to track successful module completions
    /// If true, will mark all modules as successful when FileCompleted event is received
    track_successes: bool,
}

impl StateTrackerListener {
    /// Create a state tracker listener for a specific project
    ///
    /// Project ID is required to ensure proper multi-project isolation.
    /// All events from other projects will be silently ignored.
    pub fn with_project(project_id: i64, state_tracker: Arc<UpdateStateTracker>) -> Self {
        Self {
            project_id,
            state_tracker,
            track_successes: true,
        }
    }

    /// Control success tracking
    pub fn with_track_successes(mut self, track: bool) -> Self {
        self.track_successes = track;
        self
    }

    /// Get project ID
    pub fn project_id(&self) -> i64 {
        self.project_id
    }

    /// Parse module name string to ModuleType
    fn parse_module_name(&self, module_name: &str) -> Result<ModuleType> {
        match module_name {
            "relation" => Ok(ModuleType::Relation),
            "summary" => Ok(ModuleType::Summary),
            "embedding" => Ok(ModuleType::Embedding),
            "bm25" => Ok(ModuleType::Bm25),
            "export" => Ok(ModuleType::Export),
            _ => {
                tracing::error!(module = %module_name, "Unknown module type");
                Err(anyhow!("Unknown module type: {}", module_name))
            }
        }
    }
}

#[async_trait]
impl EventListener for StateTrackerListener {
    async fn on_event(&self, event: &OperationEvent) -> Result<()> {
        // Filter events by project ID to prevent cross-project pollution
        if event.project_id() != self.project_id {
            return Ok(());
        }

        match event {
            // Handle file processing failure
            OperationEvent::FileFailed {
                file_path,
                module,
                error,
                ..
            } => {
                let module_type = match self.parse_module_name(module) {
                    Ok(mt) => mt,
                    Err(e) => {
                        // Graceful degradation: log error but don't fail the entire event
                        tracing::warn!(
                            module = %module,
                            error = %e,
                            file = %file_path,
                            "Invalid module name in FileFailed event, skipping state update"
                        );
                        return Ok(());
                    }
                };

                self.state_tracker
                    .mark_failed(Path::new(file_path), module_type, error.clone())
                    .await
                    .map_err(|e| {
                        anyhow!("Failed to mark module as failed for {}: {}", file_path, e)
                    })?;

                tracing::trace!(
                    file = %file_path,
                    module = %module,
                    error = %error,
                    "StateTracker: marked module as failed"
                );
            }

            // Handle file processing completion with module-level details
            OperationEvent::FileCompleted {
                file_path,
                successfully_processed_modules,
                failed_modules,
                ..
            } => {
                // Mark successful modules
                for module_name in successfully_processed_modules {
                    let module_type = match self.parse_module_name(module_name) {
                        Ok(mt) => mt,
                        Err(e) => {
                            // Graceful degradation: log and continue with other modules
                            tracing::warn!(
                                module = %module_name,
                                error = %e,
                                file = %file_path,
                                "Invalid module name in FileCompleted event, skipping success mark"
                            );
                            continue;
                        }
                    };

                    if let Err(e) = self
                        .state_tracker
                        .mark_success(Path::new(file_path), module_type)
                        .await
                    {
                        tracing::error!(
                            file = %file_path,
                            module = %module_name,
                            error = %e,
                            "Failed to mark module as success"
                        );
                        // Continue processing other modules even if one fails
                        continue;
                    }
                }

                // Mark failed modules (from FileCompleted event)
                for (module_name, error_msg) in failed_modules {
                    let module_type = match self.parse_module_name(module_name) {
                        Ok(mt) => mt,
                        Err(e) => {
                            tracing::warn!(
                                module = %module_name,
                                error = %e,
                                file = %file_path,
                                "Invalid module name in FileCompleted failed_modules, skipping"
                            );
                            continue;
                        }
                    };

                    if let Err(e) = self
                        .state_tracker
                        .mark_failed(Path::new(file_path), module_type, error_msg.clone())
                        .await
                    {
                        tracing::error!(
                            file = %file_path,
                            module = %module_name,
                            error = %e,
                            "Failed to mark module as failed"
                        );
                        // Continue processing other modules
                        continue;
                    }
                }
            }

            _ => {
                // Ignore other event types
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "StateTrackerListener"
    }

    fn filter(&self) -> Option<Vec<EventType>> {
        // Only care about these event types
        Some(vec![
            EventType::FileFailed,
            EventType::FileCompleted,
            EventType::BatchCompleted,
            EventType::Completed,
        ])
    }

    /// StateTrackerListener has priority 1 (system-critical state updates)
    /// This ensures state updates happen before other listeners
    fn priority(&self) -> u32 {
        listener_priority::SYSTEM_STATE_UPDATE
    }

    /// State updates should complete quickly, 5 seconds should be sufficient
    fn timeout_ms(&self) -> Option<u64> {
        Some(5000) // 5 second timeout for state updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_tracker_listener_creation() {
        let tracker = Arc::new(UpdateStateTracker::new(1));
        let listener = StateTrackerListener::with_project(0, tracker);

        assert_eq!(listener.name(), "StateTrackerListener");
        assert_eq!(listener.project_id(), 0);
        assert!(listener.filter().is_some());

        let filter = listener.filter().unwrap();
        assert_eq!(filter.len(), 4); // FileFailed, FileCompleted, BatchCompleted, Completed
    }

    #[tokio::test]
    async fn test_state_tracker_listener_with_project() {
        let tracker = Arc::new(UpdateStateTracker::new(1));
        let listener = StateTrackerListener::with_project(42, tracker);

        assert_eq!(listener.project_id(), 42);
    }

    #[tokio::test]
    async fn test_state_tracker_listener_parse_module() {
        let tracker = Arc::new(UpdateStateTracker::new(1));
        let listener = StateTrackerListener::with_project(0, tracker);

        assert!(listener.parse_module_name("relation").is_ok());
        assert!(listener.parse_module_name("summary").is_ok());
        assert!(listener.parse_module_name("embedding").is_ok());
        assert!(listener.parse_module_name("bm25").is_ok());
        assert!(listener.parse_module_name("export").is_ok());
        assert!(listener.parse_module_name("unknown").is_err());
    }

    #[tokio::test]
    async fn test_state_tracker_listener_on_event() {
        use crate::hot_update::FileChangeType;

        let tracker = Arc::new(UpdateStateTracker::new(1));

        // Create initial state for the file
        tracker
            .create_update(
                std::path::Path::new("/test/file.rs"),
                FileChangeType::Modified,
            )
            .await;

        let listener = StateTrackerListener::with_project(0, tracker);

        let event = OperationEvent::FileFailed {
            project_id: 0,
            operation_id: "op1".to_string(),
            file_path: "/test/file.rs".to_string(),
            module: "embedding".to_string(),
            error: "timeout".to_string(),
            retry_count: 0,
            timestamp_ms: 0,
        };

        match listener.on_event(&event).await {
            Ok(_) => {}
            Err(e) => panic!("Expected success but got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_state_tracker_listener_project_filtering() {
        use crate::hot_update::FileChangeType;

        let tracker = Arc::new(UpdateStateTracker::new(1));

        // Create initial state for the file
        tracker
            .create_update(
                std::path::Path::new("/test/file.rs"),
                FileChangeType::Modified,
            )
            .await;

        let listener = StateTrackerListener::with_project(42, tracker);

        // Event from different project (project_id=0)
        let event = OperationEvent::FileFailed {
            project_id: 0,
            operation_id: "op1".to_string(),
            file_path: "/test/file.rs".to_string(),
            module: "embedding".to_string(),
            error: "timeout".to_string(),
            retry_count: 0,
            timestamp_ms: 0,
        };

        // Should not process event from different project
        let result = listener.on_event(&event).await;
        assert!(result.is_ok(), "Should return Ok even when filtering event");

        // Event from same project (project_id=42)
        let event_same_project = OperationEvent::FileFailed {
            project_id: 42,
            operation_id: "op1".to_string(),
            file_path: "/test/file.rs".to_string(),
            module: "embedding".to_string(),
            error: "timeout".to_string(),
            retry_count: 0,
            timestamp_ms: 0,
        };

        let result = listener.on_event(&event_same_project).await;
        assert!(result.is_ok(), "Should process event from same project");
    }
}
