//! Global debounce mechanism for hot update
//!
//! This module provides a global debounce timer that controls when
//! hot update operations should be triggered.
//!
//! # Design
//!
//! - Single global timer for all changes
//! - Configurable intervals for different scenarios
//! - Cache miss triggers debounce reset
//! - Time-based trigger for idle periods

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Global debounce configuration
#[derive(Debug, Clone, Copy)]
pub struct DebounceConfig {
    /// Short interval when changes are pending (default: 30 seconds)
    pub pending_interval: Duration,
    /// Maximum time to wait after a change before triggering (default: 5 minutes)
    pub max_wait_time: Duration,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            pending_interval: Duration::from_secs(30), // 30 seconds
            max_wait_time: Duration::from_secs(300),   // 5 minutes
        }
    }
}

impl DebounceConfig {
    /// Create a new config with custom intervals
    pub fn new(pending_interval_secs: u64, max_wait_time_secs: u64) -> Self {
        Self {
            pending_interval: Duration::from_secs(pending_interval_secs),
            max_wait_time: Duration::from_secs(max_wait_time_secs),
        }
    }

    /// Set pending interval
    pub fn with_pending_interval(mut self, secs: u64) -> Self {
        self.pending_interval = Duration::from_secs(secs);
        self
    }

    /// Set max wait time
    pub fn with_max_wait_time(mut self, secs: u64) -> Self {
        self.max_wait_time = Duration::from_secs(secs);
        self
    }
}

/// Global debounce state
#[derive(Debug)]
struct DebounceState {
    /// Last time update was triggered
    last_trigger: Instant,
    /// Whether there are recent changes pending
    has_pending_changes: bool,
    /// Time when first pending change occurred
    pending_since: Option<Instant>,
    /// Configuration
    config: DebounceConfig,
    /// Whether this is the first check (startup)
    is_first_check: bool,
}

impl DebounceState {
    fn new(config: DebounceConfig) -> Self {
        Self {
            last_trigger: Instant::now(),
            has_pending_changes: false,
            pending_since: None,
            config,
            is_first_check: true,
        }
    }

    /// Reset to initial state
    fn reset(&mut self) {
        self.last_trigger = Instant::now();
        self.has_pending_changes = false;
        self.pending_since = None;
    }

    /// Mark that changes have occurred
    fn mark_changes(&mut self) {
        self.has_pending_changes = true;
        if self.pending_since.is_none() {
            self.pending_since = Some(Instant::now());
        }
    }

    /// Check if update should be triggered
    fn should_update(&mut self, cache_miss: bool, force: bool) -> bool {
        // Force trigger always succeeds
        if force {
            self.reset();
            return true;
        }

        // First check after startup - allow immediate trigger
        if self.is_first_check {
            self.is_first_check = false;
            if cache_miss {
                self.reset();
                return true;
            }
        }

        // Cache miss indicates changes
        if cache_miss {
            self.mark_changes();
        }

        // Only check intervals if we have pending changes
        if !self.has_pending_changes {
            return false;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_trigger);

        // Check max wait time (safety limit for high-frequency changes)
        if let Some(pending_since) = self.pending_since {
            let pending_duration = now.duration_since(pending_since);
            if pending_duration >= self.config.max_wait_time {
                tracing::trace!(
                    "Max wait time reached ({}s), triggering update",
                    pending_duration.as_secs()
                );
                self.reset();
                return true;
            }
        }

        // If we have pending changes, use shorter interval
        if elapsed >= self.config.pending_interval {
            tracing::trace!(
                "Pending interval reached ({}s), triggering update",
                elapsed.as_secs()
            );
            self.reset();
            return true;
        }

        false
    }

    /// Get time until next potential update
    fn time_until_next(&self) -> Duration {
        // If no pending changes, return max duration (no update needed)
        if !self.has_pending_changes {
            return Duration::from_secs(u64::MAX);
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_trigger);

        if elapsed >= self.config.pending_interval {
            Duration::from_secs(0)
        } else {
            self.config.pending_interval - elapsed
        }
    }
}

/// Thread-safe global debounce timer
#[derive(Debug, Clone)]
pub struct GlobalDebounce {
    state: Arc<RwLock<DebounceState>>,
}

