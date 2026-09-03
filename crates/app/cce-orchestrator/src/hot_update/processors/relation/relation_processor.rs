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

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::processors::trait_def::UpdateProcessor;
use crate::hot_update::{BatchChangeResult, FileChangeType};

use cce_config::RelationBuilderParams;
use cce_metrics::RelationMetrics;
use cce_plugin::PluginRegistry;
use cce_relation::BuildConfigParser;
use cce_relation::IndexBuilder;
use cce_relation::index::{
    RelationDeltaOps, RelationIndex, RelationIndexView, SnapshotFileQueryOps,
};
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::RelationSnapshotRepository;
use cce_storage_sqlite::snapshot_store::SqliteSnapshotStore;
use cce_types::{LanguageInfo, StorageError};

use crate::index::RelationBaseCache;
use crate::index::RelationSnapshotPublisher;
use crate::index::StorageCoordinator;
use cce_types::normalize_project_path;

pub struct RelationUpdateProcessor {
    /// SQLite client for persistence (legacy direct access, kept for project_meta reads).
    pub(crate) sqlite_client: Option<Arc<SqliteClient>>,
    /// Abstract snapshot store (preferred over `sqlite_client` for epoch/snapshot/delta ops).
    pub(crate) relation_store:
        Option<Arc<dyn crate::index::relation_store_trait::RelationSnapshotStore>>,
    /// Sole publisher for complete canonical snapshots.
    pub(crate) publisher: Option<Arc<dyn RelationSnapshotPublisher>>,
    /// Whether this processor is enabled
    pub(crate) enabled: bool,
    /// Whether to enable dependency propagation (Phase 3)
    pub(crate) dependency_propagation_enabled: bool,
    /// Maximum depth for dependency propagation (0 = unlimited)
    pub(crate) max_propagation_depth: usize,
    /// Upper bound for dependent-file reparsing during a single update.
    pub(crate) reparse_concurrency_limit: usize,
    /// Maximum ratio of the symbol-fingerprint scope size to the project
    /// file count before an incremental candidate is conservatively rejected
    /// (0.0 disables the check).
    pub(crate) max_fingerprint_scope_ratio: f64,
    /// Maximum number of relations retained for a rebuilt source file.
    pub(crate) max_relations_per_file: usize,
    /// Whether imports and exports are retained in relation candidates.
    pub(crate) analyze_imports: bool,
    /// Whether cross-file dependency edges are retained and propagated.
    pub(crate) track_cross_file_deps: bool,
    /// Whether standard library relations are excluded from candidates.
    pub(crate) filter_stdlib_calls: bool,
    /// Whether `SymbolExtract` plugins supply import extraction for custom
    /// languages during partial rebuilds (mirrors the full-index policy).
    pub(crate) symbol_extract_enabled: bool,
    /// Whether `RelationExtract` plugin symbols/relations enter the candidate
    /// graph (mirrors the full-index `plugin_symbols_enabled` policy).
    pub(crate) plugin_symbols_enabled: bool,
    /// Plugin registry for `RelationExtract` symbol/relation injection.
    pub(crate) plugin_registry: Option<Arc<PluginRegistry>>,
    /// Maximum directory depth for build-manifest discovery (mirrors
    /// `RelationConfig.manifest_scan_depth`).
    pub(crate) manifest_scan_depth: usize,
    /// Project root for config reload
    pub(crate) project_root: Mutex<Option<PathBuf>>,
    /// Project ID for database records
    pub(crate) project_id: i64,
    /// Shared publication coordinator for the hot-update data candidate.
    pub(crate) storage: Option<Arc<StorageCoordinator>>,
    /// Process-internal cache of the materialized relation base.
    ///
    /// Hot updates reuse the cached base instead of reloading the full graph
    /// from SQLite on every attempt; the base is rebuilt only when the active
    /// epoch advanced (CAS conflict) or after a cold start.
    pub(crate) base_cache: RelationBaseCache,
    /// Refuse relation writes when no unified publisher is configured.
    pub(crate) safe_mode: bool,
    /// Relation pipeline metrics (unbounded edge-dropped counter).
    pub(crate) relation_metrics: Option<Arc<RelationMetrics>>,
}

impl Default for RelationUpdateProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationUpdateProcessor {
    /// Create a new relation update processor
    pub fn new() -> Self {
        let params = RelationBuilderParams::default();
        Self {
            sqlite_client: None,
            relation_store: None,
            publisher: None,
            enabled: true,
            dependency_propagation_enabled: params.track_cross_file_deps,
            max_propagation_depth: params.max_propagation_depth,
            reparse_concurrency_limit: 8,
            max_fingerprint_scope_ratio: params.max_fingerprint_scope_ratio,
            max_relations_per_file: params.max_relations_per_file,
            analyze_imports: params.analyze_imports,
            track_cross_file_deps: params.track_cross_file_deps,
            filter_stdlib_calls: params.filter_stdlib_calls,
            symbol_extract_enabled: params.symbol_extract_enabled,
            plugin_symbols_enabled: params.plugin_symbols_enabled,
            plugin_registry: None,
            manifest_scan_depth: params.manifest_scan_depth,
            project_root: Mutex::new(None),
            project_id: 0,
            storage: None,
            base_cache: RelationBaseCache::new(),
            safe_mode: true,
            relation_metrics: None,
        }
    }

