//! Project symbol table (project-level)
//!
//! Manages all packages in a project and external dependencies.
//! This is the top-level symbol table in the four-level hierarchy.
//!
//! # Enhanced Features
//! - Module paths and namespaces
//! - Import aliases
//! - Re-exports
//! - External package symbols
//! - Visibility rules

use std::sync::Arc;

use super::{ExternalSymbolTable, ProjectStats, ProjectSymbolTable, SimpleNameEntry};
use crate::symbol::SymbolRef;
use crate::symbol_table::package;
use crate::type_inference::TypeInferenceContext;
use cce_types::entity::EntityId;
use cce_types::normalize_project_path;

impl ProjectSymbolTable {
    /// Find all symbols matching a pattern across all packages
    pub fn find_all_matching(&self, pattern: &str) -> Vec<SymbolRef> {
        let pattern_lower = pattern.to_lowercase();
        let mut results = Vec::new();

        // Search in all packages
        for package in self.packages.iter() {
            for metadata in package.find_matching(&pattern_lower) {
                // Use a synthetic entity ID for pattern matches
                let counter = results.len() as u64;
                let entity_id = EntityId(Self::SYNTHETIC_MARK | counter);
                results.push(SymbolRef::new(entity_id, metadata));
            }
        }

        results
    }

    /// Get the total number of symbols in the project
    pub fn total_symbol_count(&self) -> usize {
        self.packages
            .iter()
            .map(|p| p.public_exports().len() + p.internal_exports().len())
            .sum()
    }

    /// Get project statistics
    pub fn project_stats(&self) -> ProjectStats {
        ProjectStats {
            package_count: self.packages.len(),
            external_dep_count: self.external_deps.len(),
            total_symbols: self.total_symbol_count(),
        }
    }

