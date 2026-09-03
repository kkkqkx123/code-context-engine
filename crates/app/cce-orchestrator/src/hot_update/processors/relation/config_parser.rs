//! Relation index update processor
//!
//! This module handles updates to the relation index during hot updates.
//! It also persists relation data to SQLite for fast cold start recovery.
//!
//! # Phase 3: Dependency Propagation
//!
//! This processor implements dependency propagation for hot updates:
//! 1. When a file changes, find all files that depend on it
//! 2. Collect all affected files (changed + dependents)
//! 3. Process files in topological order (dependencies first)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::hot_update::error::{HotUpdateError, Result};

use cce_config::{RelationBuilderParams, RelationConfig};
use cce_relation::BuildConfigParser;
use cce_relation::index::RelationIndexView;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;

use super::external_packages::{BuildConfigParserExt, ExternalPackageData};
use super::relation_processor::RelationUpdateProcessor;

use cce_types::{LanguageInfo, StorageError};

impl RelationUpdateProcessor {
    pub fn set_relation_config(&mut self, config: &RelationConfig) {
        self.max_relations_per_file = config.index.max_relations_per_file;
        self.analyze_imports = config.analyze_imports;
        self.track_cross_file_deps = config.track_cross_file_deps;
        self.filter_stdlib_calls = config.index.filter_stdlib_calls;
        self.symbol_extract_enabled = config.plugin_symbol_extract_enabled;
        self.plugin_symbols_enabled = config.plugin_symbols_enabled;
        self.dependency_propagation_enabled = config.track_cross_file_deps;
        // Keep the propagation depth in sync with the config so the
        // effective scope is reflected in the exposed capabilities.
        self.max_propagation_depth = config.max_propagation_depth;
        self.max_fingerprint_scope_ratio = config.max_fingerprint_scope_ratio;
        self.manifest_scan_depth = config.manifest_scan_depth;

        // Fields consumed only at query time, not during graph construction.
        // Logged here so the build path is transparent about ignoring them.
        if config.max_call_depth != 10 || config.index.resolve_call_chains {
            tracing::debug!(
                max_call_depth = config.max_call_depth,
                resolve_call_chains = config.index.resolve_call_chains,
                "Relation query-only config fields have no effect during graph construction"
            );
        }
    }

    /// Apply construction policy from shared parameters.
    ///
    /// This method uses `RelationBuilderParams` as the single source of
    /// truth, ensuring consistency with the full-index construction path.
    pub fn set_relation_params(&mut self, params: &RelationBuilderParams) {
        self.filter_stdlib_calls = params.filter_stdlib_calls;
        self.max_relations_per_file = params.max_relations_per_file;
        self.analyze_imports = params.analyze_imports;
        self.track_cross_file_deps = params.track_cross_file_deps;
        self.symbol_extract_enabled = params.symbol_extract_enabled;
        self.plugin_symbols_enabled = params.plugin_symbols_enabled;
        self.dependency_propagation_enabled = params.track_cross_file_deps;
        self.max_propagation_depth = params.max_propagation_depth;
        self.max_fingerprint_scope_ratio = params.max_fingerprint_scope_ratio;
        self.manifest_scan_depth = params.manifest_scan_depth;
    }

    /// Set whether this processor is enabled
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn persistence_project_id(&self) -> Result<i64> {
        if self.project_id > 0 {
            Ok(self.project_id)
        } else {
            Err(HotUpdateError::relation(
                "project_id must be configured when relation persistence is enabled",
            ))
        }
    }

    /// Check if a config file is relevant for this processor
    pub(crate) fn is_relevant_config(config_path: &Path) -> bool {
        config_path
            .file_name()
            .and_then(|filename| filename.to_str())
            .is_some_and(BuildConfigParser::is_build_config)
    }

    /// Reload build configurations from project root
    ///
    /// This method re-scans all build configuration files and updates
    /// the external package list used for import classification.
    /// It clears old package information before loading new ones.
    pub async fn reload_build_config(&self, project_root: &Path) -> Result<()> {
        // Validate the new build configuration now. The candidate builder
        // reloads it directly when the epoch is constructed, so retaining a
        // second mutable copy here would create divergent resolver state.
        self.prepare_config_data(project_root).await?;

        // Store the project root for future reloads
        let mut root = self.project_root.lock().await;
        *root = Some(project_root.to_path_buf());

        tracing::info!(
            "Successfully reloaded build configurations from {}",
            project_root.display()
        );

        Ok(())
    }

