//! Project registry runtime implementation
//!
//! This module owns the cache and metrics for project-level effective
//! configuration. The effective configuration is built from the global
//! configuration plus the project profile document in the metadata database.
//! The server never reads project configuration files.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{Mutex, RwLock};

use cce_config::project_registry::{ProjectEntry, ProjectMetadata, RegistryError};
use cce_config::validation::Validate;
use cce_config::{AppConfig, ProjectAppConfig, Settings};
use cce_metrics::{LabeledCounter, LabeledGauge, LabeledHistogram, MetricsRegistry};
use chrono::Utc;

use crate::{ProjectRepository, ProjectUpdateRecord, SqliteClient};

#[derive(Debug)]
struct ProjectRegistryMetrics {
    cache_hits_total: LabeledCounter,
    cache_misses_total: LabeledCounter,
    cache_invalidations_total: LabeledCounter,
    cache_size: LabeledGauge,
    load_latency_ms: LabeledHistogram,
}

impl ProjectRegistryMetrics {
    fn new(registry: &MetricsRegistry) -> Self {
        Self {
            cache_hits_total: registry.counter_simple("project_registry_cache_hits_total"),
            cache_misses_total: registry.counter_simple("project_registry_cache_misses_total"),
            cache_invalidations_total: registry
                .counter_simple("project_registry_cache_invalidations_total"),
            cache_size: registry.gauge_simple("project_registry_cache_size"),
            load_latency_ms: registry.histogram_default_simple("project_registry_load_latency_ms"),
        }
    }

    fn record_cache_hit(&self, cache_size: usize) {
        self.cache_hits_total.increment();
        self.cache_size.set(cache_size as u64);
    }

    fn record_cache_miss(&self) {
        self.cache_misses_total.increment();
    }

    fn record_cache_invalidation(&self, cache_size: usize) {
        self.cache_invalidations_total.increment();
        self.cache_size.set(cache_size as u64);
    }

    fn record_cache_size(&self, cache_size: usize) {
        self.cache_size.set(cache_size as u64);
    }

    fn record_load_latency(&self, latency_ms: f64) {
        self.load_latency_ms.observe(latency_ms);
    }
}

/// Project registry for loading and caching project configurations
///
/// Provides async access to project entries by project ID.
#[derive(Debug, Clone)]
pub struct ProjectRegistry {
    inner: Arc<RwLock<HashMap<i64, ProjectEntry>>>,
    /// Per-key load mutexes. Concurrent cache misses for the same project
    /// serialize here so the database is read once; losers reuse the
    /// winner's entry from the cache.
    key_locks: Arc<Mutex<HashMap<i64, Arc<Mutex<()>>>>>,
    metrics: Arc<ProjectRegistryMetrics>,
    sqlite: SqliteClient,
}

