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

use crate::symbol::scope::ScopeContext;
use std::sync::Arc;

use super::qualified_resolution::OverloadContext;
use super::{ProjectSymbolTable, ResolutionContext};
use crate::symbol::SymbolRef;
use crate::symbol_table::module;
use crate::symbol_table::package;
impl ProjectSymbolTable {
    /// Resolve a symbol with enhanced features
    ///
    /// This method provides the unified resolution entry point:
    /// - Level 0: Local scope chain resolution (inner-to-outer, handles shadowing)
    /// - Level 1: Direct/qualified name lookup
    /// - Level 2: Import resolution via module tables
    /// - Level 3: Re-export resolution via module tables
    /// - Level 4: Cross-package resolution
    pub fn resolve_enhanced(&self, name: &str, context: &ResolutionContext) -> Option<SymbolRef> {
        self.resolve_enhanced_inner(name, context, true)
    }

    /// Resolve a symbol with enhanced features, skipping the last-segment
    /// fallback.
    ///
    /// Used for stdlib targets: a qualified stdlib name (`Vec::new`,
    /// `console.log`) must never fall back to a project-internal symbol that
    /// happens to share its last segment.
    pub fn resolve_enhanced_strict(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<SymbolRef> {
        self.resolve_enhanced_inner(name, context, false)
    }

    pub fn resolve_enhanced_with_overload(
        &self,
        name: &str,
        context: &ResolutionContext,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        self.resolve_enhanced_inner_with_overload(name, context, true, overload_ctx)
    }

    /// Shared implementation for enhanced resolution.
    ///
    /// This method provides the unified resolution entry point:
    /// - Level 0: Local scope chain resolution (inner-to-outer, handles shadowing)
    /// - Level 1: Direct/qualified name lookup
    /// - Level 2.5: Type-member resolution (Type.member / Type::member)
    /// - Level 2: Import resolution via module tables
    /// - Level 3: Re-export resolution via module tables
    /// - Level 4: Cross-package resolution
    fn resolve_enhanced_inner(
        &self,
        name: &str,
        context: &ResolutionContext,
        allow_last_segment_fallback: bool,
    ) -> Option<SymbolRef> {
        // Caller scope for visibility enforcement; `None` when the caller
        // file is not indexed (resolution stays permissive).
        let from_scope = self.caller_scope(&context.file_path);

        // Level 0: Local scope chain resolution (inner-to-outer, handles shadowing)
        if !context.scope_chain.is_empty() {
            if let Some(symbol) = self.resolve_via_local_scope(name, context) {
                return Some(symbol);
            }
        }

        // Alias precedence: a bare name bound by the caller module's import
        // table (including aliases like `import { foo as bar }`) shadows every
        // global simple-name match. Resolve through the import binding before
        // the Level 1 qualified/simple search so an unrelated export of the
        // same name cannot hijack the alias. Qualified names (containing `::`
        // or `.`) are never import local names and skip this step.
        if !name.contains(':') && !name.contains('.') {
            if let Some(symbol) = self.resolve_via_module_import(name, context, from_scope.as_ref())
            {
                return Some(symbol);
            }
        }

        // Level 1: Direct/qualified name lookup (cache contextualized per caller
        // file). For qualified names (containing . or ::), type-member
        // resolution runs first so receiver-qualified calls (Foo::bar) are
        // dispatched through the type index before the last-segment fallback
        // can match an unrelated bare name.
        let qualified = name.contains('.') || name.contains("::");
        if qualified {
            if let Some(symbol) = self.resolve_via_type_member(name, context, from_scope.as_ref()) {
                return Some(symbol);
            }
        }
        if let Some(symbol) = self.resolve_qualified_cached(
            name,
            Some(&context.file_path),
            allow_last_segment_fallback,
            from_scope.as_ref(),
        ) {
            return Some(symbol);
        }
        if !qualified {
            if let Some(symbol) = self.resolve_via_type_member(name, context, from_scope.as_ref()) {
                return Some(symbol);
            }
        }

        // Level 2: Import resolution via module tables
        if let Some(symbol) = self.resolve_via_module_import(name, context, from_scope.as_ref()) {
            return Some(symbol);
        }

        // Level 3: Re-export resolution via module tables
        if let Some(symbol) = self.resolve_via_module_reexport(name, context, from_scope.as_ref()) {
            return Some(symbol);
        }

        // Level 4: Cross-package resolution
        if let Some(symbol) = self.resolve_via_cross_package(name, context) {
            return Some(symbol);
        }

        None
    }

    fn resolve_enhanced_inner_with_overload(
        &self,
        name: &str,
        context: &ResolutionContext,
        allow_last_segment_fallback: bool,
        overload_ctx: Option<&OverloadContext>,
    ) -> Option<SymbolRef> {
        let from_scope = self.caller_scope(&context.file_path);
        if !context.scope_chain.is_empty() {
            if let Some(symbol) = self.resolve_via_local_scope(name, context) {
                return Some(symbol);
            }
        }
        if !name.contains(':') && !name.contains('.') {
            if let Some(symbol) = self.resolve_via_module_import(name, context, from_scope.as_ref())
            {
                return Some(symbol);
            }
        }
        let qualified = name.contains('.') || name.contains("::");
        if qualified {
            if let Some(symbol) = self.resolve_via_type_member(name, context, from_scope.as_ref()) {
                return Some(symbol);
            }
        }
        if let Some(symbol) = self.resolve_qualified_cached_with_overload(
            name,
            Some(&context.file_path),
            allow_last_segment_fallback,
            from_scope.as_ref(),
            overload_ctx,
        ) {
            return Some(symbol);
        }
        if !qualified {
            if let Some(symbol) = self.resolve_via_type_member(name, context, from_scope.as_ref()) {
                return Some(symbol);
            }
        }
        if let Some(symbol) = self.resolve_via_module_import(name, context, from_scope.as_ref()) {
            return Some(symbol);
        }
        if let Some(symbol) = self.resolve_via_module_reexport(name, context, from_scope.as_ref()) {
            return Some(symbol);
        }
        if let Some(symbol) = self.resolve_via_cross_package(name, context) {
            return Some(symbol);
        }
        None
    }

    /// Resolve via local scope chain (Level 0)
    ///
    /// Uses the scope chain from the resolution context to resolve
    /// symbols from inner-to-outer scope, correctly handling name shadowing.
    /// Falls through to higher levels if no local table is available.
    fn resolve_via_local_scope(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<SymbolRef> {
        if let Some(package) = self.get_package_for_file(&context.file_path) {
            if let Some(module) = package.get_module(&context.file_path) {
                return module.resolve_local_scope(name, &context.scope_chain);
            }
        }
        // Fallback to deterministic scan when file->package mapping is stale.
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(&context.file_path) {
                if let Some(symbol) = module.resolve_local_scope(name, &context.scope_chain) {
                    return Some(symbol);
                }
            }
        }
        None
    }

    /// Resolve via cross-package lookup (Level 4)
    ///
    /// Checks all packages' public exports (deterministic order) and
    /// external dependencies.
    fn resolve_via_cross_package(
        &self,
        name: &str,
        _context: &ResolutionContext,
    ) -> Option<SymbolRef> {
        // Check all packages' public exports
        for package in self.sorted_packages() {
            if let Some(metadata) = package.get_public_export(name) {
                return Some(self.stable_symbol_ref(&metadata));
            }
        }

        // Check external dependencies
        self.resolve_external(name)
    }

    /// Resolve via module import (Level 2)
    ///
    /// Looks up the caller module's import bindings, resolving them lazily
    /// against the owning package's module tables when they carry no cached
    /// result (import bindings are now populated by the symbol table
    /// builder instead of being dead code).
    fn resolve_via_module_import(
        &self,
        name: &str,
        context: &ResolutionContext,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        if let Some(package) = self.get_package_for_file(&context.file_path) {
            if let Some(module) = package.get_module(&context.file_path) {
                if let Some(symbol) =
                    self.resolve_import_binding(&module, &package, name, from_scope)
                {
                    return Some(symbol);
                }
                if let Some(symbol) =
                    self.resolve_wildcard_binding(&module, &package, name, from_scope)
                {
                    return Some(symbol);
                }
                return None;
            }
        }
        // Fallback to deterministic scan when file->package mapping is stale.
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(&context.file_path) {
                if let Some(symbol) =
                    self.resolve_import_binding(&module, &package, name, from_scope)
                {
                    return Some(symbol);
                }
                if let Some(symbol) =
                    self.resolve_wildcard_binding(&module, &package, name, from_scope)
                {
                    return Some(symbol);
                }
            }
        }
        None
    }

