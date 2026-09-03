//! Mode switch state machine for hot update coordinator
//!
//! This module implements the state machine for switching between
//! FileWatch and PeriodicScan modes based on event storm detection.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::state::HotUpdateMode;

/// Mode switch configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSwitchConfig {
    /// Storm threshold (events per second)
    pub storm_threshold: usize,

    /// Storm duration threshold (seconds)
    pub storm_duration_secs: u64,

    /// Recovery threshold (events per second)
    pub recovery_threshold: usize,

    /// Recovery duration threshold (seconds)
    pub recovery_duration_secs: u64,

    /// Degraded scan interval (seconds)
    pub degraded_scan_interval_secs: u64,
}

impl Default for ModeSwitchConfig {
    fn default() -> Self {
        Self {
            storm_threshold: 100,
            storm_duration_secs: 10,
            recovery_threshold: 50,
            recovery_duration_secs: 30,
            degraded_scan_interval_secs: 30,
        }
    }
}

/// Mode switch state machine
///
/// Manages transitions between FileWatch and PeriodicScan modes
/// based on event rate monitoring.
pub struct ModeStateMachine {
    /// Current mode
    pub current_mode: HotUpdateMode,

    /// Mode since time
    pub mode_since: Instant,

    /// Configuration
    pub config: ModeSwitchConfig,

    // Storm detection with sliding window
    /// Event timestamps in the sliding window
    event_timestamps: Vec<Instant>,

    /// Sliding window size (seconds)
    window_size_secs: u64,

    /// Time when storm threshold was first exceeded
    storm_exceeded_since: Option<Instant>,

    // Recovery detection
    /// Time when recovery threshold was first met
    recovery_below_since: Option<Instant>,
}

impl ModeStateMachine {
    /// Create a new mode state machine
    pub fn new(config: ModeSwitchConfig) -> Self {
        Self {
            current_mode: HotUpdateMode::FileWatch,
            mode_since: Instant::now(),
            config,
            event_timestamps: Vec::with_capacity(1000),
            window_size_secs: 1, // 1 second sliding window
            storm_exceeded_since: None,
            recovery_below_since: None,
        }
    }

    /// Create with default configuration
    pub fn new_default() -> Self {
        Self::new(ModeSwitchConfig::default())
    }

    /// Record an event and check if storm is detected
    ///
    /// Returns true if storm is detected and should trigger mode switch.
    pub fn record_event_and_check_storm(&mut self) -> bool {
        let now = Instant::now();

        // Add event timestamp
        self.event_timestamps.push(now);

        // Remove old events outside the window
        let window_start = now - Duration::from_secs(self.window_size_secs);
        self.event_timestamps.retain(|&ts| ts > window_start);

        // Check if exceeds threshold
        self.event_timestamps.len() > self.config.storm_threshold
    }

    /// Get current event rate (events per second)
    pub fn current_event_rate(&self) -> usize {
        // Return count in the sliding window
        self.event_timestamps.len()
    }

    /// Check if should degrade to periodic scan mode
    ///
    /// This checks if:
    /// 1. Currently in FileWatch mode
    /// 2. Event rate exceeds storm threshold
    /// 3. Has exceeded for the required duration
    pub fn should_degrade(&mut self) -> bool {
        if self.current_mode != HotUpdateMode::FileWatch {
            return false;
        }

        let event_rate = self.current_event_rate();
        let now = Instant::now();

        if event_rate > self.config.storm_threshold {
            // Track when we first exceeded the threshold
            if self.storm_exceeded_since.is_none() {
                self.storm_exceeded_since = Some(now);
            }

            // Check if we've exceeded for long enough
            if let Some(since) = self.storm_exceeded_since {
                let duration = now.duration_since(since);
                if duration >= Duration::from_secs(self.config.storm_duration_secs) {
                    return true;
                }
            }
        } else {
            // Reset if below threshold
            self.storm_exceeded_since = None;
        }

        false
    }

