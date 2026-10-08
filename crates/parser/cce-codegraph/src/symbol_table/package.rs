//! Package symbol table (package/crate-level)
//!
//! Manages exports within a package/crate, handling visibility
//! across modules within the same package.

use crate::symbol::SymbolMetadata;
use crate::symbol::scope::ScopeContext;
use cce_types::language::Language;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Whether `file_path` is a package init file (`__init__.py` / `__init__.pyi`).
fn is_package_init_file(file_path: &str) -> bool {
    let name = cce_types::path::file_name_str(file_path);
    name == "__init__.py" || name == "__init__.pyi"
}

/// Package symbol table - manages package-level exports
#[derive(Debug, Clone)]
pub struct PackageSymbolTable {
    /// Package ID
    pub package_id: String,

    /// Package name
    pub package_name: String,

    /// Package path (root directory)
    pub package_path: String,

    /// Language (primary language of the package)
    pub language: Language,

    /// Public exports (visible to other packages)
    public_exports: DashMap<String, SymbolMetadata>,

    /// Internal exports (visible within package only)
    internal_exports: DashMap<String, SymbolMetadata>,

    /// Module symbol tables (keyed by file path)
    ///
    /// Shared via `Arc` so resolution paths clone a refcount instead of
    /// deep-cloning the whole module table per lookup
    modules: DashMap<String, Arc<super::module::ModuleSymbolTable>>,

    /// Module path to file path mapping
    module_path_index: DashMap<String, String>,

    /// Per-file contribution of public exports for incremental diff.
    ///
    /// `file_path -> set of export names contributed by that file's last
    /// module table`. Used by `add_module` to compute a delta and update
    /// `public_exports` without a full scan.
    module_export_contrib: DashMap<String, HashSet<String>>,

    /// Namespace index: namespace_name -> Set<module_file_path>
    namespace_modules: DashMap<String, HashSet<String>>,
}

/// Incremental delta produced by `PackageSymbolTable::add_module_incremental`.
#[derive(Debug, Clone, Default)]
pub struct PackageDelta {
    /// Export names that disappeared from this file's module.
    pub removed: Vec<String>,
    /// Export names that are present in the new module (added or updated).
    pub added: Vec<String>,
    /// Whether the module path collided.
    pub path_collision: bool,
}

impl PackageSymbolTable {
    /// Create a new package symbol table
    pub fn new(
        package_id: String,
        package_name: String,
        package_path: String,
        language: Language,
    ) -> Self {
        Self {
            package_id,
            package_name: package_name.clone(),
            package_path,
            language,
            public_exports: DashMap::new(),
            internal_exports: DashMap::new(),
            modules: DashMap::new(),
            module_path_index: DashMap::new(),
            module_export_contrib: DashMap::new(),
            namespace_modules: DashMap::new(),
        }
    }

    /// Get all modules in a namespace.
    pub fn modules_in_namespace(
        &self,
        namespace: &str,
    ) -> Vec<Arc<super::module::ModuleSymbolTable>> {
        let file_paths = self
            .namespace_modules
            .get(namespace)
            .map(|s| s.clone())
            .unwrap_or_default();
        file_paths
            .iter()
            .filter_map(|fp| self.get_module(fp))
            .collect()
    }

    /// Register a module's namespace contribution.
    fn register_namespace(&self, ns: &str, file_path: &str) {
        self.namespace_modules
            .entry(ns.to_string())
            .or_default()
            .insert(file_path.to_string());
    }

    /// Remove a file's namespace contributions (for incremental updates).
    fn unregister_namespace_for_file(&self, file_path: &str) {
        let mut empty_keys = Vec::new();
        for mut entry in self.namespace_modules.iter_mut() {
            entry.value_mut().remove(file_path);
            if entry.value().is_empty() {
                empty_keys.push(entry.key().clone());
            }
        }
        for k in empty_keys {
            self.namespace_modules.remove(&k);
        }
    }

    // === Export Management ===

    /// Add a public export
    pub fn add_public_export(&self, name: String, metadata: SymbolMetadata) {
        self.public_exports.insert(name, metadata);
    }

    /// Add an internal export
    pub fn add_internal_export(&self, name: String, metadata: SymbolMetadata) {
        self.internal_exports.insert(name, metadata);
    }

    /// Get a public export
    pub fn get_public_export(&self, name: &str) -> Option<SymbolMetadata> {
        self.public_exports.get(name).map(|s| s.clone())
    }