    /// Create a new relation update processor with build config loaded
    pub fn with_build_config(project_root: &std::path::Path) -> Self {
        let params = RelationBuilderParams::default();
        Self {
            sqlite_client: None,
            relation_store: None,
            publisher: None,
            enabled: true,
            dependency_propagation_enabled: params.track_cross_file_deps,
            max_propagation_depth: params.max_propagation_depth,
            reparse_concurrency_limit: 8,
            max_fingerprint_scope_ratio: params.max_fingerprint_scope_ratio,
            max_relations_per_file: params.max_relations_per_file,
            analyze_imports: params.analyze_imports,
            track_cross_file_deps: params.track_cross_file_deps,
            filter_stdlib_calls: params.filter_stdlib_calls,
            symbol_extract_enabled: params.symbol_extract_enabled,
            plugin_symbols_enabled: params.plugin_symbols_enabled,
            plugin_registry: None,
            manifest_scan_depth: params.manifest_scan_depth,
            project_root: Mutex::new(Some(project_root.to_path_buf())),
            project_id: 0,
            storage: None,
            base_cache: RelationBaseCache::new(),
            safe_mode: true,
            relation_metrics: None,
        }
    }

    /// Create a new relation update processor with SQLite persistence
    pub fn with_persistence(sqlite_client: Arc<SqliteClient>) -> Self {
        let params = RelationBuilderParams::default();
        Self {
            sqlite_client: Some(Arc::clone(&sqlite_client)),
            relation_store: Some(Arc::new(
                crate::index::relation_store_trait::SqliteRelationStore::new(Arc::clone(
                    &sqlite_client,
                )),
            )
                as Arc<dyn crate::index::relation_store_trait::RelationSnapshotStore>),
            publisher: None,
            enabled: true,
            dependency_propagation_enabled: params.track_cross_file_deps,
            max_propagation_depth: params.max_propagation_depth,
            reparse_concurrency_limit: 8,
            max_fingerprint_scope_ratio: params.max_fingerprint_scope_ratio,
            max_relations_per_file: params.max_relations_per_file,
            analyze_imports: params.analyze_imports,
            track_cross_file_deps: params.track_cross_file_deps,
            filter_stdlib_calls: params.filter_stdlib_calls,
            symbol_extract_enabled: params.symbol_extract_enabled,
            plugin_symbols_enabled: params.plugin_symbols_enabled,
            plugin_registry: None,
            manifest_scan_depth: params.manifest_scan_depth,
            project_root: Mutex::new(None),
            project_id: 0,
            storage: None,
            base_cache: RelationBaseCache::new(),
            safe_mode: false,
            relation_metrics: None,
        }
    }

    /// Create a new relation update processor with SQLite persistence and build config
    pub fn with_persistence_and_config(
        sqlite_client: Arc<SqliteClient>,
        project_root: &std::path::Path,
        project_id: i64,
    ) -> Self {
        let params = RelationBuilderParams::default();
        Self {
            sqlite_client: Some(Arc::clone(&sqlite_client)),
            relation_store: Some(Arc::new(
                crate::index::relation_store_trait::SqliteRelationStore::new(Arc::clone(
                    &sqlite_client,
                )),
            )
                as Arc<dyn crate::index::relation_store_trait::RelationSnapshotStore>),
            publisher: None,
            enabled: true,
            dependency_propagation_enabled: params.track_cross_file_deps,
            max_propagation_depth: params.max_propagation_depth,
            reparse_concurrency_limit: 8,
            max_fingerprint_scope_ratio: params.max_fingerprint_scope_ratio,
            max_relations_per_file: params.max_relations_per_file,
            analyze_imports: params.analyze_imports,
            track_cross_file_deps: params.track_cross_file_deps,
            filter_stdlib_calls: params.filter_stdlib_calls,
            symbol_extract_enabled: params.symbol_extract_enabled,
            plugin_symbols_enabled: params.plugin_symbols_enabled,
            plugin_registry: None,
            manifest_scan_depth: params.manifest_scan_depth,
            project_root: Mutex::new(Some(project_root.to_path_buf())),
            project_id,
            storage: None,
            base_cache: RelationBaseCache::new(),
            safe_mode: true,
            relation_metrics: None,
        }
    }

    /// Enable or disable relation hot updates.
    pub fn set_safe_mode(&mut self, enabled: bool) {
        self.safe_mode = enabled;
    }

    /// Check if safe mode is enabled.
    pub fn is_safe_mode(&self) -> bool {
        self.safe_mode
    }

    /// Set project ID
    pub fn set_project_id(&mut self, project_id: i64) {
        self.project_id = project_id;
    }

