//! File processor for parsed file handling
//!
//! Processes parsed files and adds them to the relation index.

use super::config::BuilderConfig;
use super::macro_body_call_extractor::extract_macro_body_calls;
use crate::dependency_graph::FileDependencyGraph;
use crate::index::core::{ExportInfo, RelationIndex};
use crate::index::resolver::RelationResolver;
use crate::index::{
    EntityIndexOps, ExportIndexOps, FileIndexOps, ImportIndexOps, LocalCallResolver,
    LocalCallResolverConfig,
};
use crate::symbol_table::ProjectSymbolTable;
use cce_metrics::domain::pipeline::RelationMetrics;
use cce_parser_core::{AstParser, set_language_resolver};
use cce_plugin::PluginRegistry;
use cce_types::relation::CallContext;
use cce_types::{
    Entity, EntityId, FileInfo, ImportTable, ParsedFile, RawRelationData, RelationLevel,
    RelationType, ResolvedRelation, normalize_project_path,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// File processor for handling parsed files
pub struct FileProcessor<'a> {
    index: &'a RelationIndex,
    config: &'a BuilderConfig,
    dependency_graph: &'a Arc<FileDependencyGraph>,
    plugin_registry: Option<&'a PluginRegistry>,
    metrics: Option<Arc<RelationMetrics>>,
    /// Shared lazy AST parser for the fallback import-extraction path
    /// The tree-sitter grammar initialization is expensive, so a single
    /// instance is reused across all files instead of being rebuilt per file.
    ast_parser: Option<&'a Mutex<AstParser>>,
}

impl<'a> FileProcessor<'a> {
    /// Create a new file processor with an optional plugin registry.
    pub fn with_registry(
        index: &'a RelationIndex,
        config: &'a BuilderConfig,
        dependency_graph: &'a Arc<FileDependencyGraph>,
        plugin_registry: Option<&'a PluginRegistry>,
    ) -> Self {
        Self {
            index,
            config,
            dependency_graph,
            plugin_registry,
            metrics: None,
            ast_parser: None,
        }
    }

    /// Attach relation metrics for quality accounting.
    pub fn with_metrics(&mut self, metrics: Option<Arc<RelationMetrics>>) -> &mut Self {
        self.metrics = metrics;
        self
    }

    /// Share the build-wide lazy AST parser with this processor so the
    /// fallback import-extraction path reuses one grammar initialization
    /// instead of creating a new `AstParser` per file.
    pub fn with_ast_parser(&mut self, parser: &'a Mutex<AstParser>) -> &mut Self {
        self.ast_parser = Some(parser);
        self
    }

    fn record_relation_key_conflict(&self) {
        if let Some(metrics) = &self.metrics {
            metrics.symbol_key_conflicts.increment();
        }
    }

