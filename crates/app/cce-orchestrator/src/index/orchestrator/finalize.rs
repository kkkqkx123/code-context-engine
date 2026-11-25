//! Post-batch finalization for full index runs.
//!
//! After every batch succeeds, the relation graph is rebuilt once against the
//! complete project symbol snapshot, published as a canonical snapshot, NL
//! documents are exported (with stale-document cleanup), and the operation
//! manifest is activated.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use cce_codegraph::index::{BuildIndexSink, LayeredSnapshotIndex};
use cce_export::NlDocumentExporter;

use super::{FullIndexContext, IndexOrchestrator};
use crate::error::OrchestratorError;
use crate::index_state::{ModuleType, ModuleUpdateState};

/// Rebuild the resolvable type context for one file without retaining it
/// across files. Cross-file return types stay in the propagator; only the
/// current file's scoped bindings are installed while it is processed.
fn install_type_context(
    file: &cce_types::ParsedFile,
    symbols: &cce_codegraph::symbol_table::ProjectSymbolTable,
) {
    let module_path = cce_codegraph::index::file_module_path(file, &symbols.root_path);
    let package_name = cce_codegraph::index::project_package_name(&symbols.root_path);
    let type_index = cce_codegraph::index::build_file_type_index(file, &module_path, &package_name);
    let type_ctx = cce_codegraph::index::infer_file_type_context(file, &type_index);
    symbols.set_type_inference_context(&file.path, type_ctx);
}

impl IndexOrchestrator {
    /// Finalize index after batch processing completes.
    ///
    /// Returns the resolved relation count from the last published snapshot.
    /// The terminal builder is retired at publish, so no builder graph is
    /// consulted here.
    pub(super) fn finalize_index(&self) -> usize {
        if let Some(ref snapshot) = self.last_published_snapshot {
            let count = snapshot.resolved_relation_count();
            tracing::info!("Relation index finalized with {} relations", count);
            count
        } else {
            0
        }
    }

    /// Share the published snapshot with the NL exporter.
    ///
    /// The snapshot is the publisher's product and is independent of the
    /// retired builder's lifetime.
    fn share_published_snapshot_with_exporter(
        &self,
        snapshot: &Arc<cce_codegraph::index::LayeredSnapshotIndex>,
    ) {
        if let Some(ref exporter) = self.nl_exporter {
            if exporter.config().enable_relation_enhancement {
                let enhancer_config = cce_export::RelationEnhancerConfig::default();
                tracing::info!("Sharing published relation snapshot with NL document export");
                exporter.set_relation_enhancement(Arc::clone(snapshot), enhancer_config);
            }
        }
    }

    /// Get the last published full-index snapshot (read-only shared product).
    ///
    /// Replaces the former long-lived builder read path. Callers must treat
    /// the snapshot as immutable; no mutable builder access exists anymore.
    pub fn get_published_relation_snapshot(&self) -> Option<Arc<LayeredSnapshotIndex>> {
        self.last_published_snapshot.as_ref().map(Arc::clone)
    }

    /// Get a materialized relation index from the last published snapshot.
    ///
    /// Test and debug only: materializes the merged (base + deltas) view into
    /// an owned `RelationIndex` so existing query assertions keep working
    /// without touching the retired builder. Production paths must use
    /// `get_published_relation_snapshot` directly to avoid the O(project)
    /// copy.
    pub fn get_relation_index(&self) -> Option<cce_codegraph::ThreadSafeIndex> {
        self.last_published_snapshot
            .as_ref()
            .map(|snapshot| snapshot.materialize_merged_index())
    }

    /// Get a reference to the NL document exporter, if configured
    pub fn get_nl_exporter(&self) -> Option<&Arc<NlDocumentExporter>> {
        self.nl_exporter.as_ref()
    }