    /// Attach the server-owned publisher that synchronizes SQLite and runtime.
    pub fn with_publisher(mut self, publisher: Arc<dyn RelationSnapshotPublisher>) -> Self {
        self.publisher = Some(publisher);
        self.safe_mode = false;
        self
    }

    /// Attach the shared data-generation publication coordinator.
    pub fn with_storage(mut self, storage: Arc<StorageCoordinator>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Attach relation pipeline metrics (edge-dropped observability).
    pub fn with_relation_metrics(mut self, metrics: Arc<RelationMetrics>) -> Self {
        self.relation_metrics = Some(metrics);
        self
    }

    /// Attach the plugin registry used for `RelationExtract` symbol/relation
    /// injection into hot-update candidate graphs.
    pub fn with_plugin_registry(mut self, registry: Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(registry);
        self
    }

    /// Attach an abstract snapshot store (preferred over direct `SqliteClient`).
    pub fn with_relation_store(
        mut self,
        store: Arc<dyn crate::index::relation_store_trait::RelationSnapshotStore>,
    ) -> Self {
        self.relation_store = Some(store);
        self
    }

    /// Set whether dependency propagation is enabled
    pub fn set_dependency_propagation(&mut self, enabled: bool) {
        self.dependency_propagation_enabled = enabled;
    }

    /// Set maximum propagation depth (0 = unlimited)
    pub fn set_max_propagation_depth(&mut self, depth: usize) {
        self.max_propagation_depth = depth;
    }

    /// Set the maximum number of dependent files reparsed concurrently.
    pub fn set_reparse_concurrency_limit(&mut self, limit: usize) {
        self.reparse_concurrency_limit = limit.max(1);
    }
}

impl RelationUpdateProcessor {
    async fn publish_candidate_epoch(
        &self,
        operation_id: &str,
        batch_result: &BatchChangeResult,
        allow_config_fingerprint_change: bool,
    ) -> Result<usize> {
        self.publish_candidate_epoch_with_parser(
            operation_id,
            batch_result,
            allow_config_fingerprint_change,
            None,
        )
        .await
    }

    async fn publish_candidate_epoch_with_parser(
        &self,
        operation_id: &str,
        batch_result: &BatchChangeResult,
        allow_config_fingerprint_change: bool,
        preloaded_parser: Option<&BuildConfigParser>,
    ) -> Result<usize> {
        // Errors are never swallowed here. They propagate to the coordinator,
        // which (a) leaves the file hashes uncommitted so the next watcher scan
        // re-selects the same files and rebuilds the candidate, and (b) records
        // module failures. The server-side publisher additionally transitions
        // the relation runtime to Degraded and emits an explicit
        // `PublishFailed` runtime event.
        let sqlite = self.sqlite_client.as_ref().ok_or_else(|| {
            HotUpdateError::relation("relation hot update requires normalized epoch persistence")
        })?;
        let publisher = self.publisher.as_ref().ok_or_else(|| {
            HotUpdateError::relation(
                "relation hot update requires the unified snapshot publisher; full rebuild required",
            )
        })?;
        let project_id = self.persistence_project_id()?;

        const MAX_RETRIES: usize = 3;
        let mut last_error: Option<StorageError> = None;
        // Cache the config fingerprint across CAS retries so the per-builder
        // sort+hash is paid only once per hot-update operation.
        let mut cached_config_fingerprint: Option<String> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tracing::warn!(
                    operation_id,
                    attempt,
                    ?last_error,
                    "Retrying candidate epoch publish after CAS rejection"
                );
            }

            let active_epoch =
                match sqlite.project_meta_get_int(project_id, "active_relation_epoch") {
                    Ok(epoch) if epoch > 0 => epoch,
                    Ok(_) => {
                        return Err(HotUpdateError::relation(
                            "relation hot update requires an active canonical epoch",
                        ));
                    }
                    Err(error) => {
                        return Err(HotUpdateError::relation(error.to_string()));
                    }
                };

            let manifest = match sqlite.with_transaction(|tx| {
                RelationSnapshotRepository::get_manifest(tx, project_id, active_epoch)
            }) {
                Ok(Some(manifest)) => manifest,
                Ok(None) => {
                    return Err(HotUpdateError::relation(
                        "active relation manifest is missing",
                    ));
                }
                Err(error) => {
                    return Err(HotUpdateError::relation(error.to_string()));
                }
            };
            // Reuse the process-internal base cache
            // instead of reloading the full graph from SQLite on every
            // attempt. On a CAS conflict the active epoch has advanced and
            // the cache rebuilds from the store exactly once.
            // Read-only layered view over the cached state at `active_epoch`:
            // the materialized base plus the delta chain published on top of
            // it. Dependency propagation, fingerprinting, and delta
            // computation all read through this view; the cached base is
            // never mutated.
            let base_view = match self.base_cache.get_or_load(
                &SqliteSnapshotStore::new(sqlite.as_ref().clone()),
                project_id,
                active_epoch,
            ) {
                Ok(view) => view,
                Err(error) => {
                    return Err(HotUpdateError::relation(error.to_string()));
                }
            };
            let project_root = match self.project_root(sqlite) {
                Ok(root) => root,
                Err(error) => return Err(error),
            };

