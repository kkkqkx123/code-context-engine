//! Configuration upgrade scheduler for Qdrant

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};

use crate::{
    error::QdrantError,
    upgrade::{ConfigUpgradeService, UpgradeProgress, UpgradeStatus},
};
use cce_utils::current_timestamp_ms;

/// Default check interval in seconds (1 hour)
pub const DEFAULT_CHECK_INTERVAL_SECS: u64 = 3600;

/// Default maximum concurrent upgrades
pub const DEFAULT_MAX_CONCURRENT_UPGRADES: usize = 1;

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Whether the scheduler is enabled
    pub enabled: bool,
    /// Check interval in seconds
    pub check_interval_secs: u64,
    /// Maximum number of concurrent upgrades
    pub max_concurrent_upgrades: usize,
    /// Upgrade time window
    pub upgrade_window: UpgradeWindow,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: DEFAULT_CHECK_INTERVAL_SECS,
            max_concurrent_upgrades: DEFAULT_MAX_CONCURRENT_UPGRADES,
            upgrade_window: UpgradeWindow::default(),
        }
    }
}

impl SchedulerConfig {
    /// Create new config with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable the scheduler
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Set check interval
    pub fn with_check_interval(mut self, secs: u64) -> Self {
        self.check_interval_secs = secs;
        self
    }

    /// Set max concurrent upgrades
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_upgrades = max;
        self
    }

    /// Set upgrade window
    pub fn with_upgrade_window(mut self, window: UpgradeWindow) -> Self {
        self.upgrade_window = window;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), QdrantError> {
        if self.check_interval_secs < 60 {
            return Err(QdrantError::InvalidConfig {
                field: "check_interval_secs".to_string(),
                reason: "must be at least 60 seconds".to_string(),
            });
        }
        if self.max_concurrent_upgrades == 0 {
            return Err(QdrantError::InvalidConfig {
                field: "max_concurrent_upgrades".to_string(),
                reason: "must be at least 1".to_string(),
            });
        }
        self.upgrade_window
            .validate()
            .map_err(|e| QdrantError::InvalidConfig {
                field: "upgrade_window".to_string(),
                reason: e,
            })
    }
}

/// Upgrade time window
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UpgradeWindow {
    /// Start hour (0-23)
    pub start_hour: u8,
    /// End hour (0-24, 24 means midnight next day)
    pub end_hour: u8,
}

impl Default for UpgradeWindow {
    fn default() -> Self {
        Self {
            start_hour: 0,
            end_hour: 24,
        }
    }
}

impl UpgradeWindow {
    /// Create all-day window
    pub fn all_day() -> Self {
        Self::default()
    }

    /// Create nighttime window (0-6)
    pub fn nighttime() -> Self {
        Self {
            start_hour: 0,
            end_hour: 6,
        }
    }

    /// Create business hours window (9-17)
    pub fn business_hours() -> Self {
        Self {
            start_hour: 9,
            end_hour: 17,
        }
    }

    /// Create evening window (18-23)
    pub fn evening() -> Self {
        Self {
            start_hour: 18,
            end_hour: 23,
        }
    }

    /// Create custom window
    pub fn custom(start: u8, end: u8) -> Result<Self, String> {
        let window = Self {
            start_hour: start,
            end_hour: end,
        };
        window.validate()?;
        Ok(window)
    }

    /// Validate the window
    pub fn validate(&self) -> Result<(), String> {
        if self.start_hour > 23 {
            return Err(format!("Start hour must be 0-23, got {}", self.start_hour));
        }
        if self.end_hour > 24 {
            return Err(format!("End hour must be 0-24, got {}", self.end_hour));
        }
        if self.start_hour >= self.end_hour && self.end_hour != 24 {
            return Err(format!(
                "Start hour ({}) must be less than end hour ({})",
                self.start_hour, self.end_hour
            ));
        }
        Ok(())
    }