impl GlobalDebounce {
    /// Create a new global debounce with default config
    pub fn new() -> Self {
        Self::with_config(DebounceConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: DebounceConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(DebounceState::new(config))),
        }
    }

    /// Check if update should be triggered
    ///
    /// # Arguments
    ///
    /// * `cache_miss` - Whether there was a cache miss (file changed)
    /// * `force` - Force trigger regardless of timing
    ///
    /// # Returns
    ///
    /// `true` if update should be triggered
    pub async fn should_update(&self, cache_miss: bool, force: bool) -> bool {
        // Fast path: check if we need to update with read lock first
        if !force && !cache_miss {
            let state = self.state.read().await;

            // If no pending changes, no need to update
            if !state.has_pending_changes {
                return false;
            }

            let now = Instant::now();
            let elapsed = now.duration_since(state.last_trigger);

            // If we haven't reached pending interval, check max wait time
            if elapsed < state.config.pending_interval {
                // Check max wait time
                if let Some(pending_since) = state.pending_since {
                    let pending_duration = now.duration_since(pending_since);
                    if pending_duration < state.config.max_wait_time {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        // Slow path: acquire write lock and perform full check
        let mut state = self.state.write().await;
        state.should_update(cache_miss, force)
    }

    /// Mark that changes have occurred (without checking)
    pub async fn mark_changes(&self) {
        let mut state = self.state.write().await;
        state.mark_changes();
    }

    /// Reset the debounce timer
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        state.reset();
    }

    /// Get time until next potential update
    pub async fn time_until_next(&self) -> Duration {
        let state = self.state.read().await;
        state.time_until_next()
    }

    /// Force immediate update on next check
    pub async fn force_next(&self) {
        let mut state = self.state.write().await;
        state.last_trigger =
            Instant::now() - state.config.pending_interval - Duration::from_secs(1);
    }

    /// Check if there are pending changes
    pub async fn has_pending_changes(&self) -> bool {
        let state = self.state.read().await;
        state.has_pending_changes
    }

    /// Check if processing should happen now (for event loop)
    ///
    /// This is a simplified check that returns true if:
    /// - There are pending changes AND
    /// - The pending interval (or max wait time) has elapsed since the
    ///   first pending change arrived
    ///
    /// The window is measured from [`DebounceState::pending_since`] rather
    /// than the last trigger: measuring from the last trigger would forward
    /// the first event of a new burst immediately once the previous batch
    /// aged past the interval, splitting duplicate file events across
    /// batches and re-processing the same file in multiple operations.
    pub async fn should_process(&self) -> bool {
        let mut state = self.state.write().await;

        if !state.has_pending_changes {
            return false;
        }

        let Some(pending_since) = state.pending_since else {
            return false;
        };
        let now = Instant::now();
        let pending_duration = now.duration_since(pending_since);

        // The max-wait safety limit is an upper bound on how long a change
        // may wait before processing, even for bursty high-frequency changes.
        if pending_duration >= state.config.pending_interval
            || pending_duration >= state.config.max_wait_time
        {
            state.reset();
            return true;
        }

        false
    }

    /// Get current configuration
    pub async fn config(&self) -> DebounceConfig {
        let state = self.state.read().await;
        state.config
    }

    /// Update configuration
    pub async fn set_config(&self, config: DebounceConfig) {
        let mut state = self.state.write().await;
        state.config = config;
    }

    /// Wait until next update should be triggered
    pub async fn wait_for_next(&self) {
        let duration = self.time_until_next().await;
        if duration > Duration::from_secs(0) {
            tokio::time::sleep(duration).await;
        }
    }
}

impl Default for GlobalDebounce {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for debounce configuration
#[derive(Debug, Default)]
pub struct DebounceConfigBuilder {
    pending_interval: Option<u64>,
    max_wait_time: Option<u64>,
}

impl DebounceConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set pending interval in seconds
    pub fn pending_interval(mut self, secs: u64) -> Self {
        self.pending_interval = Some(secs);
        self
    }

    /// Set max wait time in seconds
    pub fn max_wait_time(mut self, secs: u64) -> Self {
        self.max_wait_time = Some(secs);
        self
    }

    /// Build the configuration
    pub fn build(self) -> DebounceConfig {
        let mut config = DebounceConfig::default();

        if let Some(secs) = self.pending_interval {
            config.pending_interval = Duration::from_secs(secs);
        }
        if let Some(secs) = self.max_wait_time {
            config.max_wait_time = Duration::from_secs(secs);
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_global_debounce_basic() {
        let debounce = GlobalDebounce::with_config(DebounceConfig {
            pending_interval: Duration::from_millis(50),
            max_wait_time: Duration::from_secs(10),
        });

        // First check with cache miss should trigger
        assert!(debounce.should_update(true, false).await);

        // Immediate second check should not trigger
        assert!(!debounce.should_update(false, false).await);

        // Wait for pending interval
        tokio::time::sleep(Duration::from_millis(60)).await;

        // With cache miss, should trigger after pending interval
        assert!(debounce.should_update(true, false).await);
    }

    #[tokio::test]
    async fn test_global_debounce_force() {
        let debounce = GlobalDebounce::with_config(DebounceConfig {
            pending_interval: Duration::from_secs(30),
            max_wait_time: Duration::from_secs(300),
        });

        // First trigger
        assert!(debounce.should_update(true, false).await);

        // Force should trigger immediately
        assert!(debounce.should_update(false, true).await);
    }

    #[tokio::test]
    async fn test_debounce_config_builder() {
        let config = DebounceConfigBuilder::new()
            .pending_interval(60)
            .max_wait_time(600)
            .build();

        assert_eq!(config.pending_interval, Duration::from_secs(60));
        assert_eq!(config.max_wait_time, Duration::from_secs(600));
    }

    #[tokio::test]
    async fn test_time_until_next() {
        let debounce = GlobalDebounce::with_config(DebounceConfig {
            pending_interval: Duration::from_millis(50),
            max_wait_time: Duration::from_secs(10),
        });

        // Trigger first update
        debounce.should_update(true, false).await;

        // Should have some time until next
        let time_until = debounce.time_until_next().await;
        assert!(time_until > Duration::from_secs(0));
    }
}
