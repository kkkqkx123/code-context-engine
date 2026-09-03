//! Post-batch finalization for full index runs.
//!
//! After every batch succeeds, the relation graph is rebuilt once against the
//! complete project symbol snapshot, published as a canonical snapshot, NL
//! documents are exported (with stale-document cleanup), and the operation
//! manifest is activated.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::export::NlDocumentExporter;
use cce_relation::{
    ThreadSafeIndex,
    index::{RelationIndexView, RelationQueryOps},
};

use super::{FullIndexContext, IndexOrchestrator};
use crate::error::OrchestratorError;
use crate::index_state::{ModuleType, ModuleUpdateState};

impl IndexOrchestrator {
    /// Finalize index after batch processing completes.
    ///
    /// Performs post-processing steps that depend on complete data:
    /// 1. Resolve relation counts (requires all files to be indexed)
    /// 2. Wire up relation enhancement for NL document export
    pub(super) fn finalize_index(&self) -> usize {
        // Step 1: Finalize relation index (must happen after all files are added)
        let total_relations = if let Some(ref builder) = self.relation_builder {
            tracing::info!("Finalizing relation index...");
            let count = builder.index().resolved_relation_count();
            tracing::info!("Relation index finalized with {} relations", count);
            count
        } else {
            0
        };

        // Step 2: Set up relation enhancement for export (requires relation index to be final)
        self.setup_export_relation_enhancement();

        total_relations
    }

    /// Wire up relation enhancement for NL document export.
    ///
    /// Called automatically by `finalize_index()` after relation data is fully resolved.
    fn setup_export_relation_enhancement(&self) {
        if let Some(ref exporter) = self.nl_exporter {
            if exporter.config().enable_relation_enhancement {
                if let Some(relation_index) = self.get_relation_index() {
                    let enhancer_config = crate::export::RelationEnhancerConfig::default();
                    tracing::info!("Setting up relation enhancement for NL document export");
                    exporter.set_relation_enhancement(Arc::new(relation_index), enhancer_config);
                }
            }
        }
    }