            let mut changed_files = HashSet::new();
            for change in &batch_result.file_changes {
                changed_files.insert(Self::candidate_path(&project_root, &change.path));
                if let Some(previous_path) = &change.previous_path {
                    changed_files.insert(Self::candidate_path(&project_root, previous_path));
                }
            }
            for result in &batch_result.parse_results {
                changed_files.insert(Self::candidate_path(&project_root, &result.file_path));
            }
            if !batch_result.failed_files.is_empty() {
                return Err(HotUpdateError::relation(
                    "relation candidate cannot be published because one or more changed files failed to parse",
                ));
            }
            let parsed_paths: HashSet<String> = batch_result
                .parse_results
                .iter()
                .map(|result| Self::candidate_path(&project_root, &result.file_path))
                .collect();
            for change in &batch_result.file_changes {
                if !matches!(change.change_type, FileChangeType::Deleted)
                    && !parsed_paths.contains(&Self::candidate_path(&project_root, &change.path))
                {
                    return Err(HotUpdateError::relation(format!(
                        "relation candidate is missing a parsed replacement for {}",
                        change.path.display()
                    )));
                }
            }
            let dependents = if self.dependency_propagation_enabled {
                Self::collect_candidate_dependents(
                    &*base_view,
                    &changed_files,
                    self.max_propagation_depth,
                )
            } else {
                HashSet::new()
            };

            let mut parsed_files = Vec::new();
            for result in &batch_result.parse_results {
                let mut parsed = result.parsed_file.clone();
                parsed.path = Self::candidate_path(&project_root, &result.file_path);
                parsed_files.push(parsed);
            }

            let supplied: HashSet<String> = parsed_files
                .iter()
                .map(|file| normalize_project_path(&file.path))
                .collect();
            let file_processor = Arc::new(crate::hot_update::file_processor::FileProcessor::new());
            let mut sorted_dependents: Vec<_> = dependents.iter().cloned().collect();
            sorted_dependents.sort();
            let pending_dependents: Vec<String> = sorted_dependents
                .into_iter()
                .filter(|dependent| !supplied.contains(dependent))
                // Document/config/text files carry no symbol relations, so
                // they cannot be real dependents; stale rows must not reach
                // the tree-sitter reparse below (their language is unsupported
                // and would fail the whole rebuild).
                .filter(|dependent| !LanguageInfo::detect_from_path(dependent).is_document_like())
                .collect();
            let parse_futures = pending_dependents.into_iter().map(|dependent| {
                let source_path = project_root.join(&dependent);
                let fp = Arc::clone(&file_processor);
                let base_view = Arc::clone(&base_view);
                async move {
                    if !source_path.exists() {
                        return Err(HotUpdateError::relation(format!(
                            "dependent file required for relation rebuild is missing: {}",
                            source_path.display()
                        )));
                    }
                    let result = fp
                        .reparse_file(&source_path, &dependent)
                        .await
                        .map_err(|e| {
                            HotUpdateError::relation(format!(
                                "Failed to reparse dependent {}: {e}",
                                source_path.display()
                            ))
                        })?;
                    // TOCTOU guard: the dependent's on-disk content must match
                    // the base snapshot it was resolved from. If the file was
                    // modified after change detection (or the parse raced a
                    // write), publishing the new content under the old hash
                    // would desynchronize the relation snapshot from the
                    // committed file hashes and the data-side modules. Reject
                    // the batch: the next scan re-detects the drift and
                    // processes the file as a normal change.
                    if let Some(message) = Self::dependent_content_drift_error(
                        &source_path,
                        &dependent,
                        result.content_hash.as_deref(),
                        &base_view,
                    ) {
                        return Err(HotUpdateError::relation(message));
                    }
                    // `reparse_file` already keyed the parse on the
                    // project-relative identity (`dependent`), so the result
                    // can flow into the relation build unchanged.
                    Ok(result.parsed_file)
                }
            });
            let results = stream::iter(parse_futures)
                .buffer_unordered(self.reparse_concurrency_limit)
                .collect::<Vec<_>>()
                .await;
            for result in results {
                match result {
                    Ok(parsed) => parsed_files.push(parsed),
                    Err(e) => return Err(e),
                }
            }

            let mut replaced_files = changed_files;
            replaced_files.extend(dependents);

            // Sparse candidate: an empty index whose entity ID counter
            // continues past the base's max ID; only the affected files are
            // loaded below. The read-only base view supplies the full symbol
            // context for cross-file resolution.
            let candidate_index = RelationIndex::new_with_entity_id_start(
                base_view.max_entity_id().saturating_add(1),
            );

            // Capture the old index state for delta computation before
            // building. The base view is read-only and shares the cached
            // base's maps; `compute_delta` and `stable_symbol_fingerprint`
            // only read it, and the cache is never mutated.
            let old_index = base_view.clone();