    /// Index file metadata, imports, exports, and function entities.
    pub fn index_file_core(&self, file: &ParsedFile) {
        if file.import_table.is_none() && self.config.policy.analyze_imports {
            let is_custom_fallback = self.config.policy.symbol_extract_enabled
                && matches!(file.language, cce_types::language::Language::Custom(_));
            if !is_custom_fallback {
                tracing::warn!(
                    file = %file.path,
                    "import_table missing; falling back to tree-sitter parse"
                );
                if let Some(metrics) = &self.metrics {
                    metrics.import_fallback_total.increment();
                }
            } else {
                tracing::debug!(
                    file = %file.path,
                    "import_table missing for custom language; using SymbolExtract plugin"
                );
            }
            if self.config.policy.require_import_table {
                tracing::error!(
                    file = %file.path,
                    "require_import_table is enabled but import_table is missing"
                );
            }
        }
        // Extract imports/exports/dependencies from raw_relations and entities.
        // The parse stage already extracts imports (ParseCoordinator fills
        // `file.import_table` while holding the AST), so the cached branch
        // below avoids a second tree-sitter parse during the relation build.
        let imports: ImportTable = if !self.config.policy.analyze_imports {
            Default::default()
        } else if let Some(ref cached) = file.import_table {
            cached.clone()
        } else if self.config.policy.require_import_table {
            Default::default()
        } else {
            self.extract_imports_fallback(file)
        };

        let exports: Vec<ExportInfo> = if self.config.policy.analyze_imports {
            crate::helpers::extract_exports_from_entities(&file.entities, &file.language)
        } else {
            Vec::new()
        };

        let dependencies: Vec<String> = if self.config.policy.track_cross_file_deps {
            crate::helpers::extract_dependencies_from_imports(&imports)
        } else {
            Vec::new()
        };

        // 1. Add file info
        let normalized_path = normalize_project_path(&file.path);
        let file_info = FileInfo {
            id: normalized_path.clone(),
            path: normalized_path.clone(),
            language: file.language.to_string(),
            // The parse stage computes the full-content hash once and carries
            // it on `ParsedFile`; only files parsed outside that pipeline
            // (e.g. test fixtures) recompute it here.
            file_hash: file
                .file_hash
                .clone()
                .unwrap_or_else(|| format!("{:x}", Sha256::digest(file.source.as_bytes()))),
            file_size: file.source.len() as u64,
            modified_time: 0,
            parse_status: cce_types::entity::ParseStatus::Success,
            parse_errors: Vec::new(),
            parse_version: 0,
            entity_count: file.entities.len(),
            relation_count: file.raw_relations.len(),
            export_count: exports.len(),
            import_count: imports.import_count(),
            depends_on: dependencies.clone(),
        };
        self.index.add_file(file_info);

        // 1.5 Record file dependencies in the dependency graph
        if self.config.policy.track_cross_file_deps {
            for dependency_path in &dependencies {
                self.dependency_graph
                    .add_dependency(&normalized_path, &normalize_project_path(dependency_path));
            }
        }

        // 2. Add relation-addressable entities with globally unique IDs.
        // ParsedFile-local EntityIds are remapped to process-unique IDs
        // to prevent collisions when multiple files share overlapping local IDs
        // (e.g. during concurrent hot-update reparsing).
        let mut remap: HashMap<EntityId, EntityId> = HashMap::with_capacity(file.entities.len());
        for entity in &file.entities {
            let new_id = self.index.allocate_entity_id();
            remap.insert(entity.id, new_id);
        }
        let remapped_entities: Vec<(EntityId, cce_types::Entity, String)> = file
            .entities
            .iter()
            .map(|e| {
                let new_id = remap
                    .get(&e.id)
                    .copied()
                    .expect("remap must contain entity id");
                let mut entity = e.clone();
                entity.id = new_id;
                if let Some(pid) = entity.parent {
                    entity.parent = remap.get(&pid).copied();
                }
                entity.children = entity
                    .children
                    .iter()
                    .filter_map(|cid| remap.get(cid).copied())
                    .collect();
                (new_id, entity, normalized_path.clone())
            })
            .collect();
        self.index.add_functions_with_paths(remapped_entities);

        let scoped_names = file.resolve_all_scoped_names();
        for entity in &file.entities {
            let new_id = remap.get(&entity.id).copied().unwrap_or(entity.id);
            if self.index.function_index().contains_key(&new_id)
                && let Some(scoped_name) = scoped_names.get(&entity.id)
            {
                let mut temp_entity = entity.clone();
                temp_entity.id = new_id;
                let registered = self.index.register_symbol_key(
                    &normalized_path,
                    scoped_name,
                    &temp_entity,
                    new_id,
                );
                if !registered {
                    self.record_relation_key_conflict();
                }
            }
        }

        self.index
            .entity_id_remaps
            .write()
            .insert(normalized_path.clone(), remap);

        // 3. Add imports
        self.index
            .add_import_table(normalized_path.clone(), imports);

        // 4. Add exports
        if !exports.is_empty() {
            self.index.add_exports(normalized_path, exports);
        }
    }

