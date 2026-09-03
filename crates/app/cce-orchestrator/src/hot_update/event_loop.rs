//! Event loop management for hot update coordinator
//!
//! This module provides event loop lifecycle management with
//! graceful shutdown support.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Event loop state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EventLoopState {
    /// Not started
    #[default]
    NotStarted,
    /// Running
    Running,
    /// Stopping
    Stopping,
    /// Stopped
    Stopped,
    /// Error state
    Error,
}

/// Event loop statistics
#[derive(Debug, Clone, Default)]
pub struct EventLoopStats {
    /// Events processed
    pub events_processed: u64,
    /// Events failed
    pub events_failed: u64,
    /// Started at
    pub started_at: Option<Instant>,
    /// Stopped at
    pub stopped_at: Option<Instant>,
}

/// Event loop manager
///
/// Manages the lifecycle of the event processing loop.
pub struct EventLoopManager {
    /// Task handle
    task_handle: Option<JoinHandle<()>>,

    /// Stop signal sender
    stop_tx: Option<mpsc::Sender<()>>,

    /// State
    state: EventLoopState,

    /// Statistics
    stats: EventLoopStats,
}

impl EventLoopManager {
    /// Create a new event loop manager
    pub fn new() -> Self {
        Self {
            task_handle: None,
            stop_tx: None,
            state: EventLoopState::NotStarted,
            stats: EventLoopStats::default(),
        }
    }

    /// Start the event loop
    pub fn start(&mut self, task_handle: JoinHandle<()>, stop_tx: mpsc::Sender<()>) {
        self.task_handle = Some(task_handle);
        self.stop_tx = Some(stop_tx);
        self.state = EventLoopState::Running;
        self.stats.started_at = Some(Instant::now());
    }

    /// Stop the event loop
    pub async fn stop(&mut self) -> Result<(), String> {
        if self.state != EventLoopState::Running {
            return Ok(());
        }

        self.state = EventLoopState::Stopping;

        // Send stop signal
        if let Some(ref stop_tx) = self.stop_tx {
            stop_tx
                .send(())
                .await
                .map_err(|e| format!("Failed to send stop signal: {}", e))?;
        }

        // Wait for task to finish
        if let Some(handle) = self.task_handle.take() {
            handle
                .await
                .map_err(|e| format!("Event loop task failed: {}", e))?;
        }

        self.state = EventLoopState::Stopped;
        self.stats.stopped_at = Some(Instant::now());

        Ok(())
    }

    /// Get state
    pub fn state(&self) -> EventLoopState {
        self.state
    }

    /// Get statistics
    pub fn stats(&self) -> &EventLoopStats {
        &self.stats
    }

    /// Get mutable statistics
    pub fn stats_mut(&mut self) -> &mut EventLoopStats {
        &mut self.stats
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.state == EventLoopState::Running
    }

    /// Record event processed
    pub fn record_event_processed(&mut self) {
        self.stats.events_processed += 1;
    }

    /// Record event failed
    pub fn record_event_failed(&mut self) {
        self.stats.events_failed += 1;
    }

    /// Get uptime
    pub fn uptime(&self) -> Option<Duration> {
        self.stats.started_at.map(|started| {
            if let Some(stopped) = self.stats.stopped_at {
                stopped.duration_since(started)
            } else {
                Instant::now().duration_since(started)
            }
        })
    }
}

impl Default for EventLoopManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_state_default() {
        let state = EventLoopState::default();
        assert_eq!(state, EventLoopState::NotStarted);
    }

    #[test]
    fn test_event_loop_manager_creation() {
        let manager = EventLoopManager::new();
        assert_eq!(manager.state(), EventLoopState::NotStarted);
        assert!(!manager.is_running());
    }

    #[test]
    fn test_event_loop_stats_default() {
        let stats = EventLoopStats::default();
        assert_eq!(stats.events_processed, 0);
        assert_eq!(stats.events_failed, 0);
        assert!(stats.started_at.is_none());
    }
}