            let governance_parser_owned: Option<BuildConfigParser>;
            let mut builder = if let Some(parser) = preloaded_parser {
                governance_parser_owned = Some(parser.clone());
                let mut b = IndexBuilder::from_index(candidate_index);
                b.auto_load_dependencies(parser);
                b.set_filter_stdlib_calls(self.filter_stdlib_calls);
                b
            } else {
                match self.scan_build_config_async(project_root.clone()).await {
                    Ok(parser) => {
                        governance_parser_owned = Some(parser.clone());
                        let mut b = IndexBuilder::from_index(candidate_index);
                        b.auto_load_dependencies(&parser);
                        b.set_filter_stdlib_calls(self.filter_stdlib_calls);
                        b
                    }
                    Err(error) => {
                        self.record_config_scan_failure();
                        return Err(HotUpdateError::relation(format!(
                            "Failed to scan build config for relation candidate ({}); rejecting partial candidate, full rebuild required: {}",
                            project_root.display(),
                            error
                        )));
                    }
                }
            };
            if let Some(registry) = &self.plugin_registry {
                builder = builder.with_plugin_registry(Arc::clone(registry));
            }
            builder.with_plugin_symbols_enabled(self.plugin_symbols_enabled);
            builder.set_graph_options(
                self.max_relations_per_file,
                self.analyze_imports,
                self.track_cross_file_deps,
            );
            builder.set_symbol_extract_enabled(self.symbol_extract_enabled);
            let parsed_refs: Vec<_> = parsed_files.iter().collect();
            if !parsed_refs.is_empty() {
                // Cross-file resolution context comes from the read-only
                // layered base view (all base symbols, zero copy); only the
                // affected files are written into the sparse candidate.
                builder.add_parsed_files_with_index_symbols(
                    &parsed_refs,
                    &*base_view,
                    Some(&replaced_files),
                );
            }
            if let Some(gov_parser) = &governance_parser_owned {
                Self::inject_governance_edges(&builder, gov_parser);
            }
            let config_fingerprint = if let Some(cached) = &cached_config_fingerprint {
                cached.clone()
            } else {
                let fp = builder.config_fingerprint();
                cached_config_fingerprint = Some(fp.clone());
                fp
            };
            if config_fingerprint != manifest.config_fingerprint {
                if !allow_config_fingerprint_change {
                    return Err(HotUpdateError::relation(format!(
                        "Config fingerprint changed ({} -> {}); rejecting partial candidate. \
                         A full-scope rebuild via on_config_change is required to ensure \
                         consistent resolution semantics",
                        manifest.config_fingerprint, config_fingerprint
                    )));
                }
                tracing::info!(
                    "Config fingerprint changed ({} -> {}) during a config-change rebuild; \
                     the candidate carries the new fingerprint into the next manifest",
                    manifest.config_fingerprint,
                    config_fingerprint
                );
            }

            // Compute delta between old and new index
            let new_index = builder.index();

            // Symbol-table fingerprint: the stable symbols of files NOT touched
            // by this update must survive the candidate build exactly. Any drift
            // (e.g. symbol pre-population regression, plugin-symbol persistence
            // loss, or duplicate symbol keys) silently changes resolution
            // semantics, so reject the increment and require a full rebuild.
            // The check is scoped to the dependency closure of the replaced
            // files so its cost is bounded by the change size, not the project.
            // The candidate is sparse (affected files only), so the new-side
            // fingerprint sources each file from the candidate when present
            // and from the base view otherwise.
            let fingerprint_scope = Self::symbol_fingerprint_scope(
                &*old_index,
                new_index,
                &replaced_files,
                self.max_propagation_depth,
            );
            if Self::scope_exceeds_ratio(
                fingerprint_scope.len(),
                old_index.base.file_count(),
                self.max_fingerprint_scope_ratio,
            ) {
                return Err(HotUpdateError::relation(format!(
                    "Symbol-fingerprint scope ({} files) exceeds {:.2} of the project; \
                     rejecting partial candidate. A full-scope rebuild is required to bound \
                     the fingerprint traversal cost",
                    fingerprint_scope.len(),
                    self.max_fingerprint_scope_ratio
                )));
            }
            let old_symbol_fp = Self::stable_symbol_fingerprint(&*old_index, &fingerprint_scope);
            let new_symbol_fp =
                Self::stable_symbol_fingerprint_merged(&*old_index, new_index, &fingerprint_scope);
            if old_symbol_fp != new_symbol_fp {
                return Err(HotUpdateError::relation(format!(
                    "Symbol-table fingerprint changed for untouched files ({} -> {}); \
                     rejecting partial candidate. A full-scope rebuild is required to \
                     restore consistent resolution semantics",
                    old_symbol_fp, new_symbol_fp
                )));
            }

            let delta = new_index.compute_delta(
                &*old_index,
                active_epoch + 1,
                active_epoch,
                config_fingerprint,
                Some(&replaced_files),
            );

