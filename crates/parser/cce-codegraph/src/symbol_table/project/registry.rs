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

use super::{ExternalSymbolTable, ProjectSymbolTable, SimpleNameEntry};
use crate::symbol_table::package;
use crate::symbol_table::package::PackageDelta;
use cce_types::normalize_project_path;
impl ProjectSymbolTable {
    /// Add a package (shared via `Arc`)
    pub fn add_package(&self, package: Arc<package::PackageSymbolTable>) {
        let package_id = package.package_id.clone();

        let mut affected_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        // Index package exports with stable symbol ids
        for name in package.public_export_names() {
            affected_names.insert(name.clone());
            if let Some(metadata) = package.get_public_export(&name) {
                let symbol_ref = self.stable_symbol_ref(&metadata);
                let qualified_name = format!("{}::{}", package.package_name, name);
                self.global_index
                    .insert(qualified_name, symbol_ref.symbol_id);

                // Index by simple name (dedup so repeated add_package calls
                // for the same package do not accumulate entries)
                if let Some(mut entries) = self.simple_name_index.get_mut(&name) {
                    entries.retain(|entry| match entry {
                        SimpleNameEntry::PackageExport {
                            package_id: pid, ..
                        } => pid != &package_id,
                        SimpleNameEntry::FileSymbol { .. } => true,
                    });
                }
                self.insert_simple_name_entry(
                    &name,
                    SimpleNameEntry::PackageExport {
                        package_id: package_id.clone(),
                        entity_id: symbol_ref.symbol_id,
                    },
                );
            }
        }

        self.packages_by_name
            .insert(package.package_name.clone(), Arc::clone(&package));
        self.packages
            .insert(package_id.clone(), Arc::clone(&package));
        for module in package.all_modules() {
            let normalized = normalize_project_path(&module.file_path);
            self.file_to_package.insert(normalized, package_id.clone());
        }
        // Fine-grained invalidation: only names exported by this package
        // may have changed resolution results.
        self.invalidate_cache_for_names(&affected_names);
        self.invalidate_sorted_packages();
    }

    /// Incrementally apply a package export delta without scanning all
    /// package exports.
    ///
    /// Only names in `delta.removed` / `delta.added` are touched,
    /// giving O(affected) cost instead of O(package_exports).
    pub fn apply_package_delta(
        &self,
        package: Arc<package::PackageSymbolTable>,
        delta: &PackageDelta,
    ) {
        let package_id = package.package_id.clone();
        let mut affected: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Removals: if the name is still exported by another file in the
        // same package, treat it as an update (upsert survivor metadata);
        // otherwise delete the global entries for this package.
        for name in &delta.removed {
            affected.insert(name.clone());
            if let Some(metadata) = package.get_public_export(name) {
                // Survivor after incremental package patch — upsert.
                let symbol_ref = self.stable_symbol_ref(&metadata);
                let qualified_name = format!("{}::{}", package.package_name, name);
                self.global_index
                    .insert(qualified_name, symbol_ref.symbol_id);
                if let Some(mut entries) = self.simple_name_index.get_mut(name) {
                    entries.retain(|entry| match entry {
                        SimpleNameEntry::PackageExport {
                            package_id: pid, ..
                        } => pid != &package_id,
                        SimpleNameEntry::FileSymbol { .. } => true,
                    });
                }
                self.insert_simple_name_entry(
                    name,
                    SimpleNameEntry::PackageExport {
                        package_id: package_id.clone(),
                        entity_id: symbol_ref.symbol_id,
                    },
                );
            } else {
                // Fully removed by this incremental step.
                let qualified_name = format!("{}::{}", package.package_name, name);
                self.global_index.remove(&qualified_name);
                if let Some(mut entries) = self.simple_name_index.get_mut(name) {
                    entries.retain(|entry| match entry {
                        SimpleNameEntry::PackageExport {
                            package_id: pid, ..
                        } => pid != &package_id,
                        SimpleNameEntry::FileSymbol { .. } => true,
                    });
                    if entries.is_empty() {
                        drop(entries);
                        self.simple_name_index.remove(name);
                    }
                }
            }
        }

        // Additions / updates: upsert each name present in the new module.
        for name in &delta.added {
            affected.insert(name.clone());
            if let Some(metadata) = package.get_public_export(name) {
                let symbol_ref = self.stable_symbol_ref(&metadata);
                let qualified_name = format!("{}::{}", package.package_name, name);
                self.global_index
                    .insert(qualified_name, symbol_ref.symbol_id);
                if let Some(mut entries) = self.simple_name_index.get_mut(name) {
                    entries.retain(|entry| match entry {
                        SimpleNameEntry::PackageExport {
                            package_id: pid, ..
                        } => pid != &package_id,
                        SimpleNameEntry::FileSymbol { .. } => true,
                    });
                }
                self.insert_simple_name_entry(
                    name,
                    SimpleNameEntry::PackageExport {
                        package_id: package_id.clone(),
                        entity_id: symbol_ref.symbol_id,
                    },
                );
            }
        }

        self.packages_by_name
            .insert(package.package_name.clone(), Arc::clone(&package));
        self.packages
            .insert(package_id.clone(), Arc::clone(&package));
        for module in package.all_modules() {
            let normalized = normalize_project_path(&module.file_path);
            self.file_to_package.insert(normalized, package_id.clone());
        }
        self.invalidate_cache_for_names(&affected);
        self.invalidate_sorted_packages();
    }

    /// Get a package (shares the stored table via `Arc`)
    pub fn get_package(&self, package_id: &str) -> Option<Arc<package::PackageSymbolTable>> {
        self.packages.get(package_id).map(|p| Arc::clone(p.value()))
    }

    /// Get a package by name (shares the stored table via `Arc`)
    ///
    /// Backed by the `packages_by_name` index built at `add_package` time
    /// instead of scanning the full package set per lookup
    pub fn get_package_by_name(&self, name: &str) -> Option<Arc<package::PackageSymbolTable>> {
        self.packages_by_name
            .get(name)
            .map(|p| Arc::clone(p.value()))
    }

    /// Get all packages (shared references)
    pub fn all_packages(&self) -> Vec<Arc<package::PackageSymbolTable>> {
        self.packages
            .iter()
            .map(|p| Arc::clone(p.value()))
            .collect()
    }

    /// Check if a package exists
    pub fn has_package(&self, package_id: &str) -> bool {
        self.packages.contains_key(package_id)
    }

    // === External Dependency Management ===

    pub fn add_external_dep(&self, table: ExternalSymbolTable) {
        let affected: std::collections::HashSet<String> =
            table.all_exports().keys().cloned().collect();
        let package_name = table.package_name.clone();
        let had_affected = !affected.is_empty();
        self.external_deps
            .insert(package_name.clone(), Arc::new(table));
        if had_affected {
            self.invalidate_cache_for_names(&affected);
        }
        self.invalidate_sorted_external();
        if affected.is_empty() {
            self.negative_cache
                .retain(|(_, qn), _| !qn.contains(package_name.as_str()));
        }
    }

    /// Get an external dependency (shares the stored table via `Arc`)
    pub fn get_external_dep(&self, name: &str) -> Option<Arc<ExternalSymbolTable>> {
        self.external_deps.get(name).map(|d| Arc::clone(d.value()))
    }

    /// Get all external dependencies (shared references)
    pub fn all_external_deps(&self) -> Vec<Arc<ExternalSymbolTable>> {
        self.external_deps
            .iter()
            .map(|d| Arc::clone(d.value()))
            .collect()
    }
}