    /// Resolve and store relations for a file.
    pub fn process_relations(
        &self,
        file: &ParsedFile,
        project_symbols: &ProjectSymbolTable,
        resolver: &RelationResolver,
    ) {
        // Retrieve the entity ID remap table built by index_file_core.
        let normalized_path = normalize_project_path(&file.path);
        let file_remap = self
            .index
            .entity_id_remaps
            .read()
            .get(&normalized_path)
            .cloned();
        // Helper: map a ParsedFile-local ID to its global ID, or pass through
        // unchanged if the ID was already global (i.e. belongs to an unchanged file).
        let remap_id = |id: EntityId| -> EntityId {
            file_remap
                .as_ref()
                .and_then(|m| m.get(&id))
                .copied()
                .unwrap_or(id)
        };
        // Remap IDs in a resolved relation.
        //
        // `caller` comes from `raw_data.src` and always carries a
        // ParsedFile-local ID, so it must be translated. `callee_id` (and
        // `callee_symbol.entity_id`) already leaves the resolver in the
        // index-global ID space — the resolver translates local-symbol hits
        // through the per-file remap itself — and must NOT be remapped here:
        // re-remapping corrupts edges whenever a global callee ID numerically
        // collides with one of this file's parsed-local IDs, which depends on
        // nondeterministic parse order.
        let remap_resolved = |resolved: ResolvedRelation| -> ResolvedRelation {
            let caller = remap_id(resolved.caller);
            ResolvedRelation { caller, ..resolved }
        };

        // Resolve local calls from raw_relations after all function entities have been indexed.
        let config = LocalCallResolverConfig {
            enable_signature_matching: true,
            ..Default::default()
        };
        let local_call_resolver = LocalCallResolver::with_config(config);
        let local_calls = local_call_resolver.resolve_from_parsed_file(file);

        let local_call_spans: HashSet<(EntityId, usize)> = local_calls
            .iter()
            .map(|lc| (lc.caller, lc.span.start_byte))
            .collect();

        // Resolve every raw relation up front (no early budget break) so
        // resolution quality decides what survives truncation. The resolver
        // drops relations it cannot map to an internal or classified external
        // target; the survivors below are the ones the canonical graph keeps.
        let entity_map: HashMap<EntityId, &Entity> =
            file.entities.iter().map(|e| (e.id, e)).collect();
        let mut resolved_raw: Vec<ResolvedRelation> = Vec::new();
        for raw_data in &file.raw_relations {
            // File-scoped edges (imports, uses, module-level calls) are
            // resolved like any other edge but stored in the per-file
            // `file_relation_index` keyed by the normalized path, so they
            // never pollute entity-scoped queries or `function_index`.
            if raw_data.level == RelationLevel::File {
                if let Some(resolved) = resolver.resolve_with_scope_map(
                    raw_data,
                    file,
                    project_symbols,
                    self.index,
                    &entity_map,
                ) {
                    self.index
                        .add_file_relation(&normalized_path, remap_resolved(resolved));
                }
                continue;
            }
            let call_key = (raw_data.src, raw_data.span.start_byte);
            if local_call_spans.contains(&call_key) {
                continue;
            }
            #[cfg(debug_assertions)]
            {
                for local_call in &local_calls {
                    if local_call.caller == raw_data.src
                        && Self::spans_overlap(&local_call.span, &raw_data.span)
                        && local_call.span != raw_data.span
                    {
                        tracing::warn!(
                            caller = ?raw_data.src,
                            local_span = ?local_call.span,
                            raw_span = ?raw_data.span,
                            "Span mismatch: local_call and raw_relation overlap with different spans"
                        );
                    }
                }
            }
            if let Some(resolved) = resolver.resolve_with_scope_map(
                raw_data,
                file,
                project_symbols,
                self.index,
                &entity_map,
            ) {
                resolved_raw.push(remap_resolved(resolved));
            }
        }

        let mut all_relations: Vec<ResolvedRelation> =
            Vec::with_capacity(local_calls.len() + resolved_raw.len());
        // Local calls are derived directly from `ParsedFile` data, so both
        // the caller and the callee still live in the ParsedFile-local ID
        // space and need the full remap.
        all_relations.extend(local_calls.iter().map(|local_call| {
            remap_resolved(ResolvedRelation {
                caller: local_call.caller,
                callee_id: Some(remap_id(local_call.callee)),
                callee_name: local_call.callee_name.clone(),
                relation_type: local_call.relation_type,
                span: local_call.span,
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
                overload_signature: None,
            })
        }));
        all_relations.extend(resolved_raw);

        // Macro body calls: `macro_rules!` definition bodies are opaque token
        // trees, so calls inside them (e.g. `run()` in `{ run(); }`) never
        // reach `raw_relations`. Recover them from `ParsedFile.behavior` and
        // resolve through the same symbol table path as ordinary calls. The
        // caller is the owning macro definition entity.
        if !file.behavior.is_empty() {
            let mut seen: HashSet<(EntityId, String, usize, usize)> =
                HashSet::with_capacity(all_relations.len());
            for rel in &all_relations {
                seen.insert((
                    rel.caller,
                    rel.callee_name.clone(),
                    rel.span.start_byte,
                    rel.span.end_byte,
                ));
            }
            for call in extract_macro_body_calls(file) {
                if !entity_map.contains_key(&call.caller_entity_id) {
                    continue;
                }
                let caller_global = remap_id(call.caller_entity_id);
                let call_key = (
                    caller_global,
                    call.callee_name.clone(),
                    call.span.start_byte,
                    call.span.end_byte,
                );
                if seen.contains(&call_key) {
                    continue;
                }
                let raw_data = RawRelationData {
                    src: call.caller_entity_id,
                    level: RelationLevel::Entity,
                    dst_name: call.callee_name.clone(),
                    relation_type: RelationType::DirectCall,
                    span: call.span,
                    stdlib_category: None,
                };
                if let Some(resolved) = resolver.resolve_with_scope_map(
                    &raw_data,
                    file,
                    project_symbols,
                    self.index,
                    &entity_map,
                ) {
                    let resolved = remap_resolved(resolved);
                    seen.insert((
                        resolved.caller,
                        resolved.callee_name.clone(),
                        resolved.span.start_byte,
                        resolved.span.end_byte,
                    ));
                    all_relations.push(resolved);
                }
            }
        }

        let max_relations = self.config.policy.max_relations_per_file;
        if max_relations == 0 {
            // 0 = unlimited: retain every resolved relation.
            for relation in all_relations {
                self.index.add_resolved_relation(relation);
            }
            return;
        }

        if all_relations.len() > max_relations {
            all_relations.sort_by(|a, b| {
                let pa = Self::relation_priority(a);
                let pb = Self::relation_priority(b);
                pb.cmp(&pa)
                    .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
            });
            let dropped = all_relations.len() - max_relations;
            let mut dropped_by_type: std::collections::HashMap<cce_types::RelationType, usize> =
                std::collections::HashMap::new();
            for rel in all_relations.iter().skip(max_relations) {
                *dropped_by_type.entry(rel.relation_type).or_insert(0) += 1;
            }
            tracing::warn!(
                file = %normalized_path,
                total = all_relations.len(),
                budget = max_relations,
                dropped,
                dropped_by_type = ?dropped_by_type,
                "per-file relation budget exceeded; excess relations dropped"
            );
            if let Some(metrics) = &self.metrics {
                metrics.truncated_relations.add(dropped as u64);
            }
            all_relations.truncate(max_relations);
        }

        for relation in all_relations {
            self.index.add_resolved_relation(relation);
        }
    }