            // Edges whose caller files are outside the affected scope are
            // dropped by dangling cleanup and can never be re-derived (the
            // caller is never re-parsed). Surface the loss through a warning
            // and the relation metrics so under-propagated hot updates are
            // observable and repairable via a full rebuild.
            // When the unbounded drop count exceeds a threshold, escalate to an
            // error so the hot-update coordinator can fall back to a full rebuild
            // rather than silently losing call-graph edges.
            let dropped_unbounded = delta.relation_edges_dropped_unbounded;
            if dropped_unbounded > 0 {
                tracing::warn!(
                    operation_id,
                    epoch = active_epoch + 1,
                    dropped_edges = dropped_unbounded,
                    replaced_files = replaced_files.len(),
                    "relation hot update dropped {dropped_unbounded} edge(s) whose caller files \
                     are outside the affected scope; these edges are lost until a full rebuild"
                );
                if let Some(metrics) = &self.relation_metrics {
                    metrics
                        .relation_edges_dropped_unbounded_total
                        .add(dropped_unbounded);
                }
                let visible_relations = base_view.resolved_relation_count()
                    + delta.added_relations.len()
                    - delta.removed_relations.len();
                let threshold = 10u64;
                let ratio_exceeded = visible_relations > 0
                    && (dropped_unbounded as f64 / visible_relations as f64) > 0.01;
                if dropped_unbounded > threshold || ratio_exceeded {
                    return Err(HotUpdateError::relation(format!(
                        "relation hot update dropped {dropped_unbounded} edge(s) outside affected scope \
                         (threshold {threshold}, ratio 1%); falling back to full rebuild to preserve correctness"
                    )));
                }
            }

            // Append the published delta to the layered cache. No full
            // materialization on the hot path: the base is shared and only
            // compaction (inside the cache, when the chain crosses a
            // threshold) produces one O(project) merge.
            let cache_delta = delta.clone();
            match publisher
                .publish_delta(
                    project_id,
                    operation_id,
                    delta,
                    Some(base_view.as_ref().clone()),
                )
                .await
            {
                Ok(publication) => {
                    self.base_cache
                        .update(project_id, publication.relation_epoch, cache_delta);
                    if let Some(storage) = &self.storage {
                        storage.set_candidate_relation_epoch(publication.relation_epoch);
                    }
                    return Ok(replaced_files.len());
                }
                Err(StorageError::EpochConflict { .. }) if attempt + 1 < MAX_RETRIES => {
                    last_error = Some(StorageError::EpochConflict {
                        active: active_epoch,
                        base: active_epoch,
                    });
                    continue;
                }
                Err(error) => {
                    return Err(HotUpdateError::relation(error.to_string()));
                }
            }
        }