    /// Resolve an import binding for `name`, caching the result on first hit.
    fn resolve_import_binding(
        &self,
        module: &module::ModuleSymbolTable,
        package: &Arc<package::PackageSymbolTable>,
        name: &str,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        let binding = module.get_import(name)?;
        if let Some(symbol) = binding.resolved_symbol {
            return Some(symbol);
        }
        if binding.is_wildcard {
            return None;
        }
        let symbol =
            self.resolve_binding_source(&binding, package, Some(&module.file_path), from_scope)?;
        module.resolve_import(name, symbol.clone());
        Some(symbol)
    }

    /// Resolve a wildcard import binding for `name` by expanding its source
    /// module's exports on demand.
    ///
    /// Uses a per-file wildcard expansion cache to avoid re-expanding the same
    /// wildcard on every lookup. The cache is invalidated when modules are
    /// added or removed.
    fn resolve_wildcard_binding(
        &self,
        module: &module::ModuleSymbolTable,
        package: &Arc<package::PackageSymbolTable>,
        name: &str,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        for wildcard in module.wildcard_imports() {
            // Check the module's pre-resolved symbols first
            if let Some(symbol) = wildcard.resolved_symbols.iter().find(|s| s.name() == name) {
                return Some(symbol.clone());
            }

            let source_module =
                crate::symbol_table::module::strip_crate_prefix(&wildcard.source_module);

            // Check the project-level wildcard expansion cache
            let cache_key = (module.file_path.clone(), source_module.to_string());
            if let Some(cached_symbols) = self.wildcard_expansion_cache.get(&cache_key) {
                if let Some(symbol) = cached_symbols.iter().find(|s| s.name() == name) {
                    return Some(symbol.clone());
                }
                // Symbol not found in this wildcard's expansion, continue to next
                continue;
            }

            // Cache miss: expand the wildcard and cache all symbols
            let target = package.resolve_module_path(source_module, Some(&module.file_path));
            if let Some(target) = target {
                // Expand all visible exports from the source module
                let expanded_symbols: Vec<SymbolRef> = match from_scope {
                    Some(scope) => target
                        .exports_visible_from(scope)
                        .into_iter()
                        .map(|(_, metadata)| self.stable_symbol_ref(metadata))
                        .collect(),
                    None => target
                        .all_exports()
                        .into_iter()
                        .map(|(_, metadata)| self.stable_symbol_ref(metadata))
                        .collect(),
                };

                // Cache the expanded symbols
                self.wildcard_expansion_cache
                    .insert(cache_key, expanded_symbols.clone());

                // Look up the requested name in the expanded symbols
                if let Some(symbol) = expanded_symbols.iter().find(|s| s.name() == name) {
                    return Some(symbol.clone());
                }
            }
        }
        None
    }