    /// Fallback import extraction when `import_table` is missing.
    ///
    /// Handles custom languages via `SymbolExtract` plugins and built-in
    /// languages via a secondary tree-sitter parse. The caller is responsible
    /// for the existence check, warning, and metric increment.
    fn extract_imports_fallback(&self, file: &ParsedFile) -> ImportTable {
        if self.config.policy.symbol_extract_enabled
            && matches!(file.language, cce_types::language::Language::Custom(_))
        {
            let plugin_registry = self.plugin_registry;
            return crate::helpers::extract_imports_from_plugin(
                &file.source,
                &file.language,
                plugin_registry,
                &file.path,
            )
            .unwrap_or_default();
        }
        if let Some(tree) = self.parse_ast(&file.source, &file.language) {
            let plugin_registry = if self.config.policy.symbol_extract_enabled {
                self.plugin_registry
            } else {
                None
            };
            return crate::helpers::extract_imports_with_registry(
                &tree,
                &file.source,
                &file.language,
                None,
                plugin_registry,
                &file.path,
            )
            .unwrap_or_default();
        }
        ImportTable::default()
    }

    /// Parse AST from source code (helper for imports/exports extraction)
    ///
    /// Uses the build-wide shared parser when one was attached , falling
    /// back to a throwaway instance for processors constructed directly.
    fn parse_ast(
        &self,
        source: &str,
        language: &cce_types::language::Language,
    ) -> Option<tree_sitter::Tree> {
        // Ensure the global language resolver is initialized for the fallback
        // parse path. Production builds normally carry `import_table` from the
        // coordinator, so this rarely runs; tests with manually constructed
        // `ParsedFile` fixtures rely on it. The resolver is a process-global
        // `OnceLock`, so repeated calls are cheap no-ops after the first.
        set_language_resolver(cce_parser::tree_sitter_init::get_tree_sitter_language);
        let parse = |parser: &mut AstParser| {
            parser
                .parse_with_tree(source, language)
                .ok()
                .map(|(tree, _)| tree)
        };
        match &self.ast_parser {
            Some(shared) => {
                let mut parser = shared.lock().ok()?;
                parse(&mut parser)
            }
            None => {
                let mut parser = AstParser::new();
                parse(&mut parser)
            }
        }
    }