        Err(HotUpdateError::relation(format!(
            "Failed to publish candidate epoch after {MAX_RETRIES} retries: {}",
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        )))
    }

    /// Construct and publish a complete candidate graph from explicit changes.
    /// This is shared by watch and HTTP incremental entry points.
    pub async fn publish_batch(
        &self,
        operation_id: &str,
        batch_result: &BatchChangeResult,
    ) -> Result<usize> {
        self.publish_candidate_epoch(operation_id, batch_result, false)
            .await
    }

    /// Rebuild the relation index for the files affected by a configuration
    /// change and publish a new candidate epoch.
    ///
    /// The candidate lifecycle (begin/activate/fail) is NOT owned here: the
    /// operation pipeline drives it through `prepare_operation` /
    /// `commit_operation` / `abort_operation` for `OperationType::ConfigChange`
    /// operations, while the standalone `on_config_change` trait callback
    /// wraps this method in its own candidate lifecycle for direct callers.
    ///
    /// Configuration changes may alter which files belong to the project and
    /// how they are grouped; the resulting graph can differ arbitrarily from
    /// the previous one. This is a COLD path: it is acceptable for it to
    /// re-materialize the whole relation index (O(project)), in contrast to
    /// the file-change hot path which stays O(sparse) via the delta chain.
    async fn rebuild_affected_for_config(
        &self,
        config_path: &Path,
        project_root: &Path,
        operation_id: &str,
    ) -> Result<usize> {
        if !Self::is_relevant_config(config_path) {
            tracing::debug!(
                "Config file {} is not relevant for relation processor",
                config_path.display()
            );
            return Ok(0);
        }

        tracing::info!(
            "Handling configuration change for relation processor: {}",
            config_path.display()
        );

        // Step 1: Snapshot old package set before reload (offloaded to
        // blocking pool so the async executor is not stalled). A failure
        // here means we cannot compute a correct diff; reject the candidate
        // and require a full rebuild instead of silently guessing.
        let mut old_parser = BuildConfigParser::new();
        old_parser
            .scan_project_async(project_root.to_path_buf(), self.manifest_scan_depth)
            .await
            .map_err(|e| {
                self.record_config_scan_failure();
                HotUpdateError::relation(format!(
                    "Failed to scan old project state for config diff: {e}"
                ))
            })?;

        // Step 1b: Persist the project root for future reloads without an
        // extra filesystem scan. The fresh scan in the next step validates
        // the new configuration; a separate validation scan would be redundant.
        {
            let mut root = self.project_root.lock().await;
            *root = Some(project_root.to_path_buf());
        }

        let mut new_parser = BuildConfigParser::new();
        new_parser
            .scan_project_async(project_root.to_path_buf(), self.manifest_scan_depth)
            .await
            .map_err(|e| {
                self.record_config_scan_failure();
                HotUpdateError::relation(format!(
                    "Failed to scan project for new config state; rejecting candidate, full rebuild required: {e}"
                ))
            })?;

        // Step 2: Identify affected files with fine-grained narrowing.
        // Failures in fine-grained narrowing fall back to the extension-based
        // set (expected behavior); the new-config scan above guarantees we
        // have valid data for diffing and synthetic-node construction.
        let affected_files = match self
            .identify_affected_files_by_config_fine_grained(config_path, &old_parser, &new_parser)
            .await
        {
            Ok(files) => files,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Fine-grained config affected set failed; falling back to extension-based set"
                );
                self.identify_affected_files_by_config(config_path).await?
            }
        };

        if affected_files.is_empty() {
            tracing::info!(
                "No source files identified as affected by config change: {}; updating synthetic config node only",
                config_path.display()
            );
            // Continue to publish the synthetic config file alone so its
            // `config -> dependency` edges and file hash stay current even
            // when no source file imports a changed package.
        }

        // Step 3: Parse all affected files before replacing any old relation
        // data. This keeps the previous graph queryable if parsing fails.
        let file_processor = crate::hot_update::file_processor::FileProcessor::new();
        let mut rebuild_batch = BatchChangeResult::new();
        for file_path in &affected_files {
            // Storage identity stays project-relative even though the file is
            // read through its absolute on-disk path.
            let parse_path = cce_types::path::relativize(project_root, file_path);
            rebuild_batch
                .add_parse_result(file_processor.reparse_file(file_path, &parse_path).await?);
        }

        // Step 3b: Include an updated synthetic node for the changed config
        // file itself so the `config_file -> dependency` edges and file hash
        // in the relation graph reflect the new content. Delegates to the
        // parser's single source of truth for synthetic construction.
        {
            let synthetic_rel = cce_types::path::relativize(project_root, config_path);
            let synthetic = new_parser.synthetic_parsed_file_for(project_root, &synthetic_rel);
            let result = crate::hot_update::change::ParseResultWithChanges::new(
                config_path.to_path_buf(),
                synthetic,
                crate::hot_update::FileChangeType::Modified,
                false,
            );
            rebuild_batch.add_parse_result(result);
        }

        // Step 4: Publish the affected graph through the same canonical epoch
        // protocol used by ordinary hot updates. Reuse the freshly scanned
        // parser to avoid a third filesystem traversal in the candidate builder.
        let parser_ref = Some(&new_parser);
        self.publish_candidate_epoch_with_parser(operation_id, &rebuild_batch, true, parser_ref)
            .await
    }
}

#[async_trait]
impl UpdateProcessor for RelationUpdateProcessor {
    fn name(&self) -> &'static str {
        "relation"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn supports_config_reload(&self) -> bool {
        true
    }

