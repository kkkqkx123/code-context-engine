//! Periodic scan mode for hot update coordinator
//!
//! This module handles the degraded periodic scan mode when
//! file watch mode is not available or has degraded.

use std::time::Duration;

use crate::hot_update::debounce::GlobalDebounce;

/// Periodic scan task handle
///
/// Manages the background task that performs periodic file scanning
/// when in degraded mode (e.g., during event storms).
pub struct PeriodicScanTask {
    handle: tokio::task::JoinHandle<()>,
}

impl PeriodicScanTask {
    /// Start a new periodic scan task
    ///
    /// # Arguments
    ///
    /// * `debounce` - Global debounce instance for change batching
    /// * `interval_secs` - Scan interval in seconds
    pub fn start(debounce: GlobalDebounce, interval_secs: u64) -> Self {
        let handle = tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(interval_secs));

            loop {
                timer.tick().await;

                // Mark changes to trigger processing
                debounce.mark_changes().await;

                if debounce.should_process().await {
                    // Perform scan and detect changes
                    tracing::trace!("Periodic scan triggered in degraded mode");

                    // Note: Actual processing would be handled by HotUpdateCoordinator.update()
                    // This task only triggers the debounce mechanism
                }
            }
        });

        Self { handle }
    }

    /// Stop the periodic scan task
    pub fn stop(self) {
        self.handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_periodic_scan_task_creation() {
        let debounce = GlobalDebounce::default();
        let task = PeriodicScanTask::start(debounce, 5);
        task.stop();
    }
}