    /// Check if current time is within the window
    pub fn is_within_window(&self, current_hour: u8) -> bool {
        if self.start_hour == 0 && self.end_hour == 24 {
            return true;
        }

        if self.start_hour < self.end_hour {
            current_hour >= self.start_hour && current_hour < self.end_hour
        } else {
            current_hour >= self.start_hour || current_hour < self.end_hour
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        if self.start_hour == 0 && self.end_hour == 24 {
            "All day".to_string()
        } else if self.start_hour == 0 && self.end_hour == 6 {
            "Night (00:00-06:00)".to_string()
        } else if self.start_hour == 9 && self.end_hour == 17 {
            "Business hours (09:00-17:00)".to_string()
        } else if self.start_hour == 18 && self.end_hour == 23 {
            "Evening (18:00-23:00)".to_string()
        } else {
            format!("{:02}:00-{:02}:00", self.start_hour, self.end_hour)
        }
    }
}

/// Scheduler status
#[derive(Debug, Clone)]
pub struct SchedulerStatus {
    /// Whether the scheduler is running
    pub is_running: bool,
    /// Last check timestamp (Unix ms)
    pub last_check_time: Option<u64>,
    /// Next scheduled check timestamp (Unix ms)
    pub next_check_time: Option<u64>,
    /// Number of pending upgrades
    pub pending_upgrades: usize,
    /// Number of currently running upgrades
    pub running_upgrades: usize,
    /// Total upgrades completed
    pub total_upgrades_completed: u64,
    /// Total upgrades failed
    pub total_upgrades_failed: u64,
}

/// Upgrade event
#[derive(Debug, Clone)]
pub enum UpgradeEvent {
    /// Check started
    CheckStarted { timestamp: u64 },
    /// Check completed
    CheckCompleted { timestamp: u64, duration_ms: u64 },
    /// Upgrade completed
    UpgradeCompleted {
        collection_name: String,
        duration_ms: u64,
    },
    /// Upgrade failed
    UpgradeFailed {
        collection_name: String,
        error: String,
    },
    /// Scheduled next check
    Scheduled { next_check_time: u64 },
}

/// Broadcast channel capacity for upgrade events
const EVENT_CHANNEL_CAPACITY: usize = 100;

/// Configuration upgrade scheduler
#[derive(Clone)]
pub struct ConfigUpgradeScheduler {
    config: Arc<RwLock<SchedulerConfig>>,
    services: Arc<Mutex<HashMap<String, ConfigUpgradeService>>>,
    is_running: Arc<RwLock<bool>>,
    last_check_time: Arc<Mutex<Option<u64>>>,
    total_completed: Arc<Mutex<u64>>,
    total_failed: Arc<Mutex<u64>>,
    event_sender: broadcast::Sender<UpgradeEvent>,
    /// Handle to the background scheduling task
    task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ConfigUpgradeScheduler {
    /// Create a new scheduler with default config
    pub fn new() -> Self {
        Self::with_config(SchedulerConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: SchedulerConfig) -> Self {
        let (event_sender, _event_receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            config: Arc::new(RwLock::new(config)),
            services: Arc::new(Mutex::new(HashMap::new())),
            is_running: Arc::new(RwLock::new(false)),
            last_check_time: Arc::new(Mutex::new(None)),
            total_completed: Arc::new(Mutex::new(0)),
            total_failed: Arc::new(Mutex::new(0)),
            event_sender,
            task_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Register an upgrade service for a collection
    pub async fn register_service(
        &self,
        collection_name: impl Into<String>,
        service: ConfigUpgradeService,
    ) {
        let name = collection_name.into();
        let mut services = self.services.lock().await;
        services.insert(name.clone(), service);
        tracing::info!("Registered upgrade service for collection: {}", name);
    }

    /// Unregister a service
    pub async fn unregister_service(&self, collection_name: &str) {
        let mut services = self.services.lock().await;
        services.remove(collection_name);
        tracing::info!(
            "Unregistered upgrade service for collection: {}",
            collection_name
        );
    }

    /// Subscribe to upgrade events
    pub fn subscribe(&self) -> broadcast::Receiver<UpgradeEvent> {
        self.event_sender.subscribe()
    }

    fn emit_event(&self, event: UpgradeEvent) {
        let _ = self.event_sender.send(event);
    }

    /// Start the scheduler
    pub async fn start(&self) -> Result<(), QdrantError> {
        {
            let running = self.is_running.read().await;
            if *running {
                tracing::warn!("Scheduler is already running");
                return Ok(());
            }
        }

        {
            let config = self.config.read().await;
            if !config.enabled {
                tracing::info!("Scheduler is disabled, not starting");
                return Ok(());
            }
        }

        {
            let mut running = self.is_running.write().await;
            *running = true;
        }

        tracing::info!("Starting upgrade scheduler");

        let config = self.config.clone();
        let services = self.services.clone();
        let is_running = self.is_running.clone();
        let last_check_time = self.last_check_time.clone();
        let total_completed = self.total_completed.clone();
        let total_failed = self.total_failed.clone();
        let event_sender = self.event_sender.clone();

        let handle = tokio::spawn(async move {
            let interval_secs = config.read().await.check_interval_secs;
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                {
                    let running = is_running.read().await;
                    if !*running {
                        tracing::info!("Scheduler stopping");
                        break;
                    }
                }

                interval.tick().await;

                let check_start = current_timestamp_ms();

                let event = UpgradeEvent::CheckStarted {
                    timestamp: check_start,
                };
                let _ = event_sender.send(event);

                {
                    let mut last = last_check_time.lock().await;
                    *last = Some(check_start);
                }

                let services_guard = services.lock().await;
                let service_count = services_guard.len();
                drop(services_guard);

                if service_count > 0 {
                    let scheduler = ConfigUpgradeScheduler {
                        config: config.clone(),
                        services: services.clone(),
                        is_running: is_running.clone(),
                        last_check_time: last_check_time.clone(),
                        total_completed: total_completed.clone(),
                        total_failed: total_failed.clone(),
                        event_sender: event_sender.clone(),
                        task_handle: Arc::new(Mutex::new(None)),
                    };

                    if let Err(e) = scheduler.perform_check().await {
                        tracing::error!("Error during upgrade check: {}", e);
                    }
                }

                let check_end = current_timestamp_ms();
                let event = UpgradeEvent::CheckCompleted {
                    timestamp: check_end,
                    duration_ms: check_end - check_start,
                };
                let _ = event_sender.send(event);
            }
        });

        {
            let mut task_handle = self.task_handle.lock().await;
            *task_handle = Some(handle);
        }

        Ok(())
    }

    /// Stop the scheduler
    pub async fn stop(&self) {
        let mut running = self.is_running.write().await;
        if *running {
            *running = false;
            tracing::info!("Scheduler stopped");
        }
        drop(running);

        let mut task_handle = self.task_handle.lock().await;
        if let Some(handle) = task_handle.take() {
            handle.abort();
            tracing::debug!("Scheduler background task aborted");
        }
    }

    /// Check if scheduler is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Perform a single check
    async fn perform_check(&self) -> Result<(), QdrantError> {
        let config = self.config.read().await;

        let current_hour = current_hour();
        if !config.upgrade_window.is_within_window(current_hour) {
            tracing::trace!(
                "Current hour {} is outside upgrade window {}",
                current_hour,
                config.upgrade_window.description()
            );
            return Ok(());
        }

        let running = self.count_running_upgrades().await;
        if running >= config.max_concurrent_upgrades {
            tracing::trace!(
                "Max concurrent upgrades ({}) reached, skipping check",
                config.max_concurrent_upgrades
            );
            return Ok(());
        }

        drop(config);

        let services = self.services.lock().await;
        let collection_names: Vec<String> = services.keys().cloned().collect();
        drop(services);

        for collection_name in collection_names {
            let running = self.count_running_upgrades().await;
            let config = self.config.read().await;
            if running >= config.max_concurrent_upgrades {
                tracing::trace!("Max concurrent upgrades reached, stopping check");
                break;
            }
            drop(config);

            let services = self.services.lock().await;
            if let Some(service) = services.get(&collection_name) {
                let start_time = current_timestamp_ms();

                match service.check_and_upgrade().await {
                    Ok(true) => {
                        let duration = current_timestamp_ms() - start_time;
                        tracing::info!(
                            "Collection {} upgraded successfully in {}ms",
                            collection_name,
                            duration
                        );

                        let mut completed = self.total_completed.lock().await;
                        *completed += 1;

                        self.emit_event(UpgradeEvent::UpgradeCompleted {
                            collection_name: collection_name.clone(),
                            duration_ms: duration,
                        });
                    }
                    Ok(false) => {
                        tracing::trace!("Collection {} does not need upgrade", collection_name);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to check/upgrade collection {}: {}",
                            collection_name,
                            e
                        );

                        let mut failed = self.total_failed.lock().await;
                        *failed += 1;

                        self.emit_event(UpgradeEvent::UpgradeFailed {
                            collection_name: collection_name.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    async fn count_running_upgrades(&self) -> usize {
        let services = self.services.lock().await;
        let mut count = 0;

        for service in services.values() {
            if let Some(upgrade) = service.get_current_upgrade("").await {
                if upgrade.status == UpgradeStatus::InProgress {
                    count += 1;
                }
            }
        }

        count
    }

    /// Get scheduler status
    pub async fn get_status(&self) -> SchedulerStatus {
        let last_check = *self.last_check_time.lock().await;
        let config = self.config.read().await;

        let next_check = last_check.map(|t| t + config.check_interval_secs * 1000);
        let running = self.count_running_upgrades().await;
        let completed = *self.total_completed.lock().await;
        let failed = *self.total_failed.lock().await;

        SchedulerStatus {
            is_running: *self.is_running.read().await,
            last_check_time: last_check,
            next_check_time: next_check,
            pending_upgrades: 0,
            running_upgrades: running,
            total_upgrades_completed: completed,
            total_upgrades_failed: failed,
        }
    }

    /// Get all current upgrades
    pub async fn get_all_current_upgrades(&self) -> HashMap<String, UpgradeProgress> {
        let services = self.services.lock().await;
        let mut all = HashMap::new();

        for (name, service) in services.iter() {
            if let Some(upgrade) = service.get_current_upgrade("").await {
                all.insert(name.clone(), upgrade);
            }
        }

        all
    }

    /// Update configuration
    pub async fn update_config(&self, new_config: SchedulerConfig) -> Result<(), String> {
        new_config.validate().map_err(|e| e.to_string())?;

        let mut config = self.config.write().await;
        *config = new_config;

        tracing::info!("Scheduler configuration updated");
        Ok(())
    }

    /// Get current configuration
    pub async fn get_config(&self) -> SchedulerConfig {
        self.config.read().await.clone()
    }

    /// Trigger manual check
    pub async fn trigger_manual_check(&self) -> Result<(), QdrantError> {
        if !self.is_running().await {
            return Err(QdrantError::api("Scheduler is not running"));
        }

        tracing::info!("Triggering manual check");
        self.perform_check().await
    }

    /// Trigger manual upgrade for a specific collection
    pub async fn trigger_manual_upgrade(&self, collection_name: &str) -> Result<bool, QdrantError> {
        if !self.is_running().await {
            return Err(QdrantError::api("Scheduler is not running"));
        }

        let services = self.services.lock().await;
        let service = services.get(collection_name).ok_or_else(|| {
            QdrantError::api(format!("Service not found for {}", collection_name))
        })?;

        tracing::info!("Triggering manual upgrade for {}", collection_name);

        let start_time = current_timestamp_ms();
        let result = service.check_and_upgrade().await;
        drop(services);

        match result {
            Ok(true) => {
                let duration = current_timestamp_ms() - start_time;
                tracing::info!("Manual upgrade completed in {}ms", duration);

                let mut completed = self.total_completed.lock().await;
                *completed += 1;

                self.emit_event(UpgradeEvent::UpgradeCompleted {
                    collection_name: collection_name.to_string(),
                    duration_ms: duration,
                });

                Ok(true)
            }
            Ok(false) => {
                tracing::info!("No upgrade needed for {}", collection_name);
                Ok(false)
            }
            Err(e) => {
                let mut failed = self.total_failed.lock().await;
                *failed += 1;

                self.emit_event(UpgradeEvent::UpgradeFailed {
                    collection_name: collection_name.to_string(),
                    error: e.to_string(),
                });

                Err(e)
            }
        }
    }
}

impl Default for ConfigUpgradeScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn current_hour() -> u8 {
    let timestamp = current_timestamp_ms() / 1000;
    ((timestamp / 3600) % 24) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_window() {
        let all_day = UpgradeWindow::all_day();
        assert!(all_day.is_within_window(0));
        assert!(all_day.is_within_window(12));
        assert!(all_day.is_within_window(23));

        let night = UpgradeWindow::nighttime();
        assert!(night.is_within_window(0));
        assert!(night.is_within_window(5));
        assert!(!night.is_within_window(12));

        let business = UpgradeWindow::business_hours();
        assert!(!business.is_within_window(8));
        assert!(business.is_within_window(9));
        assert!(business.is_within_window(16));
        assert!(!business.is_within_window(17));
    }

    #[test]
    fn test_window_validation() {
        let valid = UpgradeWindow::custom(9, 17);
        assert!(valid.is_ok());

        let invalid = UpgradeWindow::custom(25, 26);
        assert!(invalid.is_err());

        let invalid = UpgradeWindow::custom(17, 9);
        assert!(invalid.is_err());
    }

    #[test]
    fn test_scheduler_config() {
        let config = SchedulerConfig::new()
            .with_check_interval(7200)
            .with_max_concurrent(2)
            .with_upgrade_window(UpgradeWindow::nighttime());

        assert_eq!(config.check_interval_secs, 7200);
        assert_eq!(config.max_concurrent_upgrades, 2);
        assert_eq!(config.upgrade_window.start_hour, 0);
    }

    #[test]
    fn test_config_validation() {
        let valid = SchedulerConfig::new().with_check_interval(120);
        assert!(valid.validate().is_ok());

        let invalid = SchedulerConfig::new().with_check_interval(30);
        assert!(invalid.validate().is_err());

        let invalid = SchedulerConfig::new().with_max_concurrent(0);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_window_description() {
        assert_eq!(UpgradeWindow::all_day().description(), "All day");
        assert_eq!(
            UpgradeWindow::nighttime().description(),
            "Night (00:00-06:00)"
        );
    }

    #[tokio::test]
    async fn test_scheduler_lifecycle() {
        let scheduler = ConfigUpgradeScheduler::new();

        assert!(!scheduler.is_running().await);

        let result = scheduler.start().await;
        assert!(result.is_ok());

        scheduler.stop().await;
        assert!(!scheduler.is_running().await);
    }
}
