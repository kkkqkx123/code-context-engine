use super::IndexBuilder;
use crate::index::{ExportIndexOps, ImportIndexOps, RelationIndexView, RelationQueryOps};
use crate::symbol_table::ProjectSymbolTable;
use cce_types::{Entity, ParsedFile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl IndexBuilder {
    /// Add multiple parsed files to the index
    ///
    /// This is the primary entry point for the new architecture.
    pub fn add_parsed_files(&self, files: &[&ParsedFile]) -> &Self {
        let start = std::time::Instant::now();

        // 1. Build project symbol table from all files
        let mut symbol_builder = super::symbol_table::SymbolTableBuilder::new(PathBuf::from("."));
        symbol_builder.with_metrics(self.metrics.clone());
        let project_symbols = symbol_builder.build(files);

        // 2. Create file processor
        let processor = self.make_file_processor(&self.config);
        let resolver = processor.create_resolver();

        // 3. First index file-local data and entities for all files.
        for file in files {
            processor.index_file_core(file);
        }

        // 3.5 Scan all files' SymbolKeys to detect conflicts (same name, different entities)
        // This is done before relation resolution to ensure stable resolution
        self.scan_symbol_key_conflicts(files);

        // 4. Resolve relations after the full entity set is present.
        for file in files {
            processor.process_relations(file, &project_symbols, &resolver);
        }

        // Record metrics if enabled
        if let Some(metrics) = &self.metrics {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let extracted_count = self.index.resolved_relation_count();
            let file_count = files.len();

            metrics.record_build(elapsed_ms, extracted_count, file_count);

            tracing::debug!("Relation building completed with metrics");
        }

        self
    }

    /// Add a single parsed file to the index
    ///
    /// For single-file processing without cross-file resolution.
    pub fn add_parsed_file(&self, parsed: &ParsedFile) -> &Self {
        // Use empty project symbols for single-file mode
        let project_symbols = ProjectSymbolTable::new(std::path::PathBuf::from("."));
        let processor = self.make_file_processor(&self.config);
        let resolver = processor.create_resolver();
        processor.index_file_core(parsed);
        processor.process_relations(parsed, &project_symbols, &resolver);
        self
    }

    /// Scan all files' SymbolKeys to detect conflicts (same name, different entities)
    /// This is done before relation resolution to ensure stable resolution
    fn scan_symbol_key_conflicts(&self, files: &[&ParsedFile]) {
        use crate::index::core::SymbolKey;
        use std::collections::HashMap;

        let _ = files;

        // Group SymbolKeys by their scoped name (ignoring file path)
        let mut name_to_keys: HashMap<String, Vec<(SymbolKey, cce_types::EntityId)>> =
            HashMap::new();

        // Collect all SymbolKeys from the index
        let all_keys = self.index.stable_symbol_keys();

        for key in all_keys {
            if let Some(entity_id) = self.index.get_entity_id_by_symbol_key(&key) {
                name_to_keys
                    .entry(key.scoped_name.clone())
                    .or_default()
                    .push((key, entity_id));
            }
        }

        // Detect conflicts: same scoped name but different entities
        for (name, keys) in &name_to_keys {
            if keys.len() > 1 {
                // Check if there are different entities
                let first_entity = &keys[0].1;
                let has_conflict = keys.iter().any(|(_, eid)| eid != first_entity);

                if has_conflict {
                    tracing::warn!(
                        symbol_name = %name,
                        count = keys.len(),
                        "detected conflicting symbol keys with different entities"
                    );

                    // Record the conflict in diagnostics
                    if let Some(metrics) = &self.metrics {
                        metrics.symbol_key_conflicts.increment();
                    }

                    // Log details of the conflict
                    for (key, entity_id) in keys {
                        tracing::debug!(
                            file = %key.file_path,
                            kind = ?key.kind,
                            entity_id = entity_id.0,
                            "conflicting symbol key entry"
                        );
                    }
                }
            }
        }
    }

    /// Add multiple parsed files using a pre-built project symbol table.
    ///
    /// Unlike `add_parsed_files` which builds a per-batch symbol table internally,
    /// this method accepts an externally-prepared `ProjectSymbolTable` that may
    /// contain symbols from files outside the current batch. This enables correct
    /// cross-batch relation resolution during full index.
    ///
    /// The caller is responsible for ensuring the symbol table contains at least
    /// the symbols for all files in `files`. The table is used read-only for
    /// relation resolution and is NOT modified by this method.
    pub fn add_parsed_files_with_symbols(
        &self,
        files: &[&ParsedFile],
        project_symbols: &ProjectSymbolTable,
    ) -> &Self {
        let start = std::time::Instant::now();

        let processor = self.make_file_processor(&self.config);
        let resolver = processor.create_resolver();

        for file in files {
            processor.index_file_core(file);
        }

        for file in files {
            processor.process_relations(file, project_symbols, &resolver);
        }

        if let Some(metrics) = &self.metrics {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let extracted_count = self.index.resolved_relation_count();
            let file_count = files.len();
            metrics.record_build(elapsed_ms, extracted_count, file_count);
        }

        self
    }

    /// Add multiple parsed files with pre-populated symbols from an existing index.
    ///
    /// Unlike `add_parsed_files` which builds a per-batch symbol table containing only
    /// the batch files' symbols, this method also pre-populates the symbol table with
    /// all symbols from the `existing_index`. This ensures correct cross-file relation
    /// resolution during hot updates, where unchanged files' symbols must be available
    /// for resolving relations in changed files.
    ///
    /// The pre-population mirrors the full-build symbol construction
    /// (`SymbolTableBuilder::add_file_to_project`) as closely as the index data
    /// allows: unchanged files receive a `ModuleSymbolTable` with their exported
    /// entities and flat `{file_path}::{entity.name}` entries, so scope-chain and
    /// module-level resolution semantics match a full build.
    ///
    /// When `scope_files` is `Some`, only files within the given set are
    /// pre-populated. This limits the cost to the dependency propagation closure
    /// instead of the full project. When `None`, all files from the base view
    /// are pre-populated (full-build compatible).
    pub fn add_parsed_files_with_index_symbols(
        &self,
        files: &[&ParsedFile],
        existing: &impl RelationIndexView,
        scope_files: Option<&std::collections::HashSet<String>>,
    ) -> &Self {
        let start = std::time::Instant::now();

        let mut symbol_builder = super::symbol_table::SymbolTableBuilder::new(PathBuf::from("."));
        symbol_builder.with_metrics(self.metrics.clone());
        let project_symbols = symbol_builder.build(files);

        // Prepopulate symbol context from the base snapshot for files that are
        // NOT part of this candidate. Affected files are skipped: their
        // entities are re-parsed by `index_file_core` below and, like the
        // full-clone baseline (where `remove_file` drops them first), must not
        // contribute stale symbols or shift symbol-id allocation.
        let skip_files: std::collections::HashSet<String> =
            files.iter().map(|f| f.path.clone()).collect();
        let entities_by_file = existing.entities_by_file();
        self.prepopulate_index_symbols(
            &project_symbols,
            existing,
            &entities_by_file,
            &skip_files,
            scope_files,
        );

        // Candidate variables may depend on return types from existing files
        // that were just inserted into the propagator.
        for file in files {
            project_symbols.propagate_cross_file_variables_for_file(file);
        }

        let processor = self.make_file_processor(&self.config);
        let resolver = processor.create_resolver();

        for file in files {
            processor.index_file_core(file);
        }

        if self.plugin_symbols_enabled && self.plugin_registry.is_some() {
            for file in files {
                self.register_file_plugin_symbols(file, &project_symbols);
            }
            for file in files {
                self.inject_plugin_relations(file, &project_symbols);
            }
        }

        for file in files {
            processor.process_relations(file, &project_symbols, &resolver);
        }

        if let Some(metrics) = &self.metrics {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let extracted_count = self.index.resolved_relation_count();
            let file_count = files.len();
            metrics.record_build(elapsed_ms, extracted_count, file_count);
        }

        self
    }

    /// Reconstruct symbol-table state for files that are NOT part of the
    /// current candidate, mirroring `SymbolTableBuilder::add_file_to_project`
    /// using index data (entities, spans, modifiers, exports, language).
    ///
    /// This keeps hot-update resolution semantics aligned with the full build:
    /// module tables (export lookup, cross-package resolution) and the flat
    /// `{file_path}::{entity.name}` index used by the resolver fallback are
    /// both populated with the same keys the full build would produce.
    ///
    /// When `scope_files` is `Some`, only files within the scope (and not in
    /// `skip_files`) are pre-populated. This bounds the cost to the dependency
    /// propagation closure instead of the full project.
    fn prepopulate_index_symbols(
        &self,
        project_symbols: &ProjectSymbolTable,
        existing: &impl crate::index::RelationIndexView,
        entities_by_file: &HashMap<String, Vec<Entity>>,
        skip_files: &std::collections::HashSet<String>,
        scope_files: Option<&std::collections::HashSet<String>>,
    ) {
        use crate::symbol::{SymbolLocation, SymbolMetadata};
        use crate::symbol_table::{ModuleSymbolTable, PackageSymbolTable};
        use cce_parser_core::determine_module_path;
        use cce_types::language::LanguageInfo;
        use rayon::prelude::*;

        let package_name = Path::new(".")
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());
        let mut symbol_builder = super::symbol_table::SymbolTableBuilder::new(PathBuf::from("."));
        symbol_builder.with_metrics(self.metrics.clone());

        // Iterate deterministically (sorted by path) so symbol-id allocation
        // and the resulting `RelationSymbolRecord` values match the full-clone
        // baseline regardless of HashMap iteration order.
        let mut file_paths: Vec<&String> = entities_by_file
            .keys()
            .filter(|path| {
                if skip_files.contains(*path) {
                    return false;
                }
                // When scope is provided, only pre-populate files within the
                // propagation closure. This bounds the O(project) prefill to
                // the change-size proportional set.
                if let Some(scope) = scope_files {
                    scope.contains(*path)
                } else {
                    true
                }
            })
            .collect();
        file_paths.sort();

        // For large scopes, precompute pure per-file module tables in parallel
        // (CPU-bound, no shared mutation), then apply shared state serially.
        // This keeps the hot-update path O(affected) and avoids DashMap
        // contention during cache invalidation.
        let use_parallel = file_paths.len() > 64;
        type PrecomputedEntry = (
            String,
            ModuleSymbolTable,
            Vec<crate::index::core::ExportInfo>,
            Vec<Entity>,
            cce_types::language::Language,
        );
        let precomputed: Vec<PrecomputedEntry> = if use_parallel {
            file_paths
                .par_iter()
                .map(|file_path| {
                    let entities = &entities_by_file[*file_path];
                    let language = LanguageInfo::detect_from_path(file_path).language;
                    let module_path =
                        determine_module_path(Path::new(file_path), Path::new("."), language)
                            .unwrap_or_default();
                    let mut module_table = ModuleSymbolTable::new(
                        module_path,
                        file_path.to_string(),
                        language,
                        package_name.clone(),
                    );
                    for entity in entities.iter() {
                        if symbol_builder.is_entity_exported(entity, language) {
                            let location =
                                SymbolLocation::new(file_path.to_string(), entity.span, language);
                            let metadata =
                                SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                            let visibility = symbol_builder.detect_visibility(entity, language);
                            module_table.add_export(entity.name.clone(), metadata, visibility);
                        }
                    }
                    let exports =
                        crate::helpers::extract_exports_from_entities(entities, &language);
                    for export in &exports {
                        if !module_table.has_export(&export.function_name) {
                            if let Some(entity) =
                                entities.iter().find(|e| e.id == export.function_id)
                            {
                                let location = SymbolLocation::new(
                                    file_path.to_string(),
                                    entity.span,
                                    language,
                                );
                                let metadata =
                                    SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                                let visibility = symbol_builder.detect_visibility(entity, language);
                                module_table.add_export(
                                    export.function_name.clone(),
                                    metadata,
                                    visibility,
                                );
                            }
                        }
                    }
                    (
                        file_path.to_string(),
                        module_table,
                        exports,
                        entities.clone(),
                        language,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        if use_parallel {
            // Group precomputed entries by package for parallel processing
            let mut by_package: std::collections::HashMap<String, Vec<PrecomputedEntry>> =
                std::collections::HashMap::new();
            for entry in precomputed {
                let file_path = &entry.0;
                let language = entry.4;
                let module_path =
                    determine_module_path(Path::new(file_path), Path::new("."), language)
                        .unwrap_or_default();
                let pkg_name = if module_path.is_empty() {
                    package_name.clone()
                } else {
                    module_path
                        .split("::")
                        .next()
                        .unwrap_or(&package_name)
                        .to_string()
                };
                by_package.entry(pkg_name).or_default().push(entry);
            }

            // Process each package in parallel (packages are independent)
            for (pkg_name, entries) in by_package {
                let package = if let Some(pkg) = project_symbols.get_package(&pkg_name) {
                    pkg
                } else {
                    Arc::new(PackageSymbolTable::new(
                        pkg_name.clone(),
                        pkg_name.clone(),
                        ".".to_string(),
                        LanguageInfo::detect_from_path(&entries[0].0).language,
                    ))
                };

                // Process all files in this package sequentially (they share package state)
                for (file_path, module_table, exports, entities, language) in entries {
                    let delta = package.add_module_incremental(module_table);
                    if delta.path_collision {
                        if let Some(metrics) = &self.metrics {
                            metrics.module_path_conflicts.increment();
                        }
                    }
                    project_symbols.apply_package_delta(Arc::clone(&package), &delta);

                    let normalized = cce_types::normalize_project_path(&file_path);
                    if let Some(import_table) = existing.imports_of(&file_path) {
                        self.index
                            .add_import_table(normalized.clone(), import_table);
                    }
                    if let Some(export_list) = existing.exports_of(&file_path) {
                        if !export_list.is_empty() {
                            self.index.add_exports(normalized.clone(), export_list);
                        }
                    }

                    for entity in &entities {
                        if symbol_builder.is_entity_exported(entity, language) {
                            let qualified_name = format!("{}::{}", file_path, entity.name);
                            project_symbols.insert_symbol(
                                qualified_name,
                                entity.id,
                                file_path.clone(),
                                file_path.clone(),
                            );
                        }
                    }
                    for export in &exports {
                        let qualified_name = format!("{}::{}", file_path, export.function_name);
                        project_symbols.insert_symbol(
                            qualified_name,
                            export.function_id,
                            file_path.clone(),
                            file_path.clone(),
                        );
                    }

                    for entity in &entities {
                        self.index.register_external_entity_name(
                            &entity.name,
                            entity.id,
                            &file_path,
                        );
                    }

                    {
                        use crate::type_inference::{ScopedTypeContext, TypeBinding};
                        let mut tmp_ctx = ScopedTypeContext::new(language);
                        for entity in &entities {
                            if let Some(rt) = &entity.return_type {
                                let binding = TypeBinding {
                                    type_name: rt.clone(),
                                    type_entity_id: None,
                                    span: entity.span,
                                    origin: None,
                                    shape: None,
                                };
                                tmp_ctx.add_return_type(entity.id, binding);
                            }
                        }
                        if !tmp_ctx.is_empty() {
                            project_symbols
                                .cross_file_propagator()
                                .insert_file(&file_path, &tmp_ctx, &entities);
                            project_symbols.set_type_inference_context(&file_path, tmp_ctx);
                        }
                    }

                    // Consistency verification: ensure prepopulated file's import/export
                    // state matches the base view. Missing tables would silently break
                    // cross-file resolution for hot-updated dependents.
                    if existing.imports_of(&file_path).is_none() {
                        tracing::trace!("prepopulate: no import table for {}", file_path);
                    }
                    if existing.exports_of(&file_path).is_none() {
                        tracing::trace!("prepopulate: no export list for {}", file_path);
                    }
                }
            }
        } else {
            for file_path in file_paths {
                let entities = &entities_by_file[file_path];
                let language = LanguageInfo::detect_from_path(file_path).language;
                let module_path =
                    determine_module_path(Path::new(file_path), Path::new("."), language)
                        .unwrap_or_default();

                let mut module_table = ModuleSymbolTable::new(
                    module_path,
                    file_path.clone(),
                    language,
                    package_name.clone(),
                );

                for entity in entities {
                    if symbol_builder.is_entity_exported(entity, language) {
                        let location =
                            SymbolLocation::new(file_path.clone(), entity.span, language);
                        let metadata =
                            SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                        let visibility = symbol_builder.detect_visibility(entity, language);
                        module_table.add_export(entity.name.clone(), metadata, visibility);
                    }
                }

                let exports = crate::helpers::extract_exports_from_entities(entities, &language);
                for export in &exports {
                    if !module_table.has_export(&export.function_name) {
                        if let Some(entity) = entities.iter().find(|e| e.id == export.function_id) {
                            let location =
                                SymbolLocation::new(file_path.clone(), entity.span, language);
                            let metadata =
                                SymbolMetadata::new(entity.name.clone(), entity.kind, location);
                            let visibility = symbol_builder.detect_visibility(entity, language);
                            module_table.add_export(
                                export.function_name.clone(),
                                metadata,
                                visibility,
                            );
                        }
                    }
                }

                let package = if let Some(pkg) = project_symbols.get_package(&package_name) {
                    pkg
                } else {
                    Arc::new(PackageSymbolTable::new(
                        package_name.clone(),
                        package_name.clone(),
                        ".".to_string(),
                        language,
                    ))
                };
                let delta = package.add_module_incremental(module_table);
                if delta.path_collision {
                    if let Some(metrics) = &self.metrics {
                        metrics.module_path_conflicts.increment();
                    }
                }
                project_symbols.apply_package_delta(package, &delta);

                let normalized = cce_types::normalize_project_path(file_path);
                if let Some(import_table) = existing.imports_of(file_path) {
                    self.index
                        .add_import_table(normalized.clone(), import_table);
                }
                if let Some(export_list) = existing.exports_of(file_path) {
                    if !export_list.is_empty() {
                        self.index.add_exports(normalized.clone(), export_list);
                    }
                }

                for entity in entities {
                    if symbol_builder.is_entity_exported(entity, language) {
                        let qualified_name = format!("{}::{}", file_path, entity.name);
                        project_symbols.insert_symbol(
                            qualified_name,
                            entity.id,
                            file_path.clone(),
                            file_path.clone(),
                        );
                    }
                }
                for export in &exports {
                    let qualified_name = format!("{}::{}", file_path, export.function_name);
                    project_symbols.insert_symbol(
                        qualified_name,
                        export.function_id,
                        file_path.clone(),
                        file_path.clone(),
                    );
                }

                for entity in entities {
                    self.index
                        .register_external_entity_name(&entity.name, entity.id, file_path);
                }

                {
                    use crate::type_inference::{ScopedTypeContext, TypeBinding};
                    let mut tmp_ctx = ScopedTypeContext::new(language);
                    for entity in entities {
                        if let Some(rt) = &entity.return_type {
                            let binding = TypeBinding {
                                type_name: rt.clone(),
                                type_entity_id: None,
                                span: entity.span,
                                origin: None,
                                shape: None,
                            };
                            tmp_ctx.add_return_type(entity.id, binding);
                        }
                    }
                    if !tmp_ctx.is_empty() {
                        project_symbols
                            .cross_file_propagator()
                            .insert_file(file_path, &tmp_ctx, entities);
                        project_symbols.set_type_inference_context(file_path, tmp_ctx);
                    }
                }

                if existing.imports_of(file_path).is_none() {
                    tracing::trace!("prepopulate: no import table for {}", file_path);
                }
                if existing.exports_of(file_path).is_none() {
                    tracing::trace!("prepopulate: no export list for {}", file_path);
                }
            }
        }
    }

    /// Add multiple parsed files with explicit external package classification
    pub fn add_parsed_files_with_classification(
        &self,
        files: &[&ParsedFile],
        external_packages: HashMap<
            cce_types::language::Language,
            std::collections::HashSet<String>,
        >,
    ) -> &Self {
        // Build project symbol table from all files
        let mut symbol_builder = super::symbol_table::SymbolTableBuilder::new(PathBuf::from("."));
        symbol_builder.with_metrics(self.metrics.clone());
        let project_symbols = symbol_builder.build(files);

        // Create temporary config with external packages (batch set)
        let mut temp_config = self.config.clone();
        temp_config.set_all_external_packages(external_packages);

        // Create processor with temporary config
        let processor = self.make_file_processor(&temp_config);
        let resolver = processor.create_resolver();

        // First index file-local data and entities for all files.
        for file in files {
            processor.index_file_core(file);
        }

        // Then resolve relations with the full entity set available.
        for file in files {
            processor.process_relations(file, &project_symbols, &resolver);
        }

        self
    }
}