    /// Resolve the target of an import binding.
    ///
    /// Rust import sources are path-shaped (`std::collections::HashMap`,
    /// `crate::a::b::c`), Python/JS sources are module-shaped
    /// (`os`, `myapp.utils`). When the source's last `::` segment equals the
    /// binding's symbol name the source is split into (module, symbol);
    /// otherwise the source is the module and the binding carries the symbol.
    ///
    /// The source module is resolved through [`PackageSymbolTable::resolve_module_path`],
    /// whose deterministic fallback (exact module_path → relative-to-caller →
    /// path-suffix match) makes C/C++ `#include "util.h"` and JS relative
    /// imports resolvable when the extractor could not derive a module path.
    fn resolve_binding_source(
        &self,
        binding: &module::ImportBinding,
        package: &Arc<package::PackageSymbolTable>,
        caller_file: Option<&str>,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        let source = module::strip_crate_prefix(&binding.source_path);
        let (module_path, symbol_name) = if let Some(symbol) = binding.symbol_name.as_deref() {
            match source.rsplit_once("::") {
                Some((prefix, last)) if last == symbol => (prefix.to_string(), symbol.to_string()),
                _ => (source.to_string(), symbol.to_string()),
            }
        } else {
            source
                .rsplit_once("::")
                .map(|(prefix, symbol)| (prefix.to_string(), symbol.to_string()))?
        };
        let target = package.resolve_module_path(&module_path, caller_file)?;
        let found = match from_scope {
            Some(scope) => target.get_export_visible_from(&symbol_name, scope),
            None => target.lookup_local(&symbol_name),
        };
        let metadata = found?.clone();
        Some(self.stable_symbol_ref(&metadata))
    }

