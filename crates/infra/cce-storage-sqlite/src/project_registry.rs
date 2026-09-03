//! Project registry runtime implementation
//!
//! This module owns the cache and metrics for project-level configuration
//! loading. Shared data types live in `cce_config::project_registry`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use cce_config::project_registry::{ProjectEntry, ProjectMetadata, RegistryError};
use cce_config::{AppConfig, ConfigLoader, ProjectConfigPaths, Settings};
use cce_metrics::{LabeledCounter, LabeledGauge, LabeledHistogram, MetricsRegistry};
use chrono::Utc;

use crate::{ProjectRepository, SqliteClient};

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
    metrics: Arc<ProjectRegistryMetrics>,
    sqlite: SqliteClient,
}

impl ProjectRegistry {
    /// Create a new empty project registry
    pub fn new(metrics_registry: Arc<MetricsRegistry>, sqlite: SqliteClient) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(ProjectRegistryMetrics::new(&metrics_registry)),
            sqlite,
        }
    }

    /// Get or load a project entry by ID
    pub async fn get_or_load(&self, project_id: i64) -> Result<ProjectEntry, RegistryError> {
        let start = Instant::now();

        {
            let inner = self
                .inner
                .read()
                .map_err(|e| RegistryError::Database(e.to_string()))?;
            if let Some(entry) = inner.get(&project_id) {
                self.metrics.record_cache_hit(inner.len());
                return Ok(entry.clone());
            }
        }

        self.metrics.record_cache_miss();

        let global_config = Settings::global().map_err(|e| {
            RegistryError::Configuration(format!("Failed to get global config: {}", e))
        })?;

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

        let root_path = Path::new(&record.root_path);
        let mut config = load_project_config(&global_config, root_path, &record.config_file_path)?;
        let extensions = parse_string_list(record.extensions);
        let exclude_dirs = parse_string_list(record.exclude_dirs);
        let ignore_patterns = parse_string_list(record.ignore_patterns);
        if let Some(value) = &extensions {
            config.orchestrator.indexer.extensions = value.clone();
        }
        if let Some(value) = &exclude_dirs {
            config.orchestrator.indexer.exclude_dirs = value.clone();
        }
        let metadata = ProjectMetadata {
            id: record.id,
            name: record.name,
            root_path: record.root_path,
            config_file_path: record.config_file_path,
            language: record.language,
            extensions: extensions
                .unwrap_or_else(|| config.orchestrator.indexer.extensions.clone()),
            exclude_dirs: exclude_dirs
                .unwrap_or_else(|| config.orchestrator.indexer.exclude_dirs.clone()),
            respect_gitignore: record.respect_gitignore.unwrap_or(true),
            ignore_patterns: ignore_patterns.unwrap_or_default(),
            last_indexed: record.last_indexed,
            created_at: timestamp_to_rfc3339(record.created_at),
            updated_at: timestamp_to_rfc3339(record.updated_at),
        };

        let entry = ProjectEntry {
            metadata,
            config,
            loaded_at: Instant::now(),
            version: 0,
        };

        {
            let mut inner = self
                .inner
                .write()
                .map_err(|e| RegistryError::Database(e.to_string()))?;
            inner.insert(project_id, entry.clone());
            self.metrics.record_cache_size(inner.len());
        }

        self.metrics
            .record_load_latency(start.elapsed().as_secs_f64() * 1000.0);

        Ok(entry)
    }

    /// Find a project by its path
    ///
    /// First checks the in-memory cache, then falls back to SQLite query.
    /// SQLite result is loaded into cache for subsequent fast access.
    pub async fn find_by_path(&self, path: &Path) -> Result<ProjectEntry, RegistryError> {
        // Check cache first (fast path)
        {
            let inner = self
                .inner
                .read()
                .map_err(|e| RegistryError::Database(e.to_string()))?;
            for entry in inner.values() {
                if Path::new(&entry.metadata.root_path) == path {
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
            let record = ProjectRepository::find_by_root_path(&conn, &path.to_string_lossy())
                .map_err(|error| RegistryError::Database(error.to_string()))?
                .ok_or_else(|| RegistryError::PathNotFound(path.display().to_string()))?;
            record.id
        };

        // Load into cache via get_or_load (uses project ID from record)
        self.get_or_load(project_id).await
    }

    /// Update project configuration
    pub async fn update_config(
        &self,
        project_id: i64,
        config: AppConfig,
    ) -> Result<(), RegistryError> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        if let Some(entry) = inner.get_mut(&project_id) {
            entry.config = config;
            entry.version += 1;
            entry.metadata.updated_at = Utc::now().to_rfc3339();
            Ok(())
        } else {
            Err(RegistryError::ProjectNotFound(project_id))
        }
    }

    /// Invalidate cache for one project or the whole registry
    pub async fn invalidate_cache(&self, project_id: Option<i64>) -> Result<(), RegistryError> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| RegistryError::Database(e.to_string()))?;

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
    pub fn insert(&self, id: i64, entry: ProjectEntry) -> Result<(), RegistryError> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| RegistryError::Database(e.to_string()))?;
        inner.insert(id, entry);
        self.metrics.record_cache_size(inner.len());
        Ok(())
    }
}

fn load_project_config(
    global: &AppConfig,
    project_root: &Path,
    configured_path: &str,
) -> Result<AppConfig, RegistryError> {
    let configured = project_root.join(configured_path);
    let discovered = ProjectConfigPaths::find_project_config(project_root);
    let Some(path) = configured.exists().then_some(configured).or(discovered) else {
        return Ok(global.clone());
    };

    let project = ConfigLoader::new()
        .load_project_config(&path)
        .map_err(|error| RegistryError::Configuration(error.to_string()))?;
    let mut merged = global.merge_with_project(&project);
    if let Some(local_path) = ProjectConfigPaths::find_local_config(&path) {
        let local = ConfigLoader::new()
            .load_project_config(&local_path)
            .map_err(|error| RegistryError::Configuration(error.to_string()))?;
        merged = merged.merge_with_project(&local);
    }
    merged.resolve_dependencies();
    Ok(merged)
}

fn parse_string_list(value: Option<String>) -> Option<Vec<String>> {
    value.map(|value| {
        serde_json::from_str(&value).unwrap_or_else(|_| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
    })
}

fn timestamp_to_rfc3339(timestamp: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
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