    fn spans_overlap(a: &cce_types::Span, b: &cce_types::Span) -> bool {
        a.start_byte <= b.end_byte && b.start_byte <= a.end_byte
    }

    fn relation_priority(rel: &ResolvedRelation) -> u32 {
        use cce_types::RelationType;
        match rel.relation_type {
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::PointerCall
            | RelationType::CallbackCall
            | RelationType::GenericCall
            | RelationType::MacroCall
            | RelationType::GoroutineCall
            | RelationType::DeferredCall
            | RelationType::AsyncCall
            | RelationType::HigherOrderCall => 100,
            RelationType::Inheritance
            | RelationType::Implementation
            | RelationType::TraitBound
            | RelationType::TraitInheritance
            | RelationType::ProtocolImplementation
            | RelationType::ImplAssociation
            | RelationType::Embedding
            | RelationType::Mixin => 80,
            RelationType::TypeReference | RelationType::FieldAccess => 60,
            RelationType::ImportStandard
            | RelationType::ImportNamed
            | RelationType::ImportDefault
            | RelationType::ImportNamespace
            | RelationType::ImportDynamic
            | RelationType::IncludeLocal
            | RelationType::Use
            | RelationType::Using
            | RelationType::MacroDependency
            | RelationType::ModuleDependency => 40,
            RelationType::Contains
            | RelationType::ElementContains
            | RelationType::TemplateReference
            | RelationType::ParameterBinding
            | RelationType::EventCallback => 20,
        }
    }

    /// Create a resolver with current settings
    pub fn create_resolver(&self) -> RelationResolver {
        let mut resolver = RelationResolver::new();
        resolver.with_filter(self.config.policy.filter_stdlib_calls);
        resolver.with_metrics(self.metrics.clone());
        if let Some(ref packages) = self.config.package_data.external_packages {
            resolver.with_external_packages(packages.clone());
        }
        if let Some(ref dependencies) = self.config.package_data.external_dependencies {
            resolver.with_external_dependencies(dependencies.clone());
        }
        resolver
    }
}