    /// Resolve via module re-export (Level 3)
    ///
    /// Looks up the caller module's re-export bindings and resolves them
    /// lazily against the owning package's module tables, caching the
    /// result on first hit. Chained re-exports (a re-export of a
    /// re-export) recurse with a bumped depth, capped at
    /// `SymbolResolutionConfig::max_reexport_chain_depth` to bound work and break cycles.
    fn resolve_via_module_reexport(
        &self,
        name: &str,
        context: &ResolutionContext,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        if let Some(package) = self.get_package_for_file(&context.file_path) {
            if let Some(module) = package.get_module(&context.file_path) {
                if let Some(binding) = module.get_reexport(name) {
                    if let Some(symbol) = binding.resolved_symbol {
                        return Some(symbol);
                    }
                    let symbol = self.resolve_reexport_target(&binding, &package, 1, from_scope)?;
                    module.resolve_reexport(name, symbol.clone());
                    return Some(symbol);
                }
                return None;
            }
        }
        // Fallback to deterministic scan when file->package mapping is stale.
        for package in self.sorted_packages() {
            if let Some(module) = package.get_module(&context.file_path) {
                if let Some(binding) = module.get_reexport(name) {
                    if let Some(symbol) = binding.resolved_symbol {
                        return Some(symbol);
                    }
                    let symbol = self.resolve_reexport_target(&binding, &package, 1, from_scope)?;
                    module.resolve_reexport(name, symbol.clone());
                    return Some(symbol);
                }
            }
        }
        None
    }

    /// Resolve the target of a re-export binding.
    ///
    /// The binding's `original_module` is looked up as a module path (Rust
    /// style, `crate::` prefix stripped); the original symbol is then
    /// resolved inside that module's exports. When the original module
    /// itself only re-exports the name, the chain is followed recursively.
    ///
    /// `hops` counts the re-export bindings followed so far (including the
    /// current one); chains longer than `SymbolResolutionConfig::max_reexport_chain_depth` hops
    /// are truncated, which also breaks cycles. A binding whose own
    /// `chain_depth` already exceeds the cap (produced by a chain-aware
    /// producer) is never resolvable.
    fn resolve_reexport_target(
        &self,
        binding: &module::ReexportBinding,
        package: &Arc<package::PackageSymbolTable>,
        hops: u8,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        let max_depth = self.max_reexport_chain_depth() as u8;
        if binding.chain_depth > max_depth {
            return None;
        }
        if hops > max_depth {
            return None;
        }
        let module_path = module::strip_crate_prefix(&binding.original_module);
        let target = package.resolve_module_path(module_path, None)?;
        // Return a previously-resolved symbol immediately; cached results
        // do not consume additional hop budget.
        if let Some(cached) = target.get_reexport(&binding.original_name) {
            if let Some(symbol) = cached.resolved_symbol {
                return Some(symbol);
            }
        }
        // Enforce the hop cap: once the budget is exhausted, do not attempt
        // further local lookups or chain-following.
        if hops >= max_depth {
            return None;
        }
        // Try a direct (local) lookup in the target module.
        let found = match from_scope {
            Some(scope) => target.get_export_visible_from(&binding.original_name, scope),
            None => target.lookup_local(&binding.original_name),
        };
        if let Some(metadata) = found {
            return Some(self.stable_symbol_ref(metadata));
        }
        // The original module may re-export the name again; follow the
        // chain until the hop cap is reached.
        let next = target.get_reexport(&binding.original_name)?;
        let symbol = self.resolve_reexport_target(&next, package, hops + 1, from_scope)?;
        target.resolve_reexport(&binding.original_name, symbol.clone());
        Some(symbol)
    }