impl ProjectRegistry {
    /// Create a new empty project registry
    pub fn new(metrics_registry: Arc<MetricsRegistry>, sqlite: SqliteClient) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            key_locks: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ProjectRegistryMetrics::new(&metrics_registry)),
            sqlite,
        }
    }

    /// Get or load a project entry by ID
    pub async fn get_or_load(&self, project_id: i64) -> Result<ProjectEntry, RegistryError> {
        let start = Instant::now();

        {
            let global_version = Settings::global_version();
            let inner = self.inner.read().await;
            if let Some(entry) = inner.get(&project_id)
                && entry.global_version == global_version
            {
                self.metrics.record_cache_hit(inner.len());
                return Ok(entry.clone());
            }
        }

        self.metrics.record_cache_miss();

        // Singleflight: serialize loads for the same key, then re-check the
        // cache so only the winner reads the database.
        let key_lock = {
            let mut locks = self.key_locks.lock().await;
            locks
                .entry(project_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _permit = key_lock.lock().await;

        {
            let global_version = Settings::global_version();
            let inner = self.inner.read().await;
            if let Some(entry) = inner.get(&project_id)
                && entry.global_version == global_version
            {
                self.metrics.record_cache_hit(inner.len());
                self.release_key_lock(project_id, &key_lock).await;
                return Ok(entry.clone());
            }
        }

        let global_config = Settings::global().map_err(|e| {
            RegistryError::Configuration(format!("Failed to get global config: {}", e))
        })?;
        let global_version = Settings::global_version();

        // Load project record (scoped to drop MutexGuard before subsequent async work)
        let record = {
            let conn = self
                .sqlite
                .read_connection()
                .map_err(|error| RegistryError::Database(error.to_string()))?;
            ProjectRepository::get_by_id(&conn, project_id)
                .map_err(|error| RegistryError::Database(error.to_string()))?
                .ok_or(RegistryError::ProjectNotFound(project_id))?
        };

        let config = effective_config(&global_config, &record)?;
        let profile = parse_profile(&record)?;
        let profile_version = record.profile_version_u64();
        // The entry version combines the profile and global versions so a
        // change on either side advances it.
        let version = Settings::combine_versions(profile_version, global_version);
        // Scan lists are derived from the stored profile document only. When
        // the profile leaves a section absent, the merged effective value is
        // used so display metadata stays consistent with indexing behavior.
        let extensions = profile
            .orchestrator
            .as_ref()
            .and_then(|orchestrator| orchestrator.indexer.as_ref())
            .and_then(|indexer| indexer.extensions.clone())
            .unwrap_or_else(|| config.orchestrator.indexer.extensions.clone());
        let exclude_dirs = profile
            .orchestrator
            .as_ref()
            .and_then(|orchestrator| orchestrator.indexer.as_ref())
            .and_then(|indexer| indexer.exclude_dirs.clone())
            .unwrap_or_else(|| config.orchestrator.indexer.exclude_dirs.clone());
        let respect_gitignore = profile
            .scanner
            .as_ref()
            .and_then(|scanner| scanner.respect_gitignore)
            .unwrap_or(config.scanner.respect_gitignore);
        let ignore_patterns = profile
            .scanner
            .as_ref()
            .and_then(|scanner| scanner.gitignore_patterns.clone())
            .unwrap_or_else(|| config.scanner.gitignore_patterns.clone());
        let metadata = ProjectMetadata {
            id: record.id,
            name: record.name,
            root_path: record.root_path,
            config_file_path: record.config_file_path,
            language: record.language,
            extensions,
            exclude_dirs,
            respect_gitignore,
            ignore_patterns,
            last_indexed: record.last_indexed,
            created_at: timestamp_to_rfc3339(record.created_at),
            updated_at: timestamp_to_rfc3339(record.updated_at),
        };

        let entry = ProjectEntry {
            metadata,
            config,
            loaded_at: Instant::now(),
            version,
            global_version,
        };

        {
            let mut inner = self.inner.write().await;
            // Guard against stale overwrite: a concurrent update may have
            // cached a newer version while this load was reading the database.
            if let Some(cached) = inner.get(&project_id) {
                if cached.version >= entry.version {
                    self.metrics.record_cache_size(inner.len());
                    self.release_key_lock(project_id, &key_lock).await;
                    return Ok(cached.clone());
                }
            }
            inner.insert(project_id, entry.clone());
            self.metrics.record_cache_size(inner.len());
        }

        self.release_key_lock(project_id, &key_lock).await;
        self.metrics
            .record_load_latency(start.elapsed().as_secs_f64() * 1000.0);

        Ok(entry)
    }

    /// Drop the per-key load mutex once no other task is waiting on it.
    async fn release_key_lock(&self, project_id: i64, key_lock: &Arc<Mutex<()>>) {
        if Arc::strong_count(key_lock) <= 2 {
            self.key_locks.lock().await.remove(&project_id);
        }
    }

    /// Find a project by its path (local supply only).
    ///
    /// Remote gateway projects use logical-only roots the server cannot
    /// canonicalize, so callers must address them by id. The lookup path is
    /// normalized the same way as at creation time so symlinks, relative
    /// paths and trailing slashes resolve to the stored canonical root.
    /// Falls back to the raw input when it cannot be resolved.
    pub async fn find_by_path(&self, path: &Path) -> Result<ProjectEntry, RegistryError> {
        let normalized = normalize_lookup_path(path);
        // Check cache first (fast path)
        {
            let inner = self.inner.read().await;
            for entry in inner.values() {
                if entry.metadata.root_path == normalized {
                    return Ok(entry.clone());
                }
            }
        }

        // Cache miss — query SQLite (scoped to drop MutexGuard before await)
        let project_id: i64 = {
            let conn = self
                .sqlite
                .read_connection()
                .map_err(|error| RegistryError::Database(error.to_string()))?;
            let record = ProjectRepository::find_by_root_path(&conn, &normalized)
                .map_err(|error| RegistryError::Database(error.to_string()))?
                .ok_or_else(|| RegistryError::PathNotFound(path.display().to_string()))?;
            record.id
        };

        // Load into cache via get_or_load (uses project ID from record)
        self.get_or_load(project_id).await
    }

    /// Read the stored project profile and its version.
    ///
    /// The returned version combines the profile and global versions, matching
    /// the version reported by `update_profile` and cached entries.
    pub async fn get_profile(
        &self,
        project_id: i64,
    ) -> Result<(ProjectAppConfig, u64), RegistryError> {
        let record = {
            let conn = self
                .sqlite
                .read_connection()
                .map_err(|error| RegistryError::Database(error.to_string()))?;
            ProjectRepository::get_by_id(&conn, project_id)
                .map_err(|error| RegistryError::Database(error.to_string()))?
                .ok_or(RegistryError::ProjectNotFound(project_id))?
        };
        let profile = parse_profile(&record)?;
        let version =
            Settings::combine_versions(record.profile_version_u64(), Settings::global_version());
        Ok((profile, version))
    }

    /// Persist a replacement project profile and refresh the cache.
    ///
    /// The write bumps `profile_version` atomically in the same statement so
    /// the version observed on the next load is the version of this write.
    /// The profile document is the single source of truth.
    pub async fn update_profile(
        &self,
        project_id: i64,
        profile: ProjectAppConfig,
    ) -> Result<u64, RegistryError> {
        profile
            .validate_structured()
            .map_err(|error| RegistryError::Validation(error.to_string()))?;
        let canonical = profile.to_canonical_json();
        let updates = ProjectUpdateRecord::default().with_profile_json(canonical);
        self.sqlite
            .with_transaction(|tx| {
                let existing = ProjectRepository::get_by_id(tx, project_id)
                    .map_err(|error| {
                        cce_types::StorageError::query(format!(
                            "Failed to load project {project_id}: {error}"
                        ))
                    })?
                    .ok_or_else(|| {
                        cce_types::StorageError::not_found(format!(
                            "Project {project_id} not found"
                        ))
                    })?;
                let _ = existing;
                ProjectRepository::update(tx, project_id, &updates)
            })
            .map_err(|error| RegistryError::Database(error.to_string()))?;
        self.invalidate_cache(Some(project_id)).await?;
        // Repopulate immediately so callers observe the fresh effective
        // configuration without waiting for the next cache miss.
        let entry = self.get_or_load(project_id).await?;
        Ok(entry.version)
    }

    /// Invalidate cache for one project or the whole registry
    pub async fn invalidate_cache(&self, project_id: Option<i64>) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;

        match project_id {
            Some(id) => {
                inner.remove(&id);
            }
            None => {
                inner.clear();
            }
        }

        self.metrics.record_cache_invalidation(inner.len());
        Ok(())
    }

    /// Insert or update a project entry
    pub async fn insert(&self, id: i64, entry: ProjectEntry) -> Result<(), RegistryError> {
        let mut inner = self.inner.write().await;
        inner.insert(id, entry);
        self.metrics.record_cache_size(inner.len());
        Ok(())
    }
}

