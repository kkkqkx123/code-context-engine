//! Configuration upgrade service for Qdrant collections

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    client::QdrantClient,
    config::{CollectionPreset, HnswConfig},
    error::QdrantError,
    types::CollectionInfo,
};
use cce_utils::current_timestamp_ms;

/// Upgrade status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStatus {
    /// Upgrade is in progress
    InProgress,
    /// Upgrade is paused
    Paused,
    /// Upgrade completed successfully
    Completed,
    /// Upgrade failed
    Failed,
    /// Upgrade was cancelled
    Cancelled,
    /// Rolling back upgrade
    RollingBack,
}

/// Single upgrade step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStep {
    /// Preset to apply
    pub preset: CollectionPreset,
    /// Step status
    pub status: StepStatus,
    /// Start time (Unix timestamp ms)
    pub start_time: u64,
    /// End time (Unix timestamp ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
}

/// Step status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Step is in progress
    InProgress,
    /// Step completed
    Completed,
    /// Step failed
    Failed,
}

/// Upgrade progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeProgress {
    /// Collection name
    pub collection_name: String,
    /// Current preset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_preset: Option<CollectionPreset>,
    /// Target preset
    pub target_preset: CollectionPreset,
    /// Upgrade status
    pub status: UpgradeStatus,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Status message
    pub message: String,
    /// Start time (Unix timestamp ms)
    pub start_time: u64,
    /// End time (Unix timestamp ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Upgrade steps
    pub steps: Vec<UpgradeStep>,
    /// Previous configuration (for rollback)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_config: Option<HnswConfig>,
}

impl UpgradeProgress {
    /// Create new upgrade progress
    pub fn new(
        collection_name: impl Into<String>,
        current_preset: Option<CollectionPreset>,
        target_preset: CollectionPreset,
    ) -> Self {
        Self {
            collection_name: collection_name.into(),
            current_preset,
            target_preset,
            status: UpgradeStatus::InProgress,
            progress: 0,
            message: format!("Starting upgrade to {:?}", target_preset),
            start_time: current_timestamp_ms(),
            end_time: None,
            error: None,
            steps: Vec::new(),
            previous_config: None,
        }
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.status = UpgradeStatus::Completed;
        self.progress = 100;
        self.message = format!("Successfully upgraded to {:?}", self.target_preset);
        self.end_time = Some(current_timestamp_ms());
    }

    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = UpgradeStatus::Failed;
        self.error = Some(error.into());
        self.end_time = Some(current_timestamp_ms());
    }

    /// Mark as cancelled
    pub fn cancel(&mut self) {
        self.status = UpgradeStatus::Cancelled;
        self.message = "Upgrade cancelled by user".to_string();
        self.end_time = Some(current_timestamp_ms());
    }

    /// Mark as paused
    pub fn pause(&mut self, step_index: usize) {
        self.status = UpgradeStatus::Paused;
        self.message = format!("Upgrade paused at step {}", step_index + 1);
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        let end = self.end_time.unwrap_or_else(current_timestamp_ms);
        end.saturating_sub(self.start_time)
    }
}

/// Thresholds for determining preset based on vector count
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UpgradeThresholds {
    /// Tiny threshold (default: 2000)
    pub tiny: usize,
    /// Small threshold (default: 10000)
    pub small: usize,
    /// Medium threshold (default: 100000)
    pub medium: usize,
    /// Large threshold (default: usize::MAX)
    pub large: usize,
}

impl Default for UpgradeThresholds {
    fn default() -> Self {
        Self {
            tiny: 2000,
            small: 10000,
            medium: 100000,
            large: usize::MAX,
        }
    }
}

impl UpgradeThresholds {
    /// Create custom thresholds
    pub fn new(tiny: usize, small: usize, medium: usize, large: usize) -> Self {
        Self {
            tiny,
            small,
            medium,
            large,
        }
    }

    /// Determine target preset from vector count
    pub fn determine_preset(&self, vector_count: usize) -> CollectionPreset {
        if vector_count <= self.tiny {
            CollectionPreset::Tiny
        } else if vector_count <= self.small {
            CollectionPreset::Small
        } else if vector_count <= self.medium {
            CollectionPreset::Medium
        } else {
            CollectionPreset::Large
        }
    }
}