    pub fn clear_cache(&self) {
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            cache.clear();
        }
        self.negative_cache.clear();
        self.wildcard_expansion_cache.clear();
        self.invalidate_sorted_packages();
        self.invalidate_sorted_external();
    }

    /// Invalidate resolution cache entries that involve the given file.
    ///
    /// This is a fine-grained alternative to `clear_cache()` for incremental
    /// updates: only cache entries keyed by `file_path` are removed, leaving
    /// entries for other files intact. This significantly reduces cache
    /// invalidation overhead during hot updates.
    pub fn invalidate_cache_for_file(&self, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        // Remove positive cache entries for this file
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            let keys_to_remove: Vec<(String, String)> = cache
                .iter()
                .filter(|(key, _)| key.0 == normalized)
                .map(|(key, _)| key.clone())
                .collect();
            for key in keys_to_remove {
                cache.pop(&key);
            }
        }
        // Remove negative cache entries for this file
        self.negative_cache
            .retain(|(key_file, _), _| key_file != &normalized);
        // Remove wildcard expansion cache entries for this file
        self.wildcard_expansion_cache
            .retain(|(key_file, _), _| key_file != &normalized);
    }

    /// Fine-grained cache invalidation for a single symbol insertion.
    ///
    /// Unlike `clear_cache()` which evicts the entire cache, this method
    /// only invalidates entries that could be affected by the newly inserted
    /// symbol. The `resolution_cache` is keyed by `(caller_file, name)` where
    /// `name` may be a simple name or a qualified name; inserting
    /// `file::simple_name` can affect any caller that previously cached a
    /// miss or a hit for that simple name or qualified name. Therefore we
    /// scan all entries and evict those where the cached name matches the
    /// inserted symbol's simple name, its qualified form, or ends with
    /// `::simple_name`.
    pub fn invalidate_cache_for_symbol(&self, simple_name: &str, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        let qualified = format!("{}::{}", normalized, simple_name);
        let suffix = format!("::{}", simple_name);
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            let keys_to_remove: Vec<(String, String)> = cache
                .iter()
                .filter(|((_, cached_name), _)| {
                    cached_name == simple_name
                        || cached_name == &qualified
                        || cached_name.ends_with(&suffix)
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in keys_to_remove {
                cache.pop(&key);
            }
        }
        self.negative_cache.retain(|(_, cached_name), _| {
            !(cached_name == &qualified || cached_name.ends_with(&suffix))
        });
        self.wildcard_expansion_cache
            .retain(|(key_file, _), _| key_file != &normalized);
    }

    /// Bulk variant of `invalidate_cache_for_symbol` that processes a set of
    /// symbol names for the same file in a single scan. Useful during
    /// `prepopulate_index_symbols` where many symbols of one file are
    /// inserted sequentially.
    pub fn invalidate_cache_for_symbols(
        &self,
        simple_names: &std::collections::HashSet<String>,
        file_path: &str,
    ) {
        if simple_names.is_empty() {
            return;
        }
        let normalized = normalize_project_path(file_path);
        // Pre-build match helpers to avoid per-entry allocation
        let mut qualified_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in simple_names {
            qualified_set.insert(format!("{}::{}", normalized, name));
        }
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            let keys_to_remove: Vec<(String, String)> = cache
                .iter()
                .filter(|((_, cached_name), _)| {
                    if simple_names.contains(cached_name) {
                        return true;
                    }
                    if qualified_set.contains(cached_name) {
                        return true;
                    }
                    // Check suffix match: any simple_name suffix
                    for n in simple_names {
                        if cached_name.ends_with(&format!("::{}", n)) {
                            return true;
                        }
                    }
                    false
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in keys_to_remove {
                cache.pop(&key);
            }
        }
        self.negative_cache.retain(|(_, cached_name), _| {
            if qualified_set.contains(cached_name) {
                return false;
            }
            for n in simple_names {
                if cached_name.ends_with(&format!("::{}", n)) {
                    return false;
                }
            }
            true
        });
        self.wildcard_expansion_cache
            .retain(|(key_file, _), _| key_file != &normalized);
    }

    /// Fine-grained invalidation for a set of simple names irrespective of
    /// file. Used for package-level deltas where the qualified prefix is the
    /// package name, not a file path. Only cache entries whose name matches
    /// or suffix-matches an affected name are evicted.
    pub fn invalidate_cache_for_names(&self, names: &std::collections::HashSet<String>) {
        if names.is_empty() {
            return;
        }
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            let keys_to_remove: Vec<(String, String)> = cache
                .iter()
                .filter(|((_, cached_name), _)| {
                    if names.contains(cached_name) {
                        return true;
                    }
                    for n in names {
                        if cached_name.ends_with(&format!("::{}", n)) {
                            return true;
                        }
                    }
                    false
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in keys_to_remove {
                cache.pop(&key);
            }
        }
        self.negative_cache.retain(|(_, cached_name), _| {
            if names.contains(cached_name) {
                return false;
            }
            for n in names {
                if cached_name.ends_with(&format!("::{}", n)) {
                    return false;
                }
            }
            true
        });
    }

    pub fn rebuild_resolution_cache(&self) {
        self.clear_cache();
    }

    pub(crate) fn invalidate_sorted_packages(&self) {
        if let Ok(mut guard) = self.sorted_packages_cache.write() {
            *guard = None;
        }
    }

    pub(crate) fn invalidate_sorted_external(&self) {
        if let Ok(mut guard) = self.sorted_external_cache.write() {
            *guard = None;
        }
    }

    pub(crate) fn sorted_packages(&self) -> Vec<Arc<package::PackageSymbolTable>> {
        // Fast path: 0 or 1 packages need no sort.
        if self.packages.len() < 2 {
            return self
                .packages
                .iter()
                .map(|p| Arc::clone(p.value()))
                .collect();
        }
        if let Ok(guard) = self.sorted_packages_cache.read() {
            if let Some(cached) = guard.as_ref() {
                return cached.clone();
            }
        }
        let mut packages: Vec<Arc<package::PackageSymbolTable>> = self
            .packages
            .iter()
            .map(|p| Arc::clone(p.value()))
            .collect();
        packages.sort_by(|a, b| a.package_id.cmp(&b.package_id));
        if let Ok(mut guard) = self.sorted_packages_cache.write() {
            *guard = Some(packages.clone());
        }
        packages
    }

    pub(crate) fn sorted_external_deps(&self) -> Vec<Arc<ExternalSymbolTable>> {
        if self.external_deps.len() < 2 {
            return self
                .external_deps
                .iter()
                .map(|d| Arc::clone(d.value()))
                .collect();
        }
        if let Ok(guard) = self.sorted_external_cache.read() {
            if let Some(cached) = guard.as_ref() {
                return cached.clone();
            }
        }
        let mut deps: Vec<Arc<ExternalSymbolTable>> = self
            .external_deps
            .iter()
            .map(|d| Arc::clone(d.value()))
            .collect();
        deps.sort_by(|a, b| a.package_name.cmp(&b.package_name));
        if let Ok(mut guard) = self.sorted_external_cache.write() {
            *guard = Some(deps.clone());
        }
        deps
    }

    /// Get symbol by qualified name
    /// Format: "package::module::symbol"
    pub fn get_by_qualified_name(&self, qualified_name: &str) -> Option<EntityId> {
        // Check global index first
        if let Some(entity_id) = self.global_index.get(qualified_name).map(|id| *id) {
            return Some(entity_id);
        }
        // Fallback: try hierarchical resolution
        self.resolve_global_qualified(qualified_name)
    }

    /// Get symbol by simple name (search in all packages)
    ///
    /// Consistent with `resolve_simple_name`: candidates are kept sorted
    /// deterministically at insert time and the first entry is returned
    pub fn get_by_simple_name(&self, name: &str) -> Option<EntityId> {
        self.simple_name_index
            .get(name)
            .and_then(|entries| entries.first().map(|entry| entry.entity_id()))
    }

    /// Rebuild all indices
    pub fn rebuild_indices(&self) {
        self.global_index.clear();
        self.simple_name_index.clear();
        self.file_symbol_contrib.clear();
        self.file_to_package.clear();
        self.namespace_index.clear();
        // Resolution entries are keyed by name only; rebuilding may move
        // any name to a new entity id, so both caches must be invalidated.
        self.clear_cache();

        for package in self.packages.iter() {
            for name in package.public_export_names() {
                if let Some(metadata) = package.get_public_export(&name) {
                    let symbol_ref = self.stable_symbol_ref(&metadata);
                    self.insert_simple_name_entry(
                        &name,
                        SimpleNameEntry::PackageExport {
                            package_id: package.package_id.clone(),
                            entity_id: symbol_ref.symbol_id,
                        },
                    );
                }
            }
            for module in package.value().all_modules() {
                let normalized = normalize_project_path(&module.file_path);
                self.file_to_package
                    .insert(normalized, package.key().clone());
            }
        }
        self.invalidate_sorted_packages();
        // Rebuild global index from simple_name_index after all insertions
        self.rebuild_global_indices();
    }

    // === Type Inference Context Management ===

    /// Store a type inference context for a file.
    ///
    /// Called during symbol table construction after type inference has been
    /// performed on a parsed file. The context is keyed by normalized file
    /// path so incremental updates can replace stale contexts.
    pub fn set_type_inference_context(&self, file_path: &str, ctx: TypeInferenceContext) {
        let normalized = normalize_project_path(file_path);
        self.type_inference_contexts.insert(normalized, ctx);
    }

    /// Get the type inference context for a file.
    ///
    /// Returns `None` if no context has been built for the file (e.g., for
    /// files that haven't been parsed yet or for languages without type
    /// inference support).
    pub fn get_type_inference_context(&self, file_path: &str) -> Option<TypeInferenceContext> {
        let normalized = normalize_project_path(file_path);
        self.type_inference_contexts
            .get(&normalized)
            .map(|ctx| ctx.clone())
    }

    /// Remove the type inference context for a file.
    ///
    /// Called during incremental updates when a file is being reprocessed.
    pub fn remove_type_inference_context(&self, file_path: &str) {
        let normalized = normalize_project_path(file_path);
        self.type_inference_contexts.remove(&normalized);
        self.cross_file_propagator.remove_file(&normalized);
    }

    // === Cross-File Type Propagation ===

    /// Rebuild the cross-file return-type cache from all currently stored
    /// type inference contexts and provided file entity lists.
    ///
    /// This is the bulk path used by [`crate::index::builder::symbol_table::SymbolTableBuilder::build`].
    /// It clears the cache, repopulates it from every file that has a context
    /// with `High`/`Medium` return types, and then propagates those return
    /// types into variable bindings (`x = foo()` -> `x: ReturnTypeOfFoo`).
    pub fn rebuild_cross_file_propagator(&self, files: &[&cce_types::ParsedFile]) {
        self.cross_file_propagator.clear();
        let file_map: std::collections::HashMap<String, &cce_types::ParsedFile> =
            files.iter().map(|f| (f.path.clone(), *f)).collect();

        for file in files {
            if let Some(ctx) = self
                .type_inference_contexts
                .get(&normalize_project_path(&file.path))
            {
                self.cross_file_propagator
                    .insert_file(&file.path, &ctx, &file.entities);
            }
        }
        // Propagate into variable types.
        self.propagate_cross_file_variables(&file_map);
    }

    /// Update the cross-file cache for a single file (incremental path).
    ///
    /// Removes the file's previous entries, inserts the new ones from its
    /// updated context, and re-propagates variable types for the changed file
    /// itself. Dependent files that call the changed function will be
    /// handled lazily by the resolver's cross-file fallback, avoiding an
    /// O(project) scan on every incremental update.
    pub fn update_cross_file_for_file(&self, file: &cce_types::ParsedFile) {
        let normalized = normalize_project_path(&file.path);
        self.cross_file_propagator.remove_file(&normalized);
        if let Some(ctx) = self.type_inference_contexts.get(&normalized) {
            self.cross_file_propagator
                .insert_file(&file.path, &ctx, &file.entities);
        }
        // Propagate variables for the changed file only (eager).
        self.propagate_cross_file_variables_for_file(file);
    }

    /// Propagate cross-file variable types for a single file.
    pub fn propagate_cross_file_variables_for_file(&self, file: &cce_types::ParsedFile) {
        use crate::type_inference::InferenceOrigin;
        use cce_types::entity::EntityKind;

        let normalized = normalize_project_path(&file.path);
        let Some(mut ctx_ref) = self.type_inference_contexts.get_mut(&normalized) else {
            return;
        };
        let ctx = ctx_ref.value_mut();

        for entity in &file.entities {
            if entity.kind != EntityKind::Variable {
                continue;
            }
            if let Some(existing) = ctx.get_variable_type(&entity.name) {
                if crate::type_inference::origin_is_authoritative(existing.origin) {
                    continue;
                }
            }
            let call_target = entity
                .metadata
                .get("call_target")
                .or_else(|| entity.metadata.get("constructor_type"))
                .cloned();
            if let Some(target) = call_target {
                let simple = target
                    .rsplit(['.', ':', '/'])
                    .next()
                    .unwrap_or(&target)
                    .trim()
                    .to_string();
                if simple.is_empty() {
                    continue;
                }
                if let Some(return_binding) =
                    self.cross_file_propagator.get_return_type_by_name(&simple)
                {
                    let propagated = crate::type_inference::TypeBinding {
                        type_name: return_binding.type_name.clone(),
                        type_entity_id: return_binding.type_entity_id,
                        span: entity.span,
                        origin: Some(InferenceOrigin::CrossFilePropagation),
                        shape: return_binding.shape.clone(),
                    };
                    let should_insert = ctx.get_variable_type(&entity.name).is_none_or(|e| {
                        crate::type_inference::binding_supersedes(propagated.origin, e.origin)
                    });
                    if should_insert {
                        ctx.add_variable_type(entity.name.clone(), propagated);
                    }
                }
            }
        }
    }

    /// Remove cross-file entries for a file.
    pub fn remove_cross_file_for_file(&self, file_path: &str) {
        self.cross_file_propagator
            .remove_file(&normalize_project_path(file_path));
    }

    /// Get a cross-file return type by EntityId.
    pub fn get_cross_file_return_type(
        &self,
        entity_id: cce_types::entity::EntityId,
    ) -> Option<crate::type_inference::TypeBinding> {
        self.cross_file_propagator.get_return_type(entity_id)
    }

    /// Get a cross-file return type by simple function name.
    pub fn get_cross_file_return_type_by_name(
        &self,
        name: &str,
    ) -> Option<crate::type_inference::TypeBinding> {
        self.cross_file_propagator.get_return_type_by_name(name)
    }

    /// Propagate cross-file return types into variable bindings using the
    /// provided file map. This is the bulk propagation path.
    fn propagate_cross_file_variables(
        &self,
        file_map: &std::collections::HashMap<String, &cce_types::ParsedFile>,
    ) {
        use crate::type_inference::InferenceOrigin;
        use cce_types::entity::EntityKind;

        for (raw_path, file) in file_map {
            let normalized = normalize_project_path(raw_path);
            // Ensure context exists (create empty if missing).
            let mut ctx_entry = self
                .type_inference_contexts
                .entry(normalized.clone())
                .or_insert_with(|| crate::type_inference::TypeInferenceContext::new(file.language));
            let ctx = ctx_entry.value_mut();

            for entity in &file.entities {
                if entity.kind != EntityKind::Variable {
                    continue;
                }
                if let Some(existing) = ctx.get_variable_type(&entity.name) {
                    if crate::type_inference::origin_is_authoritative(existing.origin) {
                        continue;
                    }
                }
                let call_target = entity
                    .metadata
                    .get("call_target")
                    .or_else(|| entity.metadata.get("constructor_type"))
                    .cloned();
                if let Some(target) = call_target {
                    let simple = target
                        .rsplit(['.', ':', '/'])
                        .next()
                        .unwrap_or(&target)
                        .trim()
                        .to_string();
                    if simple.is_empty() {
                        continue;
                    }
                    if let Some(return_binding) =
                        self.cross_file_propagator.get_return_type_by_name(&simple)
                    {
                        let propagated = crate::type_inference::TypeBinding {
                            type_name: return_binding.type_name.clone(),
                            type_entity_id: return_binding.type_entity_id,
                            span: entity.span,
                            origin: Some(InferenceOrigin::CrossFilePropagation),
                            shape: return_binding.shape.clone(),
                        };
                        let should_insert = ctx.get_variable_type(&entity.name).is_none_or(|e| {
                            crate::type_inference::binding_supersedes(propagated.origin, e.origin)
                        });
                        if should_insert {
                            ctx.add_variable_type(entity.name.clone(), propagated);
                        }
                    }
                }
            }
        }
    }
}