    /// Get the relation index (thread-safe)
    pub fn get_relation_index(&self) -> Option<ThreadSafeIndex> {
        self.relation_builder.as_ref().map(|b| b.index().clone())
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
    /// index.
    pub(super) async fn build_and_publish_relations(
        &self,
        ctx: &mut FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        let builder = self.relation_builder.as_ref().ok_or_else(|| {
            OrchestratorError::index("relation_build", "relation builder is unavailable")
        })?;
        let spool = ctx.relation_spool.as_mut().ok_or_else(|| {
            OrchestratorError::index("relation_build_spool", "relation spool is unavailable")
        })?;
        let relation_build_started = Instant::now();
        builder.clear();

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
                        tracing::error!(
                            error = %e,
                            root = %project_root.display(),
                            "Failed to generate synthetic config files: spawn_blocking task failed"
                        );
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
                builder.add_file_symbols(synthetic, spool.project_symbols());
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
        const MEMORY_CACHE_LIMIT_BYTES: u64 = 256 * 1024 * 1024;
        let use_memory_cache = spool.estimated_decoded_bytes() < MEMORY_CACHE_LIMIT_BYTES
            && spool.entry_count() < 100_000;
        let (registered_files, resolved_files) = if use_memory_cache {
            tracing::info!(
                spool_entries = spool.entry_count(),
                total_bytes,
                estimated_decoded = spool.estimated_decoded_bytes(),
                "Using in-memory cache for relation spool replay (single disk read)"
            );
            let t_collect = Instant::now();
            let cached_files = spool.collect_all().map_err(|error| {
                OrchestratorError::index(
                    "relation_build_spool",
                    format!("Failed to collect spool for memory-cache replay: {error}"),
                )
            })?;
            record_pass(t_collect.elapsed(), total_bytes);
            let t0 = Instant::now();
            for parsed in &cached_files {
                builder.register_file_entities(parsed);
                if plugin_enabled {
                    builder.register_file_plugin_symbols(parsed, spool.project_symbols());
                }
            }
            let registered = cached_files.len();
            record_pass(t0.elapsed(), total_bytes);
            if let Some(parser) = &self.cached_build_config {
                Self::inject_governance_edges(builder, parser);
            }
            let t1 = Instant::now();
            for parsed in &cached_files {
                builder.resolve_file_relations(parsed, spool.project_symbols());
                if plugin_enabled {
                    builder.inject_plugin_relations(parsed, spool.project_symbols());
                }
            }
            record_pass(t1.elapsed(), total_bytes);
            (registered, cached_files.len())
        } else {
            // Disk-backed two-pass replay: registration and resolution each
            // scan the spool from disk. Required when the estimated decoded
            // size exceeds the memory limit.
            let t0 = Instant::now();
            let registered = spool
                .for_each(|parsed| {
                    builder.register_file_entities(parsed);
                    if plugin_enabled {
                        builder.register_file_plugin_symbols(parsed, spool.project_symbols());
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
                Self::inject_governance_edges(builder, parser);
            }
            let t1 = Instant::now();
            let resolved = spool
                .for_each(|parsed| {
                    builder.resolve_file_relations(parsed, spool.project_symbols());
                    if plugin_enabled {
                        builder.inject_plugin_relations(parsed, spool.project_symbols());
                    }
                })
                .map_err(|error| {
                    OrchestratorError::index(
                        "relation_build_spool",
                        format!("Failed to replay relation edges: {error}"),
                    )
                })?;
            record_pass(t1.elapsed(), total_bytes);
            (registered, resolved)
        };

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
        builder.record_streamed_build(relation_build_started.elapsed(), registered_files);

        match builder
            .index()
            .to_canonical_snapshot(builder.config_fingerprint())
        {
            Ok(snapshot) => match &self.relation_publisher {
                Some(publisher) => {
                    match publisher
                        .publish(
                            self.project_id,
                            &ctx.operation_id,
                            snapshot,
                            builder.index(),
                        )
                        .await
                    {
                        Ok(publication) => {
                            ctx.published_relation_epoch = Some(publication.relation_epoch);
                            // the relation phase is only marked complete
                            // (and files successful) AFTER the snapshot was
                            // actually built and published. The previous
                            // per-batch marking ran before the build and could
                            // report success for an operation whose relation
                            // phase was skipped entirely (batch failure).
                            let mut relation_paths: Vec<std::path::PathBuf> = Vec::new();
                            if let Err(error) = spool.for_each(|parsed| {
                                relation_paths.push(std::path::PathBuf::from(parsed.path.clone()));
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
                    ctx.errors.push(
                        "Relation indexing requires a configured snapshot publisher".to_string(),
                    );
                    ctx.all_batches_completed = false;
                }
            },
            Err(error) => {
                ctx.errors.push(format!(
                    "Failed to create canonical relation snapshot: {error}"
                ));
                ctx.all_batches_completed = false;
            }
        }
        Ok(())
    }

    fn inject_governance_edges(
        builder: &cce_relation::IndexBuilder,
        parser: &cce_relation::BuildConfigParser,
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
        let index = builder.index();
        index.for_each_import(|path, table| {
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
                builder.dependency_graph().add_dependency(path, &cfg);
                let cfg_entities = index.entities_of_file(&cfg);
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
                    };
                    index.add_file_relation(path, rel);
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
            let chunks = spool.load_chunks(&file_path)?;
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
                .map(crate::export::ExportSummaryView::from);
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
                            e.to_string(),
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
                crate::export::path_utils::relative_source_path(&path, exporter.project_root())
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
            }
        }
        Ok(())
    }
    /// Activate the operation manifest (or mark it failed) once the full index
    /// either completed or aborted, then GC stale generations.
    pub(super) async fn finalize_manifest(
        &self,
        ctx: &FullIndexContext,
    ) -> Result<(), OrchestratorError> {
        if ctx.all_batches_completed {
            let relation_epoch = match ctx.published_relation_epoch {
                Some(epoch) => epoch,
                // No relation generation was produced by this operation; reuse
                // the currently active one (0 when none was ever published).
                None => self.storage.active_relation_epoch()?,
            };
            if let Err(error) = self
                .storage
                .activate_project_manifest(&ctx.operation_id, relation_epoch)
            {
                let _ = self.storage.fail_project_manifest(
                    &ctx.operation_id,
                    &format!("manifest activation failed: {error}"),
                );
                return Err(error);
            }
            if let Err(error) = self.storage.publish_file_hashes(ctx.file_indexer.files()) {
                let _ = self.storage.fail_project_manifest(
                    &ctx.operation_id,
                    &format!("file hash publication failed: {error}"),
                );
                return Err(error);
            }
            if let Err(error) = self.storage.gc_stale_generations().await {
                tracing::warn!(
                    operation_id = %ctx.operation_id,
                    error = %error,
                    "Generation GC after full-index publication failed"
                );
            }
        } else if let Err(error) = self.storage.fail_project_manifest(
            &ctx.operation_id,
            "one or more required index stages did not complete",
        ) {
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
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "Failed to remove stale NL document"
                );
            }
        }
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