    /// Identify files affected by configuration changes
    ///
    /// When build configuration changes (e.g., Cargo.toml), we need to identify
    /// which files might be affected. This method queries BuildConfigParser's
    /// metadata to determine the appropriate file extensions.
    ///
    /// # Design
    ///
    /// Instead of hard-coding config file to extension mappings, we query
    /// BuildConfigParser which serves as the single source of truth for
    /// build system metadata.
    pub(crate) async fn identify_affected_files_by_config(
        &self,
        config_path: &Path,
    ) -> Result<Vec<PathBuf>> {
        let config_filename = match config_path.file_name().and_then(|f| f.to_str()) {
            Some(name) => name,
            None => {
                tracing::warn!("Invalid config path: {}", config_path.display());
                return Ok(Vec::new());
            }
        };

        // Query BuildConfigParser for affected extensions (no more hard-coding!)
        let affected_extensions = BuildConfigParser::get_affected_extensions(config_filename);

        if affected_extensions.is_empty() {
            tracing::debug!(
                "Config file {} not recognized by BuildConfigParser, skipping",
                config_filename
            );
            return Ok(Vec::new());
        }

        let sqlite = match self.sqlite_client.as_ref() {
            Some(sqlite) => sqlite,
            None => {
                tracing::warn!(
                    config = config_filename,
                    "Cannot identify config-affected relation files without persistence"
                );
                return Ok(Vec::new());
            }
        };
        let project_id = self.persistence_project_id()?;
        let active_epoch = sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|error| HotUpdateError::relation(error.to_string()))?;
        if active_epoch <= 0 {
            tracing::warn!(
                project_id,
                config = config_filename,
                "Cannot identify config-affected relation files without an active epoch"
            );
            return Ok(Vec::new());
        }

        let project_root = self.project_root(sqlite)?;
        // Reuse the process-internal materialized base + delta chain instead
        // of reloading the full graph from SQLite; a config reload typically
        // happens right after a hot update that already populated the cache.
        let view = match self.base_cache.get_or_load(
            &SqliteSnapshotStore::new(sqlite.as_ref().clone()),
            project_id,
            active_epoch,
        ) {
            Ok(view) => view,
            Err(error) => {
                return Err(HotUpdateError::relation(error.to_string()));
            }
        };
        let mut affected_files: Vec<PathBuf> = Vec::new();
        view.for_each_file(|path, _| {
            let extension = Path::new(path).extension().and_then(|e| e.to_str());
            if let Some(extension) = extension
                && affected_extensions
                    .iter()
                    .any(|affected| affected == extension)
            {
                affected_files.push(project_root.join(path));
            }
        });

        tracing::info!(
            count = affected_files.len(),
            config = config_filename,
            extensions = ?affected_extensions,
            "Identified files potentially affected by config change"
        );