    /// Rebuild the relation graph once against the complete symbol snapshot and
    /// publish it as a canonical snapshot.
    ///
    /// Parsed relation inputs are replayed from the operation-local spool so
    /// peak memory remains bounded by one parsed file plus the final relation
    /// index. The builder is round-local: constructed fresh per run, handed
    /// over to the publisher via shared ownership, and retired afterwards.
    /// No clearing or reuse occurs across runs.
    pub(super) async fn build_and_publish_relations(
        &mut self,
        ctx: &mut FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        let spool = ctx.relation_spool.as_mut().ok_or_else(|| {
            OrchestratorError::index("relation_build_spool", "relation spool is unavailable")
        })?;
        let store_path = spool.directory().join("relation-build-store.sqlite3");
        let store =
            cce_codegraph::index::BuildRelationStore::new(&store_path).map_err(|error| {
                OrchestratorError::index(
                    "relation_build_store",
                    format!("Failed to create relation build store: {error}"),
                )
            })?;
        let builder = self.create_relation_builder()?.into_disk_builder(store);
        let relation_build_started = Instant::now();
        let rss_trace = std::env::var_os("CCE_RELATION_RSS_TRACE").is_some();
        let trace_rss = |stage: &str| {
            if rss_trace {
                eprintln!(
                    "[relation-rss] stage={stage} rss_kb={}",
                    cce_utils::rss_kb().unwrap_or(0)
                );
            }
        };
        trace_rss("relation_build_start");

        // Inject synthetic config file nodes (e.g. Cargo.toml, package.json)
        // so the relation graph contains `config_file -> dependency` edges and
        // `source_file -> config_file` governance linkage. The synthetic files
        // are produced from the single build-system scan performed in
        // `init_relation_builder` and appended to the spool so they participate
        // in both registration and resolution passes (no second filesystem scan).
        {
            let project_root = spool.project_symbols().root_path.clone();
            let parser = self.cached_build_config.as_ref().ok_or_else(|| {
                tracing::error!(
                    "build config not initialized; init_relation_builder must succeed before finalize"
                );
                OrchestratorError::index(
                    "relation_build_config",
                    "build config not initialized; init_relation_builder must succeed before finalize",
                )
            })?.clone();
            let synthetic_files = {
                let parser_clone = parser.clone();
                let root_clone = project_root.clone();
                match tokio::task::spawn_blocking(move || {
                    parser_clone.synthetic_config_parsed_files(&root_clone)
                })
                .await
                {
                    Ok(files) => files,
                    Err(e) => {
                        let reason = format!(
                            "Failed to generate synthetic config files for {}: spawn_blocking task failed: {e}",
                            project_root.display()
                        );
                        tracing::error!(
                            error = %e,
                            root = %project_root.display(),
                            "Failed to generate synthetic config files: spawn_blocking task failed"
                        );
                        ctx.errors.push(reason);
                        ctx.all_batches_completed = false;
                        Vec::new()
                    }
                }
            };
            let mut injected = 0usize;
            for synthetic in &synthetic_files {
                if let Err(error) = spool.append(synthetic) {
                    tracing::warn!(
                        path = %synthetic.path,
                        error = %error,
                        "Failed to spool synthetic config file"
                    );
                    continue;
                }
                injected += 1;
            }
            if injected > 0 {
                tracing::info!(
                    injected,
                    "Injected synthetic config files into relation spool"
                );
            }
        }

        let plugin_enabled =
            self.relation_config.plugin_symbols_enabled && builder.plugin_registry().is_some();
        // Spool replay observability helper
        let record_pass = |elapsed: std::time::Duration, bytes: u64| {
            if let Some(metrics) = builder.metrics() {
                metrics.relation_spool_replay_bytes_total.add(bytes);
                metrics.relation_spool_replay_passes_total.increment();
                metrics
                    .relation_spool_decompress_ms
                    .observe(elapsed.as_secs_f64() * 1000.0);
            }
        };
        let total_bytes = spool.total_encoded_bytes();
        ctx.stages.relation_spool_entries = spool.entry_count() as u64;
        ctx.stages.relation_spool_bytes = total_bytes;

        let mut symbol_builder =
            cce_codegraph::SymbolTableBuilder::new(spool.project_symbols().root_path.clone());
        symbol_builder.with_metrics(self.relation_metrics());
        let project_symbols = spool.project_symbols();
        let mut session = symbol_builder.begin_streaming_build();
        let symbols_started = Instant::now();
        spool
            .for_each(|parsed| session.add_file(parsed, project_symbols))
            .map_err(|error| {
                OrchestratorError::index(
                    "relation_build_spool",
                    format!("Failed to build symbols from spool: {error}"),
                )
            })?;
        let symbol_timings = session.finish(project_symbols);
        trace_rss("symbol_add_files_done");
        record_pass(symbols_started.elapsed(), total_bytes);
        let symbols_ms = symbols_started.elapsed().as_millis() as u64;
        ctx.stages.symbol_modules_ms += symbol_timings.modules_ms;
        ctx.stages.symbol_type_index_ms += symbol_timings.type_index_ms;
        ctx.stages.symbol_overload_ms += symbol_timings.overload_ms;
        ctx.stages.symbol_flat_ms += symbol_timings.flat_ms;
        ctx.stages.symbol_cross_file_ms += symbol_timings.cross_file_ms;
        ctx.stages.relation_symbols_ms += symbols_ms;

        let propagation_started = Instant::now();
        spool
            .for_each(|parsed| {
                install_type_context(parsed, project_symbols);
                project_symbols.insert_cross_file_for_file(parsed);
                project_symbols.propagate_cross_file_variables_for_file(parsed);
                project_symbols.forget_type_inference_context(&parsed.path);
            })
            .map_err(|error| {
                OrchestratorError::index(
                    "relation_build_spool",
                    format!("Failed to propagate cross-file symbols: {error}"),
                )
            })?;
        ctx.stages.symbol_cross_file_ms += propagation_started.elapsed().as_millis() as u64;
        record_pass(propagation_started.elapsed(), total_bytes);
        project_symbols
            .cross_file_propagator()
            .drop_invalidation_indexes();
        trace_rss("symbol_propagation_done");

        let artifact_options = cce_types::SnapshotArtifactOptions::default()
            .with_shard_items(self.relation_config.shard.max_shard_items);
        let t0 = Instant::now();
        let registered_files = spool
            .for_each(|parsed| {
                builder.register_file_entities(parsed);
                if plugin_enabled {
                    builder.register_file_plugin_symbols(parsed, project_symbols);
                }
            })
            .map_err(|error| {
                OrchestratorError::index(
                    "relation_build_spool",
                    format!("Failed to replay relation entities: {error}"),
                )
            })?;
        record_pass(t0.elapsed(), total_bytes);
        if let Some(parser) = &self.cached_build_config {
            Self::inject_governance_edges(builder.store(), parser);
        }
        let t1 = Instant::now();
        let resolved_files = spool
            .for_each(|parsed| {
                install_type_context(parsed, project_symbols);
                builder.resolve_file_relations(parsed, project_symbols);
                if plugin_enabled {
                    builder.inject_plugin_relations(parsed, project_symbols);
                }
                project_symbols.forget_type_inference_context(&parsed.path);
            })
            .map_err(|error| {
                OrchestratorError::index(
                    "relation_build_spool",
                    format!("Failed to replay relation edges: {error}"),
                )
            })?;
        record_pass(t1.elapsed(), total_bytes);
        trace_rss("relation_resolution_done");

        let (symbol_entries, symbol_bytes) = project_symbols.capacity_counts();
        ctx.stages.symbol_table_entries = symbol_entries;
        ctx.stages.symbol_table_estimated_bytes = symbol_bytes;
        let (nodes, edges, relation_bytes) = builder.store().capacity_counts();
        ctx.stages.relation_nodes = nodes;
        ctx.stages.relation_edges = edges;
        ctx.stages.relation_estimated_bytes = relation_bytes;

        if registered_files != resolved_files {
            // explicit check (not only a debug assertion) — a mismatch
            // between the registration and resolution passes indicates a
            // spool replay bug that would silently drop relations.
            let error = format!(
                "relation replay mismatch: registered {registered_files} files but resolved {resolved_files}"
            );
            tracing::error!(operation_id = %ctx.operation_id, "{error}");
            ctx.errors.push(error);
            ctx.all_batches_completed = false;
        }
        // Derive implicit hierarchy edges (Go interface method-set
        // satisfaction) once the complete graph is resolved and before the
        // canonical snapshot is taken.
        builder.derive_implicit_hierarchy_edges(spool.project_symbols());
        trace_rss("derived_relations_done");
        let relation_count = builder.store().relation_count();
        builder.record_build(
            relation_build_started.elapsed(),
            registered_files,
            relation_count,
        );
        let (nodes, _edges, relation_bytes) = builder.store().capacity_counts();
        ctx.stages.relation_nodes = nodes;
        ctx.stages.relation_edges = relation_count as u64;
        ctx.stages.relation_estimated_bytes = relation_bytes;

        let artifact_result = builder
            .store()
            .write_artifact(builder.config_fingerprint().to_string(), artifact_options);
        trace_rss("artifact_finish_done");
        match artifact_result {
            Ok(artifact) => {
                let counts = artifact.counts();
                ctx.stages.relation_snapshot_entries =
                    (counts.files + counts.entities + counts.relations + counts.dependencies)
                        as u64;
                ctx.stages.relation_snapshot_estimated_bytes = builder.store().capacity_counts().2;
                ctx.stages.canonical_write_batches = artifact.write_batches();
                ctx.stages.canonical_write_buffer_peak_bytes = artifact.buffer_peak_bytes();
                if let Ok(plan) =
                    cce_types::plan_snapshot_shards(&artifact, artifact_options.max_shard_items)
                {
                    ctx.stages.relation_shard_count = plan.descriptors.len() as u64;
                    ctx.stages.relation_shard_directory_entries =
                        (plan.path_shards.len() + plan.descriptors.len()) as u64;
                    ctx.stages.relation_shard_directory_estimated_bytes = plan
                        .descriptors
                        .iter()
                        .map(|descriptor| descriptor.estimated_bytes)
                        .sum();
                }
                match &self.relation_publisher {
                    Some(publisher) => {
                        match publisher
                            .publish(self.project_id, &ctx.operation_id, &artifact)
                            .await
                        {
                            Ok(publication) => {
                                ctx.published_relation_epoch = Some(publication.relation_epoch);
                                // Retain the shared product for read-only queries; the
                                // round-local builder is retired here (dropped) and
                                // never touched again, so the shared maps are
                                // effectively immutable.
                                self.last_published_snapshot =
                                    Some(Arc::clone(&publication.snapshot));
                                let report = publication.snapshot.quality_report();
                                ctx.stages.published_snapshot_entries = (report.file_count
                                    + report.entity_count
                                    + report.relation_count)
                                    as u64;
                                // Share the publisher's product with export
                                // enhancement. The snapshot is independent of the
                                // retired builder lifetime.
                                self.share_published_snapshot_with_exporter(&publication.snapshot);
                                // the relation phase is only marked complete
                                // (and files successful) AFTER the snapshot was
                                // actually built and published. The previous
                                // per-batch marking ran before the build and could
                                // report success for an operation whose relation
                                // phase was skipped entirely (batch failure).
                                let mut relation_paths: Vec<std::path::PathBuf> = Vec::new();
                                if let Err(error) = spool.for_each(|parsed| {
                                    relation_paths
                                        .push(std::path::PathBuf::from(parsed.path.clone()));
                                }) {
                                    tracing::warn!(
                                        error = %error,
                                        "Failed to replay spool paths for relation state marking"
                                    );
                                }
                                self.state_tracker
                                .mark_phase_complete(
                                    &relation_paths,
                                    crate::index_state::IndexPhase::RelationBuilding,
                                )
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!(error = %e, "State tracking operation failed");
                                });
                                for path in &relation_paths {
                                    self.state_tracker
                                        .mark_success(path, ModuleType::Relation)
                                        .await
                                        .unwrap_or_else(|e| {
                                            tracing::warn!(
                                                error = %e,
                                                "State tracking operation failed"
                                            );
                                        });
                                }
                            }
                            Err(error) => {
                                ctx.errors.push(format!(
                                    "Failed to publish canonical relation snapshot: {error}"
                                ));
                                ctx.all_batches_completed = false;
                            }
                        }
                    }
                    None => {
                        // Production always wires a publisher. Without one there is
                        // no query facade to activate, so the artifact is discarded
                        // with the build store and only the stage report is kept.
                        tracing::warn!(
                            operation_id = %ctx.operation_id,
                            "No relation snapshot publisher configured; canonical artifact is not activated"
                        );
                    }
                }
            }
            Err(error) => {
                ctx.errors.push(format!(
                    "Failed to create canonical relation snapshot: {error}"
                ));
                ctx.all_batches_completed = false;
            }
        }
        let overload_started = Instant::now();
        spool.project_symbols().rebuild_overload_sets();
        ctx.stages.symbol_overload_ms += overload_started.elapsed().as_millis() as u64;
        Ok(())
    }

    fn inject_governance_edges<I: BuildIndexSink>(
        sink: &I,
        parser: &cce_codegraph::BuildConfigParser,
    ) {
        use cce_types::relation::CallContext;
        use cce_types::{EntityId, LanguageInfo, RelationType, ResolvedRelation, Span};
        use std::collections::{HashMap, HashSet};

        let mut pkg_to_configs: HashMap<String, Vec<String>> = HashMap::new();
        for (cfg, deps) in parser.config_file_dependencies() {
            for dep in deps {
                pkg_to_configs
                    .entry(dep.name.clone())
                    .or_default()
                    .push(cfg.clone());
            }
        }
        if pkg_to_configs.is_empty() {
            return;
        }
        sink.for_each_import(|path, table| {
            let lang = LanguageInfo::detect_from_path(path).language;
            let mut matched_configs: HashSet<String> = HashSet::new();
            for imp in &table.standardized_imports {
                for (pkg, cfgs) in &pkg_to_configs {
                    if cce_types::build_system::imports_match_package(&imp.source, pkg, lang) {
                        for cfg in cfgs {
                            matched_configs.insert(cfg.clone());
                        }
                    }
                    if let Some(alias) = &imp.alias
                        && cce_types::build_system::imports_match_package(alias, pkg, lang)
                    {
                        for cfg in cfgs {
                            matched_configs.insert(cfg.clone());
                        }
                    }
                }
            }
            if matched_configs.is_empty() {
                return;
            }
            for cfg in matched_configs {
                sink.add_dependency(path, &cfg);
                let cfg_entities = sink.entities_of_file(&cfg);
                if let Some(cfg_entity) = cfg_entities.first() {
                    let rel = ResolvedRelation {
                        caller: EntityId(0),
                        callee_id: Some(cfg_entity.id),
                        callee_name: cfg.clone(),
                        relation_type: RelationType::DirectCall,
                        span: Span::default(),
                        is_external: false,
                        external_type: None,
                        callee_symbol: None,
                        stdlib_category: None,
                        owner_type: None,
                        call_context: CallContext::Direct,
                        overload_signature: None,
                        call_frequency: 1,
                        cfg_condition: None,
                    };
                    sink.add_file_relation(path, rel);
                }
            }
        });
    }

    /// Export NL documents after the relation index is finalized so relation
    /// enhancement is active. Removes stale documents whose sources are no
    /// longer part of the index.
    pub(super) async fn export_nl_documents(
        &self,
        ctx: &mut FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        let Some(ref exporter) = self.nl_exporter else {
            return Ok(());
        };
        let Some(ref spool) = ctx.export_spool else {
            return Ok(());
        };
        let mut export_success = 0usize;
        let mut export_failed = 0usize;
        let mut exported_paths: Vec<std::path::PathBuf> = Vec::new();
        for file_path in spool.file_paths() {
            let chunks = match spool.load_chunks(&file_path) {
                Ok(chunks) => chunks,
                Err(error) => {
                    export_failed += 1;
                    tracing::warn!(
                        path = %file_path,
                        error = %error,
                        "Failed to load export chunks; skipping file without aborting export"
                    );
                    self.state_tracker
                        .mark_failed(
                            &std::path::PathBuf::from(&file_path),
                            ModuleType::Export,
                            crate::index_state::TrackerFailure::transient(error.to_string()),
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "State tracking operation failed");
                        });
                    ctx.errors.push(format!(
                        "NL document export skipped corrupt spool entry {file_path}: {error}"
                    ));
                    continue;
                }
            };
            self.state_tracker
                .update_module_state(
                    &std::path::PathBuf::from(&file_path),
                    ModuleType::Export,
                    ModuleUpdateState::Updating,
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "State tracking operation failed");
                });
            let summary = ctx
                .export_summaries_by_file
                .get(&file_path)
                .map(cce_export::ExportSummaryView::from);
            match exporter.export_file(&chunks, summary.as_ref()).await {
                Ok(_) => {
                    export_success += 1;
                    exported_paths.push(std::path::PathBuf::from(&file_path));
                    self.state_tracker
                        .mark_success(&std::path::PathBuf::from(&file_path), ModuleType::Export)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "State tracking operation failed");
                        });
                }
                Err(e) => {
                    export_failed += 1;
                    tracing::warn!(
                        path = %file_path,
                        error = %e,
                        "Failed to export NL document"
                    );
                    self.state_tracker
                        .mark_failed(
                            &std::path::PathBuf::from(&file_path),
                            ModuleType::Export,
                            crate::index_state::TrackerFailure::transient(e.to_string()),
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "State tracking operation failed");
                        });
                    ctx.errors.push(format!(
                        "NL document export failed for {}: {}",
                        file_path, e
                    ));
                }
            }
        }

        if export_success > 0 || export_failed > 0 {
            tracing::info!(
                exported = export_success,
                failed = export_failed,
                files = exported_paths.len(),
                "NL document export completed after relation finalize"
            );
        }

        // Remove documents left over from source files that no longer
        // exist in the index (deleted files or newly excluded paths).
        // The full-index path re-exports only currently-indexed files,
        // so without this pass stale `.md` documents accumulate.
        let kept_sources: HashSet<String> = spool
            .file_paths()
            .into_iter()
            .map(|path| {
                cce_export::relative_source_path(&path, exporter.project_root())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        match remove_stale_documents(exporter, &kept_sources).await {
            Ok(removed) => {
                if removed > 0 {
                    tracing::info!(
                        removed = removed,
                        "Removed stale NL documents after full index"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to remove stale NL documents after full index"
                );
                ctx.errors.push(format!(
                    "Failed to remove stale NL documents after full index: {error}"
                ));
                ctx.all_batches_completed = false;
            }
        }
        Ok(())
    }
    /// Activate the operation manifest (or mark it failed) once the full index
    /// either completed or aborted, then GC stale generations.
    pub(super) async fn finalize_manifest(
        &self,
        ctx: &mut FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        if ctx.all_batches_completed {
            let relation_epoch = match ctx.published_relation_epoch {
                Some(epoch) => epoch,
                // No relation generation was produced by this operation; reuse
                // the currently active one (0 when none was ever published).
                None => self.storage.active_relation_epoch().await?,
            };
            if let Err(error) = self
                .storage
                .activate_project_manifest(&ctx.operation_id, relation_epoch)
                .await
            {
                if let Err(manifest_error) = self
                    .storage
                    .fail_project_manifest(
                        &ctx.operation_id,
                        &format!("manifest activation failed: {error}"),
                    )
                    .await
                {
                    tracing::error!(
                        operation_id = %ctx.operation_id,
                        error = %manifest_error,
                        "Failed to mark project manifest failed after activation error"
                    );
                }
                return Err(error);
            }
            if let Err(error) = self
                .storage
                .publish_file_hashes(ctx.file_indexer.files())
                .await
            {
                if let Err(manifest_error) = self
                    .storage
                    .fail_project_manifest(
                        &ctx.operation_id,
                        &format!("file hash publication failed: {error}"),
                    )
                    .await
                {
                    tracing::error!(
                        operation_id = %ctx.operation_id,
                        error = %manifest_error,
                        "Failed to mark project manifest failed after file hash publication error"
                    );
                }
                return Err(error);
            }
            if let Err(error) = self.storage.gc_stale_generations().await {
                tracing::warn!(
                    operation_id = %ctx.operation_id,
                    error = %error,
                    "Generation GC after full-index publication failed"
                );
                ctx.errors.push(format!(
                    "Generation GC after full-index publication failed: {error}"
                ));
                ctx.all_batches_completed = false;
            }
        } else if let Err(error) = self
            .storage
            .fail_project_manifest(
                &ctx.operation_id,
                "one or more required index stages did not complete",
            )
            .await
        {
            tracing::warn!(operation_id = %ctx.operation_id, error = %error, "Failed to mark index manifest candidate as failed");
        }
        Ok(())
    }
}

/// Remove NL documents whose source files are no longer part of the index.
///
/// Walks the export output directory recursively and deletes every `.md`
/// document whose project-relative source path is not present in
/// `kept_sources` (e.g. documents of deleted or newly excluded files). Backup
/// directories (`.export-backup-*`) and temporary files are left untouched.
async fn remove_stale_documents(
    exporter: &NlDocumentExporter,
    kept_sources: &HashSet<String>,
) -> Result<usize, OrchestratorError> {
    let output_dir = exporter.config().output_dir();
    let mut files = Vec::new();
    collect_export_documents(&output_dir, &mut files).await?;

    let mut removed = 0usize;
    let mut failed = 0usize;
    for path in files {
        let Some(rel) = path.strip_prefix(&output_dir).ok() else {
            continue;
        };
        // Output naming is `<relative_source_path>.md`; stripping the final
        // `.md` extension recovers the source path (e.g. `src/a.rs.md` ->
        // `src/a.rs`).
        let source_rel = rel.with_extension("").to_string_lossy().into_owned();
        if kept_sources.contains(&source_rel) {
            continue;
        }
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {
                removed += 1;
            }
            Err(error) => {
                failed += 1;
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "Failed to remove stale NL document"
                );
            }
        }
    }
    if failed > 0 {
        return Err(OrchestratorError::index(
            "export_stale_cleanup",
            format!("failed to remove {failed} stale NL documents"),
        ));
    }
    Ok(removed)
}

/// Recursively collect `.md` export documents under `dir`, skipping backup
/// directories left over from the hot-update export lifecycle.
///
/// Uses an explicit stack to avoid recursive `async fn` futures.
async fn collect_export_documents(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), OrchestratorError> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&current).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(OrchestratorError::index(
                    "nl_document_cleanup",
                    format!(
                        "Failed to read export output dir {}: {error}",
                        current.display()
                    ),
                ));
            }
        };
        while let Some(entry) = entries.next_entry().await.map_err(|error| {
            OrchestratorError::index(
                "nl_document_cleanup",
                format!("Failed to read export dir entry: {error}"),
            )
        })? {
            let path = entry.path();
            let is_backup_dir = path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with(".export-backup-"))
                .unwrap_or(false);
            if path.is_dir() {
                if !is_backup_dir {
                    stack.push(path);
                }
            } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    Ok(())
}