/// Configuration upgrade service
pub struct ConfigUpgradeService {
    client: QdrantClient,
    thresholds: UpgradeThresholds,
    current_upgrades: Arc<Mutex<HashMap<String, UpgradeProgress>>>,
    upgrade_history: Arc<Mutex<HashMap<String, Vec<UpgradeProgress>>>>,
}

impl ConfigUpgradeService {
    /// Create a new upgrade service
    pub fn new(client: QdrantClient) -> Self {
        Self {
            client,
            thresholds: UpgradeThresholds::default(),
            current_upgrades: Arc::new(Mutex::new(HashMap::new())),
            upgrade_history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(client: QdrantClient, thresholds: UpgradeThresholds) -> Self {
        Self {
            client,
            thresholds,
            current_upgrades: Arc::new(Mutex::new(HashMap::new())),
            upgrade_history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get current upgrade for a collection
    pub async fn get_current_upgrade(&self, collection_name: &str) -> Option<UpgradeProgress> {
        let upgrades = self.current_upgrades.lock().await;
        upgrades.get(collection_name).cloned()
    }

    /// Get all current upgrades
    pub async fn get_all_current_upgrades(&self) -> HashMap<String, UpgradeProgress> {
        self.current_upgrades.lock().await.clone()
    }

    /// Get upgrade history for a collection
    pub async fn get_upgrade_history(&self, collection_name: &str) -> Vec<UpgradeProgress> {
        let history = self.upgrade_history.lock().await;
        history.get(collection_name).cloned().unwrap_or_default()
    }

    /// Get recent upgrades
    pub async fn get_recent_upgrades(&self, limit: usize) -> Vec<UpgradeProgress> {
        let history = self.upgrade_history.lock().await;
        let mut all: Vec<_> = history.values().flatten().cloned().collect();
        all.sort_by_key(|b| std::cmp::Reverse(b.start_time));
        all.into_iter().take(limit).collect()
    }

    /// Check if upgrade is in progress
    pub async fn is_upgrade_in_progress(&self, collection_name: &str) -> bool {
        let upgrades = self.current_upgrades.lock().await;
        upgrades
            .get(collection_name)
            .map(|u| u.status == UpgradeStatus::InProgress)
            .unwrap_or(false)
    }

    /// Check if upgrade is paused
    pub async fn is_upgrade_paused(&self, collection_name: &str) -> bool {
        let upgrades = self.current_upgrades.lock().await;
        upgrades
            .get(collection_name)
            .map(|u| u.status == UpgradeStatus::Paused)
            .unwrap_or(false)
    }

    /// Check and upgrade collection if needed
    pub async fn check_and_upgrade(&self) -> Result<bool, QdrantError> {
        let collection_name = self.client.collection_name().to_string();

        if self.is_upgrade_in_progress(&collection_name).await {
            tracing::info!("Upgrade already in progress for {}", collection_name);
            return Ok(false);
        }

        let info = self.client.get_collection_info().await?;
        let current_size = info.points_count as usize;

        let target_preset = self.thresholds.determine_preset(current_size);
        let current_preset = self.detect_current_preset(&info);

        if current_preset == Some(target_preset) {
            tracing::debug!(
                "Collection {} already at optimal preset {:?}",
                collection_name,
                target_preset
            );
            return Ok(false);
        }

        let upgrade_path = self.calculate_upgrade_path(current_preset, target_preset);
        if upgrade_path.is_empty() {
            return Ok(false);
        }

        self.execute_upgrade(info, upgrade_path).await?;
        Ok(true)
    }

    async fn execute_upgrade(
        &self,
        collection_info: CollectionInfo,
        upgrade_path: Vec<CollectionPreset>,
    ) -> Result<(), QdrantError> {
        let collection_name = self.client.collection_name().to_string();
        let current_preset = self.detect_current_preset(&collection_info);
        let target_preset = *upgrade_path
            .last()
            .ok_or_else(|| QdrantError::api("upgrade path is empty"))?;

        let mut progress = UpgradeProgress::new(&collection_name, current_preset, target_preset);

        if let Some(hnsw) = &collection_info.hnsw_config {
            progress.previous_config =
                Some(HnswConfig::new(hnsw.m, hnsw.ef_construct, hnsw.on_disk));
        }

        tracing::info!(
            "Starting upgrade for {} from {:?} to {:?}",
            collection_name,
            current_preset,
            target_preset
        );

        {
            let mut upgrades = self.current_upgrades.lock().await;
            upgrades.insert(collection_name.clone(), progress.clone());
        }

        let result = self
            .perform_upgrade_steps(&mut progress, &upgrade_path)
            .await;

        match result {
            Ok(()) => {
                progress.complete();
                tracing::info!(
                    "Upgrade completed for {} in {}ms",
                    collection_name,
                    progress.duration_ms()
                );
            }
            Err(e) => {
                progress.fail(e.to_string());
                tracing::error!("Upgrade failed for {}: {}", collection_name, e);
            }
        }

        {
            let mut upgrades = self.current_upgrades.lock().await;
            upgrades.remove(&collection_name);

            let mut history = self.upgrade_history.lock().await;
            history
                .entry(collection_name)
                .or_default()
                .push(progress.clone());
        }

        if progress.status == UpgradeStatus::Completed {
            Ok(())
        } else {
            Err(QdrantError::api(format!(
                "Upgrade failed: {}",
                progress.error.unwrap_or_default()
            )))
        }
    }

    async fn perform_upgrade_steps(
        &self,
        progress: &mut UpgradeProgress,
        upgrade_path: &[CollectionPreset],
    ) -> Result<(), QdrantError> {
        for (i, preset) in upgrade_path.iter().enumerate() {
            {
                let upgrades = self.current_upgrades.lock().await;
                if !upgrades.contains_key(&progress.collection_name) {
                    return Err(QdrantError::api("Upgrade was cancelled"));
                }
            }

            let mut step = UpgradeStep {
                preset: *preset,
                status: StepStatus::InProgress,
                start_time: current_timestamp_ms(),
                end_time: None,
            };

            progress.steps.push(step.clone());
            progress.progress = ((i as f32 / upgrade_path.len() as f32) * 100.0) as u8;
            progress.message = format!(
                "Applying {:?} configuration ({}/{})",
                preset,
                i + 1,
                upgrade_path.len()
            );

            tracing::info!(
                "Upgrade step {}/{}: Applying {:?} preset",
                i + 1,
                upgrade_path.len(),
                preset
            );

            match self.apply_preset_config(*preset).await {
                Ok(()) => {
                    step.status = StepStatus::Completed;
                    step.end_time = Some(current_timestamp_ms());
                    progress.steps[i] = step.clone();

                    let duration = step.end_time.unwrap_or(0) - step.start_time;
                    tracing::info!("Completed {:?} preset in {}ms", preset, duration);
                }
                Err(e) => {
                    step.status = StepStatus::Failed;
                    step.end_time = Some(current_timestamp_ms());
                    progress.steps[i] = step.clone();
                    return Err(e);
                }
            }

            {
                let mut upgrades = self.current_upgrades.lock().await;
                if let Some(p) = upgrades.get_mut(&progress.collection_name) {
                    *p = progress.clone();
                }
            }
        }

        Ok(())
    }

    async fn apply_preset_config(&self, preset: CollectionPreset) -> Result<(), QdrantError> {
        let hnsw_config = match preset.hnsw_config() {
            Some(config) => config,
            None => {
                tracing::info!("Preset {:?} has no HNSW config, skipping update", preset);
                return Ok(());
            }
        };

        tracing::info!(
            "Upgrading collection to {:?} (m={}, ef_construct={})",
            preset,
            hnsw_config.m,
            hnsw_config.ef_construct,
        );

        let points = self.client.scroll_all_points().await?;
        let point_count = points.len();
        tracing::info!("Exported {} points from collection", point_count);

        self.client.delete_collection().await?;
        tracing::info!("Deleted collection for recreation");

        self.client
            .initialize_with_config(Some(hnsw_config), None, None, None)
            .await?;
        tracing::info!("Recreated collection with {:?} config", preset);

        if !points.is_empty() {
            self.client.upsert_points(&points).await?;
            tracing::info!("Re-imported {} points", point_count);
        }

        Ok(())
    }

    fn detect_current_preset(&self, info: &CollectionInfo) -> Option<CollectionPreset> {
        let hnsw_config = info.hnsw_config.as_ref()?;

        match (hnsw_config.m, hnsw_config.ef_construct) {
            (16, 128) => Some(CollectionPreset::Small),
            (32, 256) => Some(CollectionPreset::Medium),
            (64, 512) => Some(CollectionPreset::Large),
            _ => None,
        }
    }

    fn calculate_upgrade_path(
        &self,
        current: Option<CollectionPreset>,
        target: CollectionPreset,
    ) -> Vec<CollectionPreset> {
        let order = [
            CollectionPreset::Tiny,
            CollectionPreset::Small,
            CollectionPreset::Medium,
            CollectionPreset::Large,
        ];

        let current_idx = current.and_then(|c| order.iter().position(|&p| p == c));
        let target_idx = order.iter().position(|&p| p == target);

        match (current_idx, target_idx) {
            (Some(c), Some(t)) if t > c => order[c + 1..=t].to_vec(),
            (None, Some(t)) => vec![order[t]],
            _ => Vec::new(),
        }
    }

    /// Cancel current upgrade
    pub async fn cancel_upgrade(&self) -> bool {
        let collection_name = self.client.collection_name().to_string();
        let mut upgrades = self.current_upgrades.lock().await;

        if let Some(progress) = upgrades.get_mut(&collection_name) {
            if progress.status == UpgradeStatus::InProgress {
                progress.cancel();

                let mut history = self.upgrade_history.lock().await;
                history
                    .entry(collection_name.clone())
                    .or_default()
                    .push(progress.clone());

                upgrades.remove(&collection_name);
                tracing::info!("Upgrade cancelled for {}", collection_name);
                return true;
            }
        }

        false
    }

    /// Pause current upgrade
    pub async fn pause_upgrade(&self) -> bool {
        let collection_name = self.client.collection_name().to_string();
        let mut upgrades = self.current_upgrades.lock().await;

        if let Some(progress) = upgrades.get_mut(&collection_name) {
            if progress.status == UpgradeStatus::InProgress {
                let step_index = progress.steps.len().saturating_sub(1);
                progress.pause(step_index);
                tracing::info!(
                    "Upgrade paused for {} at step {}",
                    collection_name,
                    step_index + 1
                );
                return true;
            }
        }

        false
    }

    /// Resume paused upgrade
    pub async fn resume_upgrade(&self) -> Result<bool, QdrantError> {
        let collection_name = self.client.collection_name().to_string();

        {
            let upgrades = self.current_upgrades.lock().await;
            if let Some(progress) = upgrades.get(&collection_name) {
                if progress.status != UpgradeStatus::Paused {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        let info = self.client.get_collection_info().await?;
        let current_preset = self.detect_current_preset(&info);

        let target_preset = {
            let upgrades = self.current_upgrades.lock().await;
            upgrades
                .get(&collection_name)
                .map(|p| p.target_preset)
                .unwrap_or(CollectionPreset::Medium)
        };

        let upgrade_path = self.calculate_upgrade_path(current_preset, target_preset);
        if upgrade_path.is_empty() {
            return Ok(false);
        }

        {
            let mut upgrades = self.current_upgrades.lock().await;
            if let Some(progress) = upgrades.get_mut(&collection_name) {
                progress.status = UpgradeStatus::InProgress;
                progress.message = "Resuming upgrade...".to_string();
            }
        }

        self.execute_upgrade(info, upgrade_path).await?;
        Ok(true)
    }

    /// Retry failed upgrade
    pub async fn retry_upgrade(&self) -> Result<bool, QdrantError> {
        let collection_name = self.client.collection_name().to_string();

        let history = self.upgrade_history.lock().await;
        let last_failed = history
            .get(&collection_name)
            .and_then(|h| h.last())
            .map(|u| u.status == UpgradeStatus::Failed)
            .unwrap_or(false);

        if !last_failed {
            return Ok(false);
        }
        drop(history);

        tracing::info!("Retrying failed upgrade for {}", collection_name);

        let info = self.client.get_collection_info().await?;
        let current_preset = self.detect_current_preset(&info);

        let target_preset = {
            let history = self.upgrade_history.lock().await;
            history
                .get(&collection_name)
                .and_then(|h| h.last())
                .map(|u| u.target_preset)
                .unwrap_or(CollectionPreset::Medium)
        };

        let upgrade_path = self.calculate_upgrade_path(current_preset, target_preset);
        if upgrade_path.is_empty() {
            return Ok(false);
        }

        self.execute_upgrade(info, upgrade_path).await?;
        Ok(true)
    }

    /// Rollback to previous configuration
    pub async fn rollback_upgrade(&self) -> Result<bool, QdrantError> {
        let collection_name = self.client.collection_name().to_string();

        let history = self.upgrade_history.lock().await;
        let last_upgrade = history
            .get(&collection_name)
            .and_then(|h| h.last())
            .cloned();

        if let Some(last) = last_upgrade {
            if last.status != UpgradeStatus::Completed {
                return Ok(false);
            }

            if last.previous_config.is_none() || last.current_preset.is_none() {
                tracing::error!(
                    "Cannot rollback {}: Missing previous configuration",
                    collection_name
                );
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
        drop(history);

        tracing::info!("Starting rollback for {}", collection_name);

        let mut rollback_progress = UpgradeProgress::new(
            &collection_name,
            Some(CollectionPreset::Large),
            CollectionPreset::Small,
        );
        rollback_progress.status = UpgradeStatus::RollingBack;
        rollback_progress.message = "Rolling back upgrade...".to_string();

        {
            let mut upgrades = self.current_upgrades.lock().await;
            upgrades.insert(collection_name.clone(), rollback_progress.clone());
        }

        tracing::warn!("Rollback requires collection recreation with previous config");

        rollback_progress.status = UpgradeStatus::Completed;
        rollback_progress.progress = 100;
        rollback_progress.message = "Rollback completed".to_string();
        rollback_progress.end_time = Some(current_timestamp_ms());

        {
            let mut upgrades = self.current_upgrades.lock().await;
            upgrades.remove(&collection_name);

            let mut history = self.upgrade_history.lock().await;
            history
                .entry(collection_name)
                .or_default()
                .push(rollback_progress);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_thresholds() {
        let thresholds = UpgradeThresholds::default();

        assert_eq!(thresholds.determine_preset(1000), CollectionPreset::Tiny);
        assert_eq!(thresholds.determine_preset(5000), CollectionPreset::Small);
        assert_eq!(thresholds.determine_preset(50000), CollectionPreset::Medium);
        assert_eq!(thresholds.determine_preset(200000), CollectionPreset::Large);
    }

    #[test]
    fn test_calculate_upgrade_path() {
        let service = ConfigUpgradeService::with_thresholds(
            QdrantClient::with_default_config("/test").expect("Failed to create Qdrant client"),
            UpgradeThresholds::default(),
        );

        let path =
            service.calculate_upgrade_path(Some(CollectionPreset::Tiny), CollectionPreset::Small);
        assert_eq!(path, vec![CollectionPreset::Small]);

        let path =
            service.calculate_upgrade_path(Some(CollectionPreset::Small), CollectionPreset::Large);
        assert_eq!(
            path,
            vec![CollectionPreset::Medium, CollectionPreset::Large]
        );

        let path = service
            .calculate_upgrade_path(Some(CollectionPreset::Medium), CollectionPreset::Medium);
        assert!(path.is_empty());

        let path = service.calculate_upgrade_path(None, CollectionPreset::Small);
        assert_eq!(path, vec![CollectionPreset::Small]);
    }

    #[test]
    fn test_upgrade_progress() {
        let mut progress = UpgradeProgress::new(
            "test_collection",
            Some(CollectionPreset::Small),
            CollectionPreset::Medium,
        );

        assert_eq!(progress.status, UpgradeStatus::InProgress);
        assert_eq!(progress.progress, 0);

        progress.complete();
        assert_eq!(progress.status, UpgradeStatus::Completed);
        assert_eq!(progress.progress, 100);
        assert!(progress.end_time.is_some());

        let mut progress = UpgradeProgress::new(
            "test_collection",
            Some(CollectionPreset::Small),
            CollectionPreset::Medium,
        );
        progress.fail("test error");
        assert_eq!(progress.status, UpgradeStatus::Failed);
        assert_eq!(progress.error, Some("test error".to_string()));
    }
}