    /// Check if should recover to file watch mode
    ///
    /// This checks if:
    /// 1. Currently in PeriodicScan mode
    /// 2. Event rate is below recovery threshold
    /// 3. Has been below for the required duration
    pub fn should_recover(&mut self, event_rate: usize) -> bool {
        if self.current_mode != HotUpdateMode::PeriodicScan {
            return false;
        }

        let now = Instant::now();

        if event_rate < self.config.recovery_threshold {
            // Track when we first went below the threshold
            if self.recovery_below_since.is_none() {
                self.recovery_below_since = Some(now);
            }

            // Check if we've been below for long enough
            if let Some(since) = self.recovery_below_since {
                let duration = now.duration_since(since);
                if duration >= Duration::from_secs(self.config.recovery_duration_secs) {
                    return true;
                }
            }
        } else {
            // Reset if above threshold
            self.recovery_below_since = None;
        }

        false
    }

    /// Switch to periodic scan mode
    pub fn switch_to_periodic_scan(&mut self) {
        self.current_mode = HotUpdateMode::PeriodicScan;
        self.mode_since = Instant::now();
        self.storm_exceeded_since = None;
        self.recovery_below_since = None;
        self.event_timestamps.clear();
    }

    /// Switch to file watch mode
    pub fn switch_to_file_watch(&mut self) {
        self.current_mode = HotUpdateMode::FileWatch;
        self.mode_since = Instant::now();
        self.storm_exceeded_since = None;
        self.recovery_below_since = None;
        self.event_timestamps.clear();
    }

    /// Get time in current mode
    pub fn time_in_mode(&self) -> Duration {
        Instant::now().duration_since(self.mode_since)
    }

    /// Reset the state machine
    pub fn reset(&mut self) {
        self.current_mode = HotUpdateMode::FileWatch;
        self.mode_since = Instant::now();
        self.event_timestamps.clear();
        self.storm_exceeded_since = None;
        self.recovery_below_since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_switch_config_default() {
        let config = ModeSwitchConfig::default();
        assert_eq!(config.storm_threshold, 100);
        assert_eq!(config.storm_duration_secs, 10);
        assert_eq!(config.recovery_threshold, 50);
        assert_eq!(config.recovery_duration_secs, 30);
    }

    #[test]
    fn test_mode_state_machine_creation() {
        let machine = ModeStateMachine::new_default();
        assert_eq!(machine.current_mode, HotUpdateMode::FileWatch);
    }

    #[test]
    fn test_record_event_and_check_storm() {
        let mut machine = ModeStateMachine::new(ModeSwitchConfig {
            storm_threshold: 10,
            ..Default::default()
        });

        // Record events below threshold
        for _ in 0..10 {
            assert!(!machine.record_event_and_check_storm());
        }

        // Record one more to exceed threshold
        assert!(machine.record_event_and_check_storm());
    }

    #[test]
    fn test_should_degrade() {
        let mut machine = ModeStateMachine::new(ModeSwitchConfig {
            storm_threshold: 10,
            storm_duration_secs: 0, // Immediate for testing
            ..Default::default()
        });

        // Simulate high event rate
        for _ in 0..15 {
            machine.record_event_and_check_storm();
        }

        // Should degrade immediately (duration = 0)
        assert!(machine.should_degrade());
    }

    #[test]
    fn test_should_recover() {
        let mut machine = ModeStateMachine::new(ModeSwitchConfig {
            recovery_threshold: 50,
            recovery_duration_secs: 0, // Immediate for testing
            ..Default::default()
        });

        // Switch to periodic scan mode
        machine.switch_to_periodic_scan();

        // Check recovery with low event rate
        assert!(machine.should_recover(10));
    }

    #[test]
    fn test_mode_switching() {
        let mut machine = ModeStateMachine::new_default();

        assert_eq!(machine.current_mode, HotUpdateMode::FileWatch);

        machine.switch_to_periodic_scan();
        assert_eq!(machine.current_mode, HotUpdateMode::PeriodicScan);

        machine.switch_to_file_watch();
        assert_eq!(machine.current_mode, HotUpdateMode::FileWatch);
    }
}
