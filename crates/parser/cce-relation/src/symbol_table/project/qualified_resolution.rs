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

use super::{ProjectSymbolTable, SimpleNameEntry};
use crate::symbol::SymbolMetadata;
use crate::symbol::SymbolRef;
use crate::symbol::scope::ScopeContext;
use crate::symbol_table::package;
use crate::type_inference::types::TypeShape;
use cce_types::entity::EntityId;

/// Context for overload-aware disambiguation.
#[derive(Debug, Default)]
pub struct OverloadContext {
    pub receiver_type: Option<String>,
    pub arg_count: Option<usize>,
    pub arg_types: Option<Vec<Option<TypeShape>>>,
}

impl ProjectSymbolTable {
    // === Global Resolution ===

    /// Resolve a symbol by qualified name
    ///
    /// Format: "package::module::symbol" or "file::symbol"
    pub fn resolve_qualified(&self, qualified_name: &str) -> Option<SymbolRef> {
        self.resolve_qualified_cached(qualified_name, None, true, None)
    }

    /// Resolve a qualified name with a caller file context.
    ///
    /// The caller file disambiguates bare-name resolution (preferring the
    /// caller's own file) and contextualizes the positive-result cache so
    /// results resolved for one file cannot short-circuit another
    pub(crate) fn resolve_qualified_cached(
        &self,
        qualified_name: &str,
        caller_file: Option<&str>,
        allow_last_segment_fallback: bool,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        // cache key is (file_path, name). Only positive results are
        // cached — caching misses would make the negative cache stale
        // forever: symbols added later (incremental path) would be shadowed
        // by the cached `None` until a plugin injection rebuilds the cache
        //
        let cache_key = (
            caller_file.unwrap_or_default().to_string(),
            qualified_name.to_string(),
        );
        // LRU cache requires &mut for get (to update access order), so use try_lock
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            if let Some(result) = cache.get(&cache_key) {
                // Cache hit: record metric if available
                if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                    metrics.resolution_cache_hit_total.increment();
                }
                return result.clone();
            }
        }

        // Multi-segment qualified misses are cached separately so repeat
        // lookups of the same absent name skip the package walk. Unlike the
        // positive cache, this is cleared on every symbol-table mutation
        // (`insert_symbol` / `add_package` / `clear_cache`), so it can never
        // shadow symbols added later by incremental builds.
        if qualified_name.contains("::") && self.negative_cache.contains_key(&cache_key) {
            // Negative cache hit: record metric if available
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.resolution_cache_miss_total.increment();
            }
            return None;
        }

        let result = self.resolve_qualified_internal(
            qualified_name,
            caller_file,
            allow_last_segment_fallback,
            from_scope,
        );

        if result.is_some() {
            self.cache_resolution(cache_key, result.clone());
        } else if qualified_name.contains("::") {
            self.cache_negative_resolution(cache_key);
            // Cache miss (will be cached next time): record metric if available
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.resolution_cache_miss_total.increment();
            }
        }

        result
    }

    pub(crate) fn resolve_qualified_cached_with_overload(
        &self,
        qualified_name: &str,
        caller_file: Option<&str>,
        allow_last_segment_fallback: bool,
        from_scope: Option<&ScopeContext>,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        self.resolve_qualified_internal_with_overload(
            qualified_name,
            caller_file,
            allow_last_segment_fallback,
            from_scope,
            overload_ctx,
        )
    }

    /// Insert a negative (miss) result into the bounded negative cache.
    fn cache_negative_resolution(&self, key: (String, String)) {
        let capacity = self.resolution_cache_capacity();
        if self.negative_cache.len() >= capacity {
            let mut to_remove: Vec<(String, String)> = Vec::new();
            for entry in self.negative_cache.iter().take(capacity / 2) {
                to_remove.push(entry.key().clone());
            }
            for key in to_remove {
                self.negative_cache.remove(&key);
            }
        }
        self.negative_cache.insert(key, ());
    }

    /// Insert a positive result into the LRU resolution cache.
    ///
    /// The LRU cache automatically evicts the least-recently-used entry
    /// when capacity is reached, replacing the previous manual batch-eviction
    /// logic.
    fn cache_resolution(&self, key: (String, String), value: Option<SymbolRef>) {
        if let Some(mut cache) = self.resolution_cache.try_lock() {
            cache.put(key, value);
        }
    }

    fn resolve_qualified_internal(
        &self,
        qualified_name: &str,
        caller_file: Option<&str>,
        allow_last_segment_fallback: bool,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        self.resolve_qualified_internal_with_overload(
            qualified_name,
            caller_file,
            allow_last_segment_fallback,
            from_scope,
            None,
        )
    }

    pub(crate) fn resolve_qualified_internal_with_overload(
        &self,
        qualified_name: &str,
        caller_file: Option<&str>,
        allow_last_segment_fallback: bool,
        from_scope: Option<&ScopeContext>,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        // Parse qualified name
        let mut parts: Vec<&str> = qualified_name.split("::").collect();
        if parts.is_empty() {
            return None;
        }

        // a leading `crate` segment maps to the caller's own package;
        // module paths no longer carry the `crate::` prefix (see
        // `determine_module_path`), so strip it before any index lookup.
        if parts.first() == Some(&"crate") {
            parts.remove(0);
        }
        if parts.is_empty() {
            return None;
        }

        if parts.len() == 1 {
            // Simple name - search in all packages (deterministically),
            // preferring symbols defined in the caller's own file.
            return self.resolve_simple_name_with_overload(
                parts[0],
                caller_file,
                from_scope,
                overload_ctx,
            );
        }

        let joined = parts.join("::");

        // exact global-index match first ({file}::{name} and
        // {package}::{name} keys were never queried before).
        if let Some(_symbol_id) = self.global_index.get(&joined).map(|id| *id) {
            if let Some(metadata) = self.metadata_for_global_key(&joined, from_scope) {
                return Some(self.stable_symbol_ref(&metadata));
            }
        }

        // Namespace-level index match: try "package::namespace::symbol" pattern
        if parts.len() >= 2 {
            for package in self.sorted_packages() {
                let ns_and_symbol = parts[1..].join("::");
                if let Some(metadata) = package.resolve_qualified(&ns_and_symbol, from_scope) {
                    return Some(self.stable_symbol_ref(&metadata));
                }
            }
        }

        // Multi-segment: resolve through per-package module_path indexes in
        // deterministic package order (no DashMap iteration order)
        for package in self.sorted_packages() {
            if let Some(metadata) = package.resolve_qualified(&joined, from_scope) {
                return Some(self.stable_symbol_ref(&metadata));
            }
        }

        // Last-segment fallback: qualified names may refer to a
        // definition through a receiver/path prefix rather than an index key
        // (e.g. `obj.method()`, `Foo::new()`). The full key is never stored,
        // so fall back to the last segment, preferring candidates whose
        // file/module path matches the leading path segments before falling
        // back to the plain simple-name search.
        //
        // The fallback is disabled for stdlib targets (resolver.rs passes
        // `allow_last_segment_fallback=false`) so `Vec::new` can never hijack
        // a local `new` definition.
        if allow_last_segment_fallback {
            let segments: Vec<&str> = qualified_name
                .split([':', '.'])
                .filter(|s| !s.is_empty())
                .collect();
            if segments.len() > 1 {
                let last = segments[segments.len() - 1];
                let prefix = segments[..segments.len() - 1].join("::");
                return self.resolve_with_prefix_fallback_with_overload(
                    last,
                    &prefix,
                    caller_file,
                    from_scope,
                    overload_ctx,
                );
            }
        }

        None
    }

    /// Resolve a simple name preferring candidates whose file/module path
    /// matches the given prefix, then falling back to the plain simple-name
    /// search.
    ///
    /// The prefix is the leading path segments of a qualified name
    /// (`obj` for `obj.method`, `foo::bar` for `foo::bar::baz`). This
    /// disambiguates the last segment without requiring an exact full-name
    /// index entry.
    #[allow(dead_code)]
    fn resolve_with_prefix_fallback(
        &self,
        name: &str,
        prefix: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        self.resolve_with_prefix_fallback_with_overload(name, prefix, caller_file, from_scope, None)
    }

    fn resolve_with_prefix_fallback_with_overload(
        &self,
        name: &str,
        prefix: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        if let Some(entries) = self.simple_name_index.get(name) {
            let candidates: Vec<SimpleNameEntry> = entries.clone().to_vec();

            // Prefer the caller's own file first (same semantics as
            // `resolve_simple_name`).
            if let Some(caller_file) = caller_file {
                if let Some(entry) = candidates
                    .iter()
                    .find(|e| e.file_path().is_some_and(|p| p == caller_file))
                {
                    if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                        return Some(symbol);
                    }
                }
            }

            // Then candidates whose file/module path matches the prefix.
            let prefix_lower = prefix.to_ascii_lowercase();
            for entry in &candidates {
                let path_matches = match entry {
                    SimpleNameEntry::FileSymbol {
                        file_path,
                        module_path,
                        ..
                    } => {
                        let file_lower = file_path.to_ascii_lowercase();
                        let module_lower = module_path.to_ascii_lowercase();
                        file_lower
                            .split(['/', '\\', '.', ':'])
                            .any(|seg| seg == prefix_lower.as_str())
                            || module_lower
                                .split([':', '/', '\\', '.'])
                                .any(|seg| seg == prefix_lower.as_str())
                            || file_lower.starts_with(&prefix_lower)
                            || module_lower.ends_with(&prefix_lower)
                    }
                    SimpleNameEntry::PackageExport { package_id, .. } => {
                        package_id.eq_ignore_ascii_case(prefix)
                    }
                };
                if path_matches {
                    if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                        return Some(symbol);
                    }
                }
            }
        }

        // No prefix-matched candidate; fall back to the plain simple-name
        // search (preserves the original behavior for receiver names that do
        // not correspond to any file/module path).
        self.resolve_simple_name_with_overload(name, caller_file, from_scope, overload_ctx)
    }

    /// Look up the metadata behind a global-index key.
    ///
    /// Keys are either `{file}::{name}` (file symbols) or `{package}::{name}`
    /// (package public exports).
    pub(crate) fn metadata_for_global_key(
        &self,
        key: &str,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolMetadata> {
        let (prefix, name) = key.rsplit_once("::")?;

        // Try as a file symbol first: {file}::{name}
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(prefix) {
                let found = match from_scope {
                    Some(scope) => module.get_export_visible_from(name, scope),
                    None => module.get_export(name),
                };
                if let Some(metadata) = found {
                    return Some(metadata.clone());
                }
            }
        }

        // Then as a package export: {package}::{name}
        let package = self.get_package_by_name(prefix)?;
        package.get_public_export(name)
    }

    /// Resolve a simple name (search in all packages, deterministically)
    #[allow(dead_code)]
    fn resolve_simple_name(
        &self,
        name: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        self.resolve_simple_name_with_overload(name, caller_file, from_scope, None)
    }

    fn resolve_simple_name_with_overload(
        &self,
        name: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        if let Some(entries) = self.simple_name_index.get(name) {
            let candidates: Vec<SimpleNameEntry> = entries.clone().to_vec();

            if let Some(caller_file) = caller_file {
                if let Some(entry) = candidates
                    .iter()
                    .find(|e| e.file_path().is_some_and(|p| p == caller_file))
                {
                    if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                        return Some(symbol);
                    }
                }
            }

            if candidates.len() > 1 {
                if let Some(symbol) = self.disambiguate_candidates_with_overload(
                    &candidates,
                    name,
                    caller_file,
                    from_scope,
                    overload_ctx,
                ) {
                    return Some(symbol);
                }
            }

            for entry in &candidates {
                if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                    return Some(symbol);
                }
            }
        }

        for dep in self.sorted_external_deps() {
            if let Some(metadata) = dep.get_export(name) {
                return Some(self.stable_symbol_ref(metadata));
            }
        }

        None
    }

    /// Disambiguate between multiple candidates for a simple name.
    ///
    /// Uses the following factors in priority order:
    /// 1. Import priority: if the caller file imports this name, prefer the
    ///    imported version (it's the explicitly intended target)
    /// 2. Same-package preference: prefer candidates from the same package
    ///    as the caller file
    /// 3. Module path matching: prefer candidates whose module path aligns
    ///    with the caller's context
    ///
    /// Returns `None` when no heuristic can pick a winner; callers fall back
    /// to the default deterministic iteration order.
    #[allow(dead_code)]
    fn disambiguate_candidates(
        &self,
        candidates: &[SimpleNameEntry],
        name: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        self.disambiguate_candidates_with_overload(candidates, name, caller_file, from_scope, None)
    }

    fn disambiguate_candidates_with_overload(
        &self,
        candidates: &[SimpleNameEntry],
        name: &str,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        // Factor 1: Import priority
        if let Some(caller_file) = caller_file {
            let try_import_check =
                |package: &Arc<package::PackageSymbolTable>| -> Option<SymbolRef> {
                    let module = package.get_module(caller_file)?;
                    if !module.is_imported(name) {
                        return None;
                    }
                    let binding = module.get_import(name)?;
                    if let Some(resolved) = binding.resolved_symbol {
                        return Some(resolved);
                    }
                    for entry in candidates {
                        if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                            return Some(symbol);
                        }
                    }
                    None
                };
            if let Some(symbol) = self
                .get_package_for_file(caller_file)
                .and_then(|pkg| try_import_check(&pkg))
            {
                return Some(symbol);
            }
            for package in self.sorted_packages() {
                if let Some(symbol) = try_import_check(&package) {
                    return Some(symbol);
                }
            }
        }

        // Factor 2: Overload-aware disambiguation (receiver type + arity)
        if let Some(ctx) = overload_ctx {
            if let Some(receiver) = &ctx.receiver_type {
                let mut receiver_matches: Vec<SimpleNameEntry> = Vec::new();
                for entry in candidates {
                    let eid = entry.entity_id();
                    if let Some(owner) = self
                        .global_type_index
                        .read()
                        .ok()
                        .and_then(|idx| idx.owner_of(eid).map(|k| k.qualified.clone()))
                    {
                        if &owner == receiver {
                            receiver_matches.push(entry.clone());
                        }
                    }
                }
                if receiver_matches.len() == 1 {
                    if let Some(symbol) =
                        self.symbol_from_entry(&receiver_matches[0], name, from_scope)
                    {
                        return Some(symbol);
                    }
                }
                if !receiver_matches.is_empty() {
                    if receiver_matches.len() > 1 {
                        if let Some(best) =
                            self.pick_best_overload(&receiver_matches, name, ctx, from_scope)
                        {
                            return Some(best);
                        }
                    }
                    if let Some(count) = ctx.arg_count {
                        let mut arity_matches: Vec<SimpleNameEntry> = Vec::new();
                        for entry in &receiver_matches {
                            if let Some(arity) = self.candidate_arity(entry, name) {
                                if arity == count {
                                    arity_matches.push(entry.clone());
                                }
                            }
                        }
                        if arity_matches.len() == 1 {
                            if let Some(symbol) =
                                self.symbol_from_entry(&arity_matches[0], name, from_scope)
                            {
                                return Some(symbol);
                            }
                        }
                    }
                }
            }

            if let Some(count) = ctx.arg_count {
                let mut arity_matches: Vec<SimpleNameEntry> = Vec::new();
                for entry in candidates {
                    if let Some(arity) = self.candidate_arity(entry, name) {
                        if arity == count {
                            arity_matches.push(entry.clone());
                        }
                    }
                }
                if arity_matches.len() == 1 {
                    if let Some(symbol) =
                        self.symbol_from_entry(&arity_matches[0], name, from_scope)
                    {
                        return Some(symbol);
                    }
                }
                if !arity_matches.is_empty() {
                    if let Some(best) =
                        self.pick_best_overload(&arity_matches, name, ctx, from_scope)
                    {
                        return Some(best);
                    }
                }
                // Also try overload resolution across all candidates when arity matches are not unique
                if let Some(best) = self.pick_best_overload(candidates, name, ctx, from_scope) {
                    return Some(best);
                }
            } else if ctx.arg_types.is_some() {
                if let Some(best) = self.pick_best_overload(candidates, name, ctx, from_scope) {
                    return Some(best);
                }
            }
            if ctx.receiver_type.is_none() {
                if let Some(entity_id) = self.pick_best_overload_for_bare_name(name, ctx) {
                    for entry in candidates {
                        if entry.entity_id() == entity_id {
                            if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                                return Some(symbol);
                            }
                        }
                    }
                }
            }
        }

        // Factor 3: Same-package preference
        if let Some(caller_file) = caller_file {
            let caller_package = self
                .get_package_for_file(caller_file)
                .map(|pkg| pkg.package_id.clone())
                .or_else(|| {
                    self.sorted_packages()
                        .iter()
                        .find(|pkg| pkg.has_module(caller_file))
                        .map(|pkg| pkg.package_id.clone())
                });

            if let Some(ref pkg_id) = caller_package {
                for entry in candidates {
                    if let SimpleNameEntry::PackageExport { package_id, .. } = entry {
                        if package_id == pkg_id {
                            if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                                return Some(symbol);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn candidate_arity(&self, entry: &SimpleNameEntry, name: &str) -> Option<usize> {
        let eid = entry.entity_id();
        if let Some(owner) = self
            .global_type_index
            .read()
            .ok()
            .and_then(|idx| idx.owner_of(eid).map(|k| k.qualified.clone()))
        {
            if let Some(overload) = self.get_overload_set(&owner, name) {
                for cand in &overload.candidates {
                    if cand.entity_id == eid {
                        return Some(cand.parameter_types.len());
                    }
                }
            }
        }
        None
    }

    fn pick_best_overload(
        &self,
        candidates: &[SimpleNameEntry],
        name: &str,
        ctx: &OverloadContext,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        // Group candidates by owner; pick the overload set that contains them
        use std::collections::HashMap;
        let mut best: Option<(SymbolRef, u32)> = None;
        let arg_types: Vec<Option<&TypeShape>> = ctx
            .arg_types
            .as_ref()
            .map(|v| v.iter().map(|opt| opt.as_ref()).collect())
            .unwrap_or_default();

        // If we have no arg_types but have arg_count, synthesize None types for scoring
        let effective_arg_types: Vec<Option<&TypeShape>> = if !arg_types.is_empty() {
            arg_types
        } else if let Some(count) = ctx.arg_count {
            vec![None; count]
        } else {
            Vec::new()
        };

        for entry in candidates {
            let eid = entry.entity_id();
            let owner_opt = self
                .global_type_index
                .read()
                .ok()
                .and_then(|idx| idx.owner_of(eid).map(|k| k.qualified.clone()));
            if let Some(owner) = owner_opt {
                if let Some(overload) = self.get_overload_set(&owner, name) {
                    // Only consider overloads where this candidate is actually a member
                    if !overload.candidates.iter().any(|c| c.entity_id == eid) {
                        continue;
                    }
                    let candidate_best = if effective_arg_types.is_empty() {
                        overload.resolve(&[])
                    } else {
                        overload.resolve_with_args(&effective_arg_types, &HashMap::new())
                    };
                    if let Some(best_cand) = candidate_best {
                        if best_cand.entity_id == eid {
                            if let Some(symbol) = self.symbol_from_entry(entry, name, from_scope) {
                                let score = best_cand.specificity;
                                match &best {
                                    Some((_, best_score)) if *best_score >= score => {}
                                    _ => best = Some((symbol, score)),
                                }
                            }
                        }
                    }
                }
            }
        }
        best.map(|(sym, _)| sym)
    }

    /// Pick the best overload candidate for a bare name call
    pub fn pick_best_overload_for_bare_name(
        &self,
        name: &str,
        overload_ctx: &OverloadContext,
    ) -> Option<EntityId> {
        let overload_sets = self.get_overload_sets_by_name(name);
        if overload_sets.is_empty() {
            return None;
        }
        let arg_types: Vec<Option<&TypeShape>> = overload_ctx
            .arg_types
            .as_ref()
            .map(|v| v.iter().map(|o| o.as_ref()).collect())
            .unwrap_or_default();
        let effective_arg_types = if !arg_types.is_empty() {
            arg_types
        } else if let Some(count) = overload_ctx.arg_count {
            vec![None; count]
        } else {
            Vec::new()
        };
        let mut best_candidate: Option<(EntityId, crate::type_inference::overload::OverloadScore)> =
            None;
        for set in &overload_sets {
            let candidate = if effective_arg_types.is_empty() {
                set.resolve(&[])
            } else {
                set.resolve_with_args(&effective_arg_types, &std::collections::HashMap::new())
            };
            if let Some(cand) = candidate {
                let score = cand.score(
                    &effective_arg_types,
                    None,
                    cce_types::language::Language::Unknown,
                );
                match &best_candidate {
                    None => best_candidate = Some((cand.entity_id, score)),
                    Some((_, best_score)) => {
                        if score.better_than(best_score) {
                            best_candidate = Some((cand.entity_id, score));
                        }
                    }
                }
            } else if let Some(best) = set.resolve_with_score(
                &effective_arg_types,
                None,
                cce_types::language::Language::Unknown,
            ) {
                let score = best.score(
                    &effective_arg_types,
                    None,
                    cce_types::language::Language::Unknown,
                );
                match &best_candidate {
                    None => best_candidate = Some((best.entity_id, score)),
                    Some((_, best_score)) => {
                        if score.better_than(best_score) {
                            best_candidate = Some((best.entity_id, score));
                        }
                    }
                }
            }
        }
        best_candidate.map(|(id, _)| id)
    }

    /// Build a symbol ref from a simple-name index entry, resolving the
    /// entry's own file/module instead of treating the stored file path as a
    /// package id
    fn symbol_from_entry(
        &self,
        entry: &SimpleNameEntry,
        name: &str,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        match entry {
            SimpleNameEntry::FileSymbol {
                file_path,
                module_path,
                ..
            } => {
                let try_resolve =
                    |package: &Arc<package::PackageSymbolTable>| -> Option<SymbolRef> {
                        let module = package.get_module(file_path)?;
                        let found = match from_scope {
                            Some(scope) => module.get_export_visible_from(name, scope),
                            None => module.get_export(name),
                        };
                        found.map(|metadata| self.symbol_ref_for(metadata, module_path))
                    };
                if let Some(symbol) = self
                    .get_package_for_file(file_path)
                    .and_then(|pkg| try_resolve(&pkg))
                {
                    return Some(symbol);
                }
                for package in self.sorted_packages() {
                    if let Some(symbol) = try_resolve(&package) {
                        return Some(symbol);
                    }
                }
                None
            }
            SimpleNameEntry::PackageExport { package_id, .. } => {
                let package = self.get_package(package_id)?;
                let metadata = package.get_public_export(name)?;
                Some(self.stable_symbol_ref(&metadata))
            }
        }
    }

    /// Stable symbol ref for a target symbol: caches the EntityId per
    /// (name, defining file, module path) so the same target always resolves
    /// to the same id
    pub(crate) fn stable_symbol_ref(&self, metadata: &SymbolMetadata) -> SymbolRef {
        let module_path = self.module_path_for_file(&metadata.location.file_path);
        self.symbol_ref_for(metadata, &module_path)
    }

    /// Module path of the module table that owns `file_path`, if any.
    fn module_path_for_file(&self, file_path: &str) -> String {
        if let Some(package) = self.get_package_for_file(file_path) {
            if let Some(module) = package.get_module(file_path) {
                return module.module_path.clone();
            }
        }
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(file_path) {
                return module.module_path.clone();
            }
        }
        String::new()
    }

    /// Scope context of the module table that owns `file_path`, if any.
    ///
    /// Used as the caller's scope when enforcing visibility on Level 1-3
    /// resolution. `None` when the caller file is not indexed (unknown
    /// caller scope), in which case resolution stays permissive.
    pub(crate) fn caller_scope(&self, file_path: &str) -> Option<ScopeContext> {
        if let Some(package) = self.get_package_for_file(file_path) {
            if let Some(module) = package.get_module(file_path) {
                return Some(module.scope_context());
            }
        }
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(file_path) {
                return Some(module.scope_context());
            }
        }
        None
    }

    /// Resolve a symbol across packages
    ///
    /// Checks visibility based on the requesting package
    pub fn resolve_cross_package(&self, name: &str, from_package: &str) -> Option<SymbolRef> {
        // 1. Check this package first
        if let Some(package) = self.get_package(from_package) {
            if let Some(metadata) = package.get_export(name) {
                return Some(self.stable_symbol_ref(&metadata));
            }
        }

        // 2. Check other packages (public exports only, deterministic order)
        for package in self.sorted_packages() {
            if package.package_id != from_package {
                if let Some(metadata) = package.get_public_export(name) {
                    return Some(self.stable_symbol_ref(&metadata));
                }
            }
        }

        // 3. Check external dependencies
        self.resolve_external(name)
    }

    /// Resolve a symbol in external dependencies (deterministic order)
    pub(crate) fn resolve_external(&self, name: &str) -> Option<SymbolRef> {
        for dep in self.sorted_external_deps() {
            if let Some(metadata) = dep.get_export(name) {
                return Some(self.stable_symbol_ref(metadata));
            }
        }

        None
    }
}