    fn resolve_via_type_member(
        &self,
        name: &str,
        context: &ResolutionContext,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolRef> {
        // Determine caller language from module table
        let language = self
            .get_package_for_file(&context.file_path)
            .and_then(|pkg| pkg.get_module(&context.file_path).map(|m| m.language))
            .or_else(|| {
                self.sorted_packages()
                    .iter()
                    .find_map(|pkg| pkg.get_module(&context.file_path).map(|m| m.language))
            })
            .unwrap_or(cce_types::language::Language::Unknown);
        let scope = match from_scope {
            Some(s) => s.clone(),
            None => self
                .caller_scope(&context.file_path)
                .unwrap_or_else(|| ScopeContext::new(&context.file_path, "")),
        };
        // Split qualified name into type part and member part
        let (type_part, member_part) = Self::split_type_member(name)?;
        if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
            metrics.type_member_lookup_total.increment();
        }
        let member = {
            let global = self.global_type_index.read().ok()?;
            global.resolve_qualified(type_part, member_part, &scope, language)
        };
        if let Some(member) = member {
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.type_member_hit_total.increment();
            }
            // Synthesize a symbol ref from the member entry
            let location = {
                let mut loc = crate::symbol::SymbolLocation::new(
                    member.file_path.clone(),
                    member.span,
                    language,
                );
                if !member.package.is_empty() {
                    loc = loc.with_package(member.package.clone());
                }
                if let Some(mp) = &member.module_path {
                    loc = loc.with_module(mp.clone());
                }
                loc
            };
            let metadata =
                crate::symbol::SymbolMetadata::new(member.name.clone(), member.kind, location)
                    .with_visibility(member.visibility.clone());
            let symbol_ref =
                self.symbol_ref_for(&metadata, member.module_path.as_deref().unwrap_or(""));
            return Some(symbol_ref);
        } else {
            if let Some(metrics) = self.metrics_sink.read().ok().and_then(|g| g.clone()) {
                metrics.type_member_miss_total.increment();
            }
        }
        None
    }

    fn split_type_member(name: &str) -> Option<(&str, &str)> {
        // Prefer last occurrence of :: or .
        // For mixed separators, take the last of either
        let last_double_colon = name.rfind("::").map(|p| (p, 2usize));
        let last_dot = name.rfind('.').map(|p| (p, 1usize));
        // Determine which is later
        let (pos, sep_len) = match (last_double_colon, last_dot) {
            (Some((p1, l1)), Some((p2, l2))) => {
                if p1 > p2 {
                    (p1, l1)
                } else {
                    (p2, l2)
                }
            }
            (Some(v), None) => v,
            (None, Some(v)) => v,
            (None, None) => return None,
        };
        let type_part = &name[..pos];
        let member_part = &name[pos + sep_len..];
        if type_part.is_empty() || member_part.is_empty() {
            return None;
        }
        // filter trivial receivers
        let lower = type_part.to_ascii_lowercase();
        if lower == "self" || lower == "this" || lower == "super" || lower == "self::super" {
            return None;
        }
        Some((type_part, member_part))
    }
}