    /// Get an internal export
    pub fn get_internal_export(&self, name: &str) -> Option<SymbolMetadata> {
        self.internal_exports.get(name).map(|s| s.clone())
    }

    /// Get any export (public or internal)
    pub fn get_export(&self, name: &str) -> Option<SymbolMetadata> {
        self.get_public_export(name)
            .or_else(|| self.get_internal_export(name))
    }

    /// Check if a symbol is exported publicly
    pub fn is_public_export(&self, name: &str) -> bool {
        self.public_exports.contains_key(name)
    }

    /// Get all public export names
    pub fn public_export_names(&self) -> Vec<String> {
        self.public_exports
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Get all public exports
    pub fn public_exports(&self) -> Vec<SymbolMetadata> {
        self.public_exports.iter().map(|e| e.clone()).collect()
    }

    /// Get all internal exports
    pub fn internal_exports(&self) -> Vec<SymbolMetadata> {
        self.internal_exports.iter().map(|e| e.clone()).collect()
    }

    // === Module Management ===

    /// Add a module symbol table.
    ///
    /// Returns `true` when the module's path collides with a different
    /// file's module path  Collisions are never silently overwritten:
    /// a package `__init__` file wins over a same-named module file
    /// (`utils/__init__.py` over `utils.py`), otherwise the first
    /// registration wins. Callers should surface the collision through a
    /// warning and the `module_path_conflicts` metric.
    pub fn add_module(&self, module: super::module::ModuleSymbolTable) -> bool {
        // Non-incremental legacy path: insert module and defer export
        // aggregation to an explicit `rebuild_exports` call.
        let file_path = module.file_path.clone();
        let module_path = module.module_path.clone();
        let namespace_prefix = module.namespace_path.namespace_prefix();

        // Remove old namespace contributions for this file if it already exists
        if self.modules.contains_key(&file_path) {
            self.unregister_namespace_for_file(&file_path);
        }

        self.modules.insert(file_path.clone(), Arc::new(module));

        if let Some(ns) = namespace_prefix {
            self.register_namespace(&ns, &file_path);
            // Also register each prefix segment for hierarchical lookup
            let segments: Vec<&str> = ns.split("::").collect();
            for i in 1..segments.len() {
                let prefix = segments[..i].join("::");
                self.register_namespace(&prefix, &file_path);
            }
        }

        if let Some(existing) = self.module_path_index.get(&module_path) {
            if existing.value() == &file_path {
                return false;
            }
            let is_init = is_package_init_file(&file_path);
            tracing::warn!(
                module_path,
                existing_file = existing.value(),
                new_file = file_path,
                rule = if is_init { "init-wins" } else { "first-wins" },
                "module path collision; keeping the deterministic winner"
            );
            if is_init {
                self.module_path_index.insert(module_path, file_path);
            }
            return true;
        }

        self.module_path_index.insert(module_path, file_path);
        false
    }

    /// Add a module and incrementally maintain `public_exports` via a
    /// per-file contribution map.
    ///
    /// This is the O(F + affected) path: only the supplied module's
    /// contribution is diffed against its previous contribution and
    /// `public_exports` is patched in place. `rebuild_exports` is retained as
    /// a fallback for full rebuilds and consistency checks.
    pub fn add_module_incremental(&self, module: super::module::ModuleSymbolTable) -> PackageDelta {
        let file_path = module.file_path.clone();
        let module_path = module.module_path.clone();
        let namespace_prefix = module.namespace_path.namespace_prefix();

        // Capture new contribution before moving `module`.
        let new_exports: HashMap<String, SymbolMetadata> = module
            .public_exports()
            .iter()
            .map(|(k, v)| (k.clone(), (*v).clone()))
            .collect();
        let new_set: HashSet<String> = new_exports.keys().cloned().collect();

        let old_set: HashSet<String> = self
            .module_export_contrib
            .get(&file_path)
            .map(|s| s.clone())
            .unwrap_or_default();

        let removed: Vec<String> = old_set.difference(&new_set).cloned().collect();

        // Insert/replace the module itself.
        self.modules.insert(file_path.clone(), Arc::new(module));

        // Module path index handling (same as `add_module`).
        let path_collision = if let Some(existing) = self.module_path_index.get(&module_path) {
            if existing.value() == &file_path {
                false
            } else {
                let is_init = is_package_init_file(&file_path);
                tracing::warn!(
                    module_path,
                    existing_file = existing.value(),
                    new_file = file_path.clone(),
                    rule = if is_init { "init-wins" } else { "first-wins" },
                    "module path collision; keeping the deterministic winner"
                );
                if is_init {
                    self.module_path_index
                        .insert(module_path.clone(), file_path.clone());
                }
                true
            }
        } else {
            self.module_path_index
                .insert(module_path.clone(), file_path.clone());
            false
        };

        // Update namespace index: remove old contributions for this file then register new.
        self.unregister_namespace_for_file(&file_path);
        if let Some(ns) = &namespace_prefix {
            self.register_namespace(ns, &file_path);
            let segments: Vec<&str> = ns.split("::").collect();
            for i in 1..segments.len() {
                let prefix = segments[..i].join("::");
                self.register_namespace(&prefix, &file_path);
            }
        }

        // Patch public_exports: handle removals first.
        for name in &removed {
            let still_exported = self
                .module_export_contrib
                .iter()
                .any(|entry| entry.key() != &file_path && entry.value().contains(name));
            if still_exported {
                // Another file still exports this name; restore its metadata
                // so the survivor wins deterministically.
                let mut replacement: Option<SymbolMetadata> = None;
                for entry in self.modules.iter() {
                    if entry.key() == &file_path {
                        continue;
                    }
                    if let Some(meta) = entry.value().public_exports().get(name) {
                        replacement = Some((*meta).clone());
                        break;
                    }
                }
                if let Some(meta) = replacement {
                    self.public_exports.insert(name.clone(), meta);
                }
                // else: inconsistent state, keep existing entry
            } else {
                self.public_exports.remove(name);
            }
        }

        // Upsert all new/updated exports.
        for (name, meta) in &new_exports {
            self.public_exports.insert(name.clone(), meta.clone());
        }

        // Update contribution map last so the removal check above sees the
        // old state of other files only.
        self.module_export_contrib
            .insert(file_path.clone(), new_set.clone());

        PackageDelta {
            removed,
            added: new_set.into_iter().collect(),
            path_collision,
        }
    }

    /// Get a module by file path (shares the stored table via `Arc`)
    pub fn get_module(&self, file_path: &str) -> Option<Arc<super::module::ModuleSymbolTable>> {
        self.modules.get(file_path).map(|m| Arc::clone(m.value()))
    }

    /// Get a module by module path (shares the stored table via `Arc`)
    pub fn get_module_by_path(
        &self,
        module_path: &str,
    ) -> Option<Arc<super::module::ModuleSymbolTable>> {
        self.module_path_index
            .get(module_path)
            .and_then(|file_path| self.get_module(&file_path))
    }

    /// Resolve a module reference with a deterministic fallback chain.
    ///
    /// Import sources are not always exact module paths: JS/C/C++ extractors
    /// may store relative sources (`./utils`, `"util.h"`) or suffix-less
    /// paths when no module path could be derived. Resolution order:
    ///
    /// 1. exact module_path match in the package's module path index
    /// 2. relative-to-caller: `./x`, `../x` resolved against the caller
    ///    file's own module path
    /// 3. deterministic path-suffix match: the module whose path ends with
    ///    the given reference, longest suffix first (ties broken by file
    ///    path). Only consulted when the reference has no `::` separators,
    ///    so multi-segment Rust paths never match ambiguously.
    ///
    /// `caller_file` is the importing file, used only for step 2.
    pub fn resolve_module_path(
        &self,
        module_path: &str,
        caller_file: Option<&str>,
    ) -> Option<Arc<super::module::ModuleSymbolTable>> {
        let stripped = super::module::strip_crate_prefix(module_path);
        if let Some(module) = self.get_module_by_path(stripped) {
            return Some(module);
        }

        // Step 2: relative references resolved against the caller's module
        // path (the caller module's own path is its nearest package dir).
        // Also handles Rust's `super::` prefix by converting to `../`.
        if let Some(caller_file) = caller_file {
            // Convert Rust's `super::` prefix to `../` for relative path resolution
            let normalized = if let Some(rest) = stripped.strip_prefix("super::") {
                if rest.is_empty() {
                    "..".to_string()
                } else {
                    format!("../{}", rest.replace("::", "/"))
                }
            } else if let Some(rest) = stripped.strip_prefix("self::") {
                if rest.is_empty() {
                    ".".to_string()
                } else {
                    rest.replace("::", "/")
                }
            } else {
                stripped.to_string()
            };

            if normalized.starts_with("./") || normalized.starts_with("../") {
                let caller_module_path = self
                    .get_module(caller_file)
                    .map(|m| m.module_path.clone())
                    .unwrap_or_default();
                if !caller_module_path.is_empty() {
                    let mut segments: Vec<&str> = caller_module_path
                        .split(['/', '\\', ':'])
                        .filter(|s| !s.is_empty())
                        .collect();
                    for part in normalized.split('/') {
                        match part {
                            "." | "" => {}
                            ".." => {
                                segments.pop();
                            }
                            seg => segments.push(seg),
                        }
                    }
                    let resolved = segments.join("/");
                    if let Some(module) = self.get_module_by_path(&resolved) {
                        return Some(module);
                    }
                }
            }
        }

        // Step 3: deterministic suffix match for unqualified references
        // (`util.h`, `utils`). Longest matching module path wins; ties are
        // broken by file path so results are reproducible.
        if !stripped.contains("::") && !stripped.is_empty() {
            let mut candidates: Vec<(usize, String)> = Vec::new();
            for entry in self.module_path_index.iter() {
                let path = entry.key();
                let suffix_len = path.split(['/', '\\', ':', '.']).count();
                if path.ends_with(&stripped) {
                    candidates.push((suffix_len, path.clone()));
                }
            }
            candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            for (_, path) in candidates {
                if let Some(module) = self.get_module_by_path(&path) {
                    return Some(module);
                }
            }
        }

        None
    }

    /// Get all modules (shared references)
    pub fn all_modules(&self) -> Vec<Arc<super::module::ModuleSymbolTable>> {
        self.modules.iter().map(|m| Arc::clone(m.value())).collect()
    }

    /// Check if a module exists
    pub fn has_module(&self, file_path: &str) -> bool {
        self.modules.contains_key(file_path)
    }

    /// Get module count
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    // === Resolution ===

    /// Resolve a symbol within the package (basic lookup only)
    ///
    /// This method only checks package-level exports.
    /// For full resolution, use ProjectSymbolTable.resolve_enhanced()
    pub fn resolve_in_package(&self, name: &str, from_module: &str) -> Option<SymbolMetadata> {
        // 1. Check public exports (always accessible)
        if let Some(metadata) = self.get_public_export(name) {
            return Some(metadata);
        }

        // 2. Check if from_module is in this package
        if self.has_module(from_module) {
            // Same package - can access internal exports
            if let Some(metadata) = self.get_internal_export(name) {
                return Some(metadata);
            }
        }

        None
    }

    /// Resolve a symbol with full path (module::symbol) - basic lookup
    ///
    /// `from_scope` (the caller's scope context) restricts module-local
    /// lookups to exports visible from that scope. Package public exports are
    /// always accessible and skip the visibility check.
    pub fn resolve_qualified(
        &self,
        qualified_name: &str,
        from_scope: Option<&ScopeContext>,
    ) -> Option<SymbolMetadata> {
        let parts: Vec<&str> = qualified_name.split("::").collect();
        if parts.is_empty() {
            return None;
        }

        if parts.len() == 1 {
            // Simple name - check exports
            return self.get_export(parts[0]);
        }

        // Qualified path: module::symbol or module::submodule::symbol
        let module_path = parts[..parts.len() - 1].join("::");
        let symbol_name = parts.last()?;

        let module = self.resolve_module_path(&module_path, None)?;
        let found = match from_scope {
            Some(scope) => module.get_export_visible_from(symbol_name, scope),
            None => module.lookup_local(symbol_name),
        };
        found.cloned()
    }

    /// Find all symbols matching a pattern
    pub fn find_matching(&self, pattern: &str) -> Vec<SymbolMetadata> {
        let pattern_lower = pattern.to_lowercase();
        let mut results = Vec::new();

        // Search public exports
        for export in self.public_exports.iter() {
            if export.name.to_lowercase().contains(&pattern_lower) {
                results.push(export.clone());
            }
        }

        // Search internal exports
        for export in self.internal_exports.iter() {
            if export.name.to_lowercase().contains(&pattern_lower) {
                results.push(export.clone());
            }
        }

        results
    }

    /// Get the public API surface of this package
    pub fn public_api(&self) -> HashMap<String, SymbolMetadata> {
        self.public_exports
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// Calculate API surface statistics
    pub fn api_stats(&self) -> PackageApiStats {
        let mut stats = PackageApiStats::default();

        for export in self.public_exports.iter() {
            stats.total_public += 1;
            match export.kind {
                cce_types::entity::EntityKind::Function => stats.public_functions += 1,
                cce_types::entity::EntityKind::Struct | cce_types::entity::EntityKind::Class => {
                    stats.public_types += 1
                }
                _ => {}
            }
        }

        for _export in self.internal_exports.iter() {
            stats.total_internal += 1;
        }

        stats
    }

    /// Rebuild exports from module tables
    ///
    /// Aggregates all public exports from modules and repopulates the
    /// incremental contribution map so a subsequent
    /// `add_module_incremental` remains consistent.
    pub fn rebuild_exports(&self) {
        self.public_exports.clear();
        self.internal_exports.clear();
        self.module_export_contrib.clear();
        self.namespace_modules.clear();

        for module in self.modules.iter() {
            let file_path = module.key().clone();
            let mut contrib = HashSet::new();
            // Collect public exports
            for (name, metadata) in module.value().public_exports() {
                self.add_public_export(name.clone(), metadata.clone());
                contrib.insert(name.clone());
            }
            self.module_export_contrib
                .insert(file_path.clone(), contrib);
            if let Some(ns) = module.value().namespace_path.namespace_prefix() {
                self.register_namespace(&ns, &file_path);
                let segments: Vec<&str> = ns.split("::").collect();
                for i in 1..segments.len() {
                    let prefix = segments[..i].join("::");
                    self.register_namespace(&prefix, &file_path);
                }
            }
        }
    }
}

/// Package API statistics
#[derive(Debug, Clone, Default)]
pub struct PackageApiStats {
    /// Total public exports
    pub total_public: usize,

    /// Total internal exports
    pub total_internal: usize,

    /// Public functions
    pub public_functions: usize,

    /// Public types (structs, classes, enums, etc.)
    pub public_types: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolLocation;
    use cce_types::Span;
    use cce_types::entity::EntityKind;

    fn create_test_metadata(name: &str) -> SymbolMetadata {
        let location = SymbolLocation::new(
            "src/lib.rs".to_string(),
            Span {
                start_byte: 0,
                end_byte: 10,
                start_position: Default::default(),
                end_position: Default::default(),
            },
            Language::Rust,
        );
        SymbolMetadata::new(name.to_string(), EntityKind::Function, location)
    }

    #[test]
    fn test_add_and_get_export() {
        let package = PackageSymbolTable::new(
            "pkg-1".to_string(),
            "my-crate".to_string(),
            "/project".to_string(),
            Language::Rust,
        );

        let metadata = create_test_metadata("public_func");
        package.add_public_export("public_func".to_string(), metadata);

        assert!(package.is_public_export("public_func"));
        assert!(!package.is_public_export("private_func"));

        let found = package.get_public_export("public_func");
        assert!(found.is_some());
    }

    #[test]
    fn test_public_vs_internal() {
        let package = PackageSymbolTable::new(
            "pkg-1".to_string(),
            "my-crate".to_string(),
            "/project".to_string(),
            Language::Rust,
        );

        package.add_public_export("pub".to_string(), create_test_metadata("pub"));
        package.add_internal_export("internal".to_string(), create_test_metadata("internal"));

        // Both should be findable with get_export
        assert!(package.get_export("pub").is_some());
        assert!(package.get_export("internal").is_some());

        // Only public should be in public exports
        assert_eq!(package.public_exports().len(), 1);
        assert_eq!(package.internal_exports().len(), 1);
    }

    #[test]
    fn test_api_stats() {
        let package = PackageSymbolTable::new(
            "pkg-1".to_string(),
            "my-crate".to_string(),
            "/project".to_string(),
            Language::Rust,
        );

        package.add_public_export("func1".to_string(), create_test_metadata("func1"));
        package.add_public_export("func2".to_string(), create_test_metadata("func2"));
        package.add_internal_export("internal".to_string(), create_test_metadata("internal"));

        let stats = package.api_stats();
        assert_eq!(stats.total_public, 2);
        assert_eq!(stats.total_internal, 1);
    }
}