        Ok(affected_files)
    }

    /// Fine-grained variant: narrow affected files by package diff.
    ///
    /// After a config file change, compute which packages were added/removed
    /// and intersect with the import index to find files that actually import
    /// a changed package, instead of invalidating the whole extension closure.
    /// Falls back to the extension-based set when the diff is empty (e.g. only
    /// versions changed) or when no import index is available.
    pub(crate) async fn identify_affected_files_by_config_fine_grained(
        &self,
        config_path: &Path,
        old_parser: &BuildConfigParser,
        new_parser: &BuildConfigParser,
    ) -> Result<Vec<PathBuf>> {
        // Start from extension closure as fallback scope
        let extension_files = self.identify_affected_files_by_config(config_path).await?;
        if extension_files.is_empty() {
            return Ok(Vec::new());
        }
        self.record_config_affected_files(extension_files.len());
        // Compute package diff
        let diff = old_parser.package_diff(new_parser);
        if diff.is_empty() {
            tracing::info!(
                "Package set unchanged (version/comment only); skipping source rebuild, synthetic node will be refreshed"
            );
            self.record_config_fine_grained_fallback();
            return Ok(Vec::new());
        }
        // Collect changed package names across all affected languages
        let mut changed_packages: HashSet<String> = HashSet::new();
        for (added, removed) in diff.values() {
            changed_packages.extend(added.iter().cloned());
            changed_packages.extend(removed.iter().cloned());
        }
        if changed_packages.is_empty() {
            self.record_config_fine_grained_fallback();
            return Ok(extension_files);
        }
        let sqlite = match self.sqlite_client.as_ref() {
            Some(s) => s,
            None => {
                self.record_config_fine_grained_fallback();
                return Ok(extension_files);
            }
        };
        let project_id = self.persistence_project_id()?;
        let active_epoch = sqlite
            .project_meta_get_int(project_id, "active_relation_epoch")
            .map_err(|e| HotUpdateError::relation(e.to_string()))?;
        if active_epoch <= 0 {
            self.record_config_fine_grained_fallback();
            return Ok(extension_files);
        }
        let view = self
            .base_cache
            .get_or_load(
                &SqliteSnapshotStore::new(sqlite.as_ref().clone()),
                project_id,
                active_epoch,
            )
            .map_err(|e| HotUpdateError::relation(e.to_string()))?;
        let project_root = self.project_root(sqlite)?;
        // Build set of extension_files for quick lookup
        let ext_set: HashSet<String> = extension_files
            .iter()
            .map(|p| Self::candidate_path(&project_root, p))
            .collect();
        // Narrow: keep only files whose import table contains a changed package
        let mut narrowed: Vec<PathBuf> = Vec::new();
        let mut candidate_paths: HashSet<String> = HashSet::new();
        view.for_each_file(|path, _| {
            if ext_set.contains(path) {
                candidate_paths.insert(path.to_string());
            }
        });
        for path in candidate_paths {
            let Some(table) = view.imports_of(&path) else {
                continue;
            };
            let file_language = LanguageInfo::detect_from_path(&path).language;
            let mut affected = false;
            for imp in &table.standardized_imports {
                for pkg in &changed_packages {
                    if Self::imports_match_package(&imp.source, pkg, file_language) {
                        affected = true;
                        break;
                    }
                    if let Some(alias) = &imp.alias
                        && Self::imports_match_package(alias, pkg, file_language)
                    {
                        affected = true;
                        break;
                    }
                }
                if affected {
                    break;
                }
            }
            if affected {
                narrowed.push(project_root.join(&path));
            }
        }
        if narrowed.is_empty() {
            self.record_config_fine_grained_fallback();
            tracing::warn!(
                changed_packages = ?changed_packages,
                "Fine-grained narrowing found no importing files; falling back to extension-based set to avoid missing rebuilds"
            );
            return Ok(extension_files);
        }
        self.record_config_narrowed_files(narrowed.len());
        tracing::info!(
            before = extension_files.len(),
            after = narrowed.len(),
            changed_packages = ?changed_packages,
            "Narrowed config-affected files by package import intersection"
        );
        Ok(narrowed)
    }

    pub(crate) fn record_config_scan_failure(&self) {
        if let Some(metrics) = &self.relation_metrics {
            metrics.config_scan_failures_total.increment();
        }
    }

    pub(crate) fn record_config_fine_grained_fallback(&self) {
        if let Some(metrics) = &self.relation_metrics {
            metrics.config_fine_grained_fallback_total.increment();
        }
    }

    pub(crate) fn record_config_affected_files(&self, count: usize) {
        if let Some(metrics) = &self.relation_metrics {
            metrics.config_affected_files_total.add(count as u64);
        }
    }

    pub(crate) fn record_config_narrowed_files(&self, count: usize) {
        if let Some(metrics) = &self.relation_metrics {
            metrics.config_narrowed_files_total.add(count as u64);
        }
    }

    pub(crate) async fn scan_build_config_async(
        &self,
        project_root: PathBuf,
    ) -> std::result::Result<BuildConfigParser, String> {
        let depth = self.manifest_scan_depth;
        let mut parser = BuildConfigParser::new();
        parser
            .scan_project_async(project_root, depth)
            .await
            .map_err(|e| e.to_string())?;
        Ok(parser)
    }

    /// Prepare configuration data (no lock needed)
    pub(crate) async fn prepare_config_data(
        &self,
        project_root: &Path,
    ) -> Result<ExternalPackageData> {
        let mut config_parser = BuildConfigParser::new();
        let root = project_root.to_path_buf();
        let depth = self.manifest_scan_depth;
        if let Err(error) = config_parser.scan_project_async(root, depth).await {
            self.record_config_scan_failure();
            return Err(HotUpdateError::relation(format!(
                "Failed to scan project for config reload: {}",
                error
            )));
        }

        // Extract only the data we need
        Ok(config_parser.extract_external_packages())
    }

    pub(crate) fn project_root(&self, sqlite: &SqliteClient) -> Result<PathBuf> {
        if let Ok(root) = self.project_root.try_lock()
            && let Some(path) = root.as_ref()
        {
            return Ok(path.clone());
        }
        let project_id = self.persistence_project_id()?;
        sqlite
            .with_transaction(|tx| {
                tx.query_row(
                    "SELECT root_path FROM projects WHERE id = ?1",
                    rusqlite::params![project_id],
                    |row| row.get::<_, String>(0),
                )
                .map(PathBuf::from)
                .map_err(|error| {
                    StorageError::Query(format!("failed to load project root: {error}"))
                })
            })
            .map_err(|error| HotUpdateError::relation(error.to_string()))
    }
}