pub fn parse_profile(record: &crate::ProjectRecord) -> Result<ProjectAppConfig, RegistryError> {
    ProjectAppConfig::from_json_str(&record.profile_json).map_err(RegistryError::Configuration)
}

pub fn effective_config(
    global: &AppConfig,
    record: &crate::ProjectRecord,
) -> Result<AppConfig, RegistryError> {
    let profile = parse_profile(record)?;
    let mut merged = global.merge_with_project(&profile);
    merged.resolve_dependencies();
    Ok(merged)
}

fn normalize_lookup_path(path: &Path) -> String {
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().to_string();
    }
    path.to_string_lossy().to_string()
}

fn timestamp_to_rfc3339(timestamp: i64) -> String {
    match chrono::DateTime::<Utc>::from_timestamp(timestamp, 0) {
        Some(valid) => valid.to_rfc3339(),
        None => {
            tracing::warn!(
                timestamp,
                "Invalid project timestamp; falling back to epoch"
            );
            chrono::DateTime::<Utc>::from_timestamp(0, 0)
                .expect("Unix epoch must be a valid timestamp")
                .to_rfc3339()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewProjectRecord, ProjectRepository};

    #[tokio::test]
    async fn loads_distinct_projects_from_sqlite() {
        let _ = Settings::init(AppConfig::default());
        let sqlite = SqliteClient::in_memory().expect("Failed to create SQLite database");
        let first_root = tempfile::tempdir().expect("Failed to create first project");
        let second_root = tempfile::tempdir().expect("Failed to create second project");
        let (first_id, second_id) = sqlite
            .with_transaction(|tx| {
                let first = ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new(
                        "registry-project-one".to_string(),
                        first_root.path().to_string_lossy().to_string(),
                    ),
                )?;
                let second = ProjectRepository::insert(
                    tx,
                    &NewProjectRecord::new(
                        "registry-project-two".to_string(),
                        second_root.path().to_string_lossy().to_string(),
                    ),
                )?;
                Ok((first, second))
            })
            .expect("Failed to insert projects");

        let registry = ProjectRegistry::new(Arc::new(MetricsRegistry::new()), sqlite);
        let first = registry
            .get_or_load(first_id)
            .await
            .expect("Failed to load first project");
        let second = registry
            .get_or_load(second_id)
            .await
            .expect("Failed to load second project");
        assert_eq!(first.metadata.id, first_id);
        assert_eq!(second.metadata.id, second_id);
        assert_eq!(
            first.metadata.root_path,
            first_root.path().to_string_lossy()
        );
        assert_eq!(
            second.metadata.root_path,
            second_root.path().to_string_lossy()
        );
        assert_ne!(first.metadata.root_path, second.metadata.root_path);
    }
}