    async fn prepare_operation(&self, ctx: &crate::operation::OperationContext) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        storage
            .begin_hot_update_candidate(&ctx.operation_id, ctx.resume)
            .await
            .map(|_| ())
            .map_err(|error| HotUpdateError::relation(error.to_string()))
    }

    async fn commit_operation(&self, ctx: &crate::operation::OperationContext) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        storage
            .activate_hot_update_candidate(&ctx.operation_id)
            .map_err(|error| HotUpdateError::relation(error.to_string()))?;
        if let Err(error) = storage.gc_stale_generations().await {
            tracing::warn!(error = %error, "Generation GC after relation publication failed");
        }
        // Compact the durable delta chain now that this operation's candidate
        // is active (and no operation is in flight). The publisher's
        // `maybe_compact` is a no-op below the thresholds and defers itself
        // when another publication candidate is detected.
        if let Some(publisher) = &self.publisher {
            let project_id = self.persistence_project_id()?;
            if let Err(error) = publisher.maybe_compact(project_id).await {
                tracing::warn!(
                    error = %error,
                    "Relation delta-chain compaction after commit failed; chain compaction deferred"
                );
            }
        }
        Ok(())
    }

    async fn abort_operation(
        &self,
        ctx: &crate::operation::OperationContext,
        reason: &str,
    ) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        storage
            .fail_hot_update_candidate(&ctx.operation_id, reason)
            .map_err(|error| HotUpdateError::relation(error.to_string()))
    }

    async fn reload_config(&self, config_path: &Path, project_root: &Path) -> Result<()> {
        // Only reload if this is a relevant config file
        if !Self::is_relevant_config(config_path) {
            tracing::debug!(
                "Config file {} is not relevant for relation processor, skipping reload",
                config_path.display()
            );
            return Ok(());
        }

        tracing::info!(
            "Reloading build configuration for: {}",
            config_path.display()
        );

        // Reload build configurations from project root
        self.reload_build_config(project_root).await?;

        tracing::info!(
            "Successfully reloaded build configuration for: {}",
            config_path.display()
        );

        Ok(())
    }

    async fn reload_all_configs(&self, project_root: &Path) -> Result<()> {
        tracing::info!(
            "Reloading all build configurations from: {}",
            project_root.display()
        );

        // Reload all build configurations from project root
        self.reload_build_config(project_root).await?;

        tracing::info!(
            "Successfully reloaded all build configurations from: {}",
            project_root.display()
        );

        Ok(())
    }

    async fn on_config_change(&self, config_path: &Path, project_root: &Path) -> Result<()> {
        // Standalone delivery for direct callers (e.g. full config reloads):
        // this path owns its own candidate lifecycle because no operation
        // pipeline is involved. Pipeline-driven config changes run through
        // `process_operation`'s ConfigChange branch instead, where
        // prepare/commit/abort drive the candidate.
        let operation_id = format!("config-hot-{}", uuid::Uuid::new_v4());
        if let Some(storage) = &self.storage {
            storage
                .begin_hot_update_candidate(&operation_id, false)
                .await
                .map_err(|error| HotUpdateError::relation(error.to_string()))?;
            match self
                .rebuild_affected_for_config(config_path, project_root, &operation_id)
                .await
            {
                Ok(_) => storage
                    .activate_hot_update_candidate(&operation_id)
                    .map_err(|error| HotUpdateError::relation(error.to_string()))?,
                Err(error) => {
                    let _ = storage.fail_hot_update_candidate(&operation_id, &error.to_string());
                    return Err(error);
                }
            }
        } else {
            self.rebuild_affected_for_config(config_path, project_root, &operation_id)
                .await?;
        }
        if let Some(publisher) = &self.publisher {
            let project_id = self.persistence_project_id()?;
            if let Err(error) = publisher.maybe_compact(project_id).await {
                tracing::warn!(
                    error = %error,
                    "Relation delta-chain compaction after config change failed; deferred"
                );
            }
        }

        tracing::info!(
            "Configuration change handled for: {}. Affected relation files were rebuilt.",
            config_path.display()
        );

        Ok(())
    }

    async fn process_operation(
        &self,
        ctx: &crate::operation::OperationContext,
        batch_result: &mut crate::hot_update::BatchChangeResult,
    ) -> crate::hot_update::Result<crate::operation::OperationProcessResult> {
        use crate::operation::{OperationMetrics, OperationProcessResult};
        use std::time::Instant;

        if !self.enabled {
            return Ok(OperationProcessResult {
                operation_id: ctx.operation_id.clone(),
                processed_files: 0,
                success_files: Vec::new(),
                failed_modules: Vec::new(),
                metrics: OperationMetrics::default(),
            });
        }

        // Configuration-change operations rebuild the affected relation files
        // through the pipeline's candidate protocol (prepare/commit/abort are
        // driven by the runtime); the change set is empty by design.
        if ctx.operation_type == crate::operation::OperationType::ConfigChange {
            let start = Instant::now();
            let config_path = ctx.config_path.as_deref().ok_or_else(|| {
                HotUpdateError::relation("ConfigChange operation requires a config path")
            })?;
            let sqlite = self.sqlite_client.as_ref().ok_or_else(|| {
                HotUpdateError::relation("config change requires normalized epoch persistence")
            })?;
            let project_root = self.project_root(sqlite)?;
            let processed_count = self
                .rebuild_affected_for_config(config_path, &project_root, &ctx.operation_id)
                .await?;
            return Ok(OperationProcessResult {
                operation_id: ctx.operation_id.clone(),
                processed_files: processed_count,
                success_files: Vec::new(),
                failed_modules: Vec::new(),
                metrics: OperationMetrics {
                    duration_ms: start.elapsed().as_millis() as i64,
                    llm_tokens_used: None,
                    llm_cost_usd: None,
                    error_count: 0,
                },
            });
        }

        if self.safe_mode {
            return Err(HotUpdateError::relation(format!(
                "relation hot update {} requires the unified snapshot publisher; full rebuild required",
                ctx.operation_id
            )));
        }

        let start = Instant::now();
        let processed_count = self
            .publish_candidate_epoch(&ctx.operation_id, batch_result, false)
            .await?;

        // All parse_result file paths are considered successes for relations
        // (individual failures are tracked inside publish_candidate_epoch).
        let success_files: Vec<String> = batch_result
            .parse_results
            .iter()
            .map(|r| r.file_path.to_string_lossy().to_string())
            .collect();

        tracing::trace!(
            operation_id = %ctx.operation_id,
            processed = processed_count,
            "Canonical relation candidate activated"
        );

        Ok(OperationProcessResult {
            operation_id: ctx.operation_id.clone(),
            processed_files: processed_count,
            success_files,
            failed_modules: Vec::new(),
            metrics: OperationMetrics {
                duration_ms: start.elapsed().as_millis() as i64,
                llm_tokens_used: None,
                llm_cost_usd: None,
                error_count: 0,
            },
        })
    }
}
