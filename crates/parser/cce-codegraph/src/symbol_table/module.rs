//! Module symbol table (module-level)
//!
//! Manages imports and exports for a single module,
//! combining FileExports and ImportTable functionality.

use crate::symbol::{ScopeContext, SymbolMetadata, SymbolRef, Visibility};
use crate::symbol_table::local::LocalSymbolTable;
use crate::symbol_table::type_index::TypeMemberIndex;
use cce_types::NamespacePath;
use cce_types::entity::EntityId;
use cce_types::import::StandardizedImportTable;
use cce_types::language::Language;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Module symbol table - combines imports and exports
#[derive(Debug, Clone)]
pub struct ModuleSymbolTable {
    /// Decomposed module path (namespace segments + leaf module name).
    pub namespace_path: NamespacePath,

    /// Module path (e.g., "cce_utils::helpers")
    pub module_path: String,

    /// File path
    pub file_path: String,

    /// Language
    pub language: Language,

    /// Package name
    pub package: String,

    /// Exports by visibility level
    exports: ModuleExportMap,

    /// Local symbol table for file-level scope resolution
    local_table: Option<LocalSymbolTable>,

    /// Import bindings (local name -> binding)
    imports: DashMap<String, ImportBinding>,

    /// Reexports (local name -> reexport info)
    reexports: DashMap<String, ReexportBinding>,

    /// Wildcard imports (for expansion)
    wildcard_imports: Vec<WildcardImport>,

    /// Import table (original standardized imports)
    import_table: StandardizedImportTable,

    /// Type-member index for this module
    type_index: TypeMemberIndex,
}

/// Export map organized by visibility
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModuleExportMap {
    public: HashMap<String, SymbolMetadata>,
    package: HashMap<String, SymbolMetadata>,
    module: HashMap<String, SymbolMetadata>,
    restricted: Vec<(String, SymbolMetadata)>, // (restriction_path, metadata)
}

/// Import binding with resolution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportBinding {
    /// Local name
    pub local_name: String,

    /// Source path
    pub source_path: String,

    /// Imported symbol name (None for wildcard)
    pub symbol_name: Option<String>,

    /// Resolved symbol reference
    pub resolved_symbol: Option<SymbolRef>,

    /// Whether this is a wildcard import
    pub is_wildcard: bool,

    /// Import source type
    pub source_type: ImportSourceType,

    /// Optional condition for conditional imports (e.g., "cfg(target_os = \"linux\")")
    pub condition: Option<String>,
}

/// Import source type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportSourceType {
    /// Internal module import (within the same package)
    InternalModule {
        /// Module path
        path: String,
    },
    /// External package dependency
    ExternalDependency {
        /// Package name
        package: String,
    },
    /// Standard library import
    StandardLibrary {
        /// Language
        lang: Language,
    },
    /// Conditional import (e.g., Rust `#[cfg(...)]`, Python `try/except`)
    Conditional {
        /// Condition expression (e.g., "target_os = \"linux\"", "ImportError")
        condition: String,
    },
    /// Dynamic import (e.g., JavaScript `import()`, Python `importlib.import_module`)
    Dynamic {
        /// The dynamic import expression or module path
        expression: String,
    },
    /// Unknown source
    Unknown,
}

/// Reexport binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReexportBinding {
    /// Local name
    pub local_name: String,

    /// Original module path
    pub original_module: String,

    /// Original symbol name
    pub original_name: String,

    /// Chain depth
    pub chain_depth: u8,

    /// Resolved symbol
    pub resolved_symbol: Option<SymbolRef>,
}

/// Wildcard import tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildcardImport {
    /// Source module path
    pub source_module: String,

    /// Resolved symbols from this wildcard
    pub resolved_symbols: Vec<SymbolRef>,
}

/// Strip a leading `crate` segment from a Rust-style module path.
///
/// `determine_module_path` no longer emits the `crate::` prefix, so import
/// sources and qualified names that still carry it (`use crate::a::b::c`)
/// must be normalized to the index key form (`a::b::c`).
pub fn strip_crate_prefix(path: &str) -> &str {
    path.strip_prefix("crate::").unwrap_or(path)
}

impl ModuleSymbolTable {
    /// Create a new module symbol table from a decomposed namespace path.
    pub fn new(
        namespace_path: impl Into<NamespacePath>,
        file_path: String,
        language: Language,
        package: String,
    ) -> Self {
        let namespace_path = namespace_path.into();
        let module_path = namespace_path.qualified();
        Self {
            namespace_path,
            module_path: module_path.clone(),
            file_path: file_path.clone(),
            language,
            package,
            exports: ModuleExportMap::default(),
            local_table: None,
            imports: DashMap::new(),
            reexports: DashMap::new(),
            wildcard_imports: Vec::new(),
            import_table: StandardizedImportTable::new(file_path),
            type_index: TypeMemberIndex::new(),
        }
    }

    /// Compatibility constructor that accepts a raw module_path string.
    ///
    /// Parses the path into a [`NamespacePath`] via `NamespacePath::parse`.
    pub fn new_with_module_path(
        module_path: String,
        file_path: String,
        language: Language,
        package: String,
    ) -> Self {
        let ns = NamespacePath::parse(&module_path);
        Self::new(ns, file_path, language, package)
    }

    // === Export Operations ===

    /// Add an export
    ///
    /// Only symbols with an export-relevant visibility enter the export map;
    /// private/internal visibility levels are deliberately not exported.
    /// The passed visibility is stamped into the metadata so downstream
    /// consumers (visibility checks, snapshot records) see the export's real
    /// level instead of the `SymbolMetadata` default.
    pub fn add_export(
        &mut self,
        name: String,
        mut metadata: SymbolMetadata,
        visibility: Visibility,
    ) {
        metadata.visibility = visibility.clone();
        match visibility {
            Visibility::Public => self.exports.public.insert(name, metadata),
            Visibility::Package => self.exports.package.insert(name, metadata),
            Visibility::Module => self.exports.module.insert(name, metadata),
            Visibility::Restricted { path } => {
                self.exports.restricted.push((path, metadata));
                None
            }
            // First-phase exported levels that behave like package-scoped:
            // `Protected`/`Internal`/`ProtectedInternal` are package-visible
            // (protected also needs subclass check, internal is assembly/package).
            // They are stored in the package bucket but retain their original
            // visibility in metadata for accurate `is_visible_from` checks.
            Visibility::Protected | Visibility::Internal | Visibility::ProtectedInternal => {
                self.exports.package.insert(name, metadata)
            }
            // Private, Super, PrivateProtected and Friend are not visible outside
            // their defining scope and must not become public exports.
            _ => None,
        };
    }

    /// Get an export by name, restricted to symbols visible from a scope.
    ///
    /// Resolution paths use this instead of [`Self::get_export`] so
    /// package/module-level exports are only resolvable from scopes the
    /// language model allows. Exports whose definition context is unknown
    /// (empty package and no module path) are treated as visible, preserving
    /// the previous permissive behavior for foreign metadata.
    pub fn get_export_visible_from(
        &self,
        name: &str,
        from_scope: &ScopeContext,
    ) -> Option<&SymbolMetadata> {
        let defined_in = self.scope_context();
        if defined_in.package.is_empty() && defined_in.module_path.is_none() {
            return self.get_export(name);
        }
        let visible = |metadata: &SymbolMetadata| {
            metadata
                .visibility
                .is_visible_from(from_scope, &defined_in, self.language)
        };
        self.exports
            .public
            .get(name)
            .filter(|m| visible(m))
            .or_else(|| self.exports.package.get(name).filter(|m| visible(m)))
            .or_else(|| self.exports.module.get(name).filter(|m| visible(m)))
            .or_else(|| {
                self.exports
                    .restricted
                    .iter()
                    .find(|(_, m)| m.name.as_ref() == name)
                    .map(|(_, m)| m)
                    .filter(|m| visible(m))
            })
    }

    /// Get an export by name (any visibility)
    pub fn get_export(&self, name: &str) -> Option<&SymbolMetadata> {
        self.exports
            .public
            .get(name)
            .or_else(|| self.exports.package.get(name))
            .or_else(|| self.exports.module.get(name))
            .or_else(|| {
                self.exports
                    .restricted
                    .iter()
                    .find(|(_, m)| m.name.as_ref() == name)
                    .map(|(_, m)| m)
            })
    }

    /// Get export with specific visibility
    pub fn get_export_with_visibility(
        &self,
        name: &str,
        visibility: Visibility,
    ) -> Option<&SymbolMetadata> {
        match visibility {
            Visibility::Public => self.exports.public.get(name),
            Visibility::Package => self.exports.package.get(name),
            Visibility::Module => self.exports.module.get(name),
            _ => None,
        }
    }

    /// Check if this module exports a symbol
    pub fn has_export(&self, name: &str) -> bool {
        self.get_export(name).is_some()
    }

    /// Get all public exports
    pub fn public_exports(&self) -> &HashMap<String, SymbolMetadata> {
        &self.exports.public
    }

    /// Get all exports (all visibilities)
    pub fn all_exports(&self) -> Vec<(&String, &SymbolMetadata)> {
        let mut result: Vec<(&String, &SymbolMetadata)> = Vec::new();
        result.extend(self.exports.public.iter());
        result.extend(self.exports.package.iter());
        result.extend(self.exports.module.iter());
        // For restricted exports, we need to convert Arc<str> to String reference
        // This requires a different approach since we can't return reference to temporary
        for (_, _m) in self.exports.restricted.iter() {
            // Skip restricted exports in this context since they have Arc<str> names
            // and we need to return &String references
        }
        result
    }

    /// Get exports visible from a scope
    pub fn exports_visible_from(
        &self,
        from_scope: &ScopeContext,
    ) -> Vec<(&String, &SymbolMetadata)> {
        let language = self.language;
        self.all_exports()
            .into_iter()
            .filter(|(_, metadata)| {
                metadata.visibility.is_visible_from(
                    from_scope,
                    &metadata.location.to_scope_context(),
                    language,
                )
            })
            .collect()
    }

    // === Import Operations ===

    /// Add an import binding
    pub fn add_import(&self, binding: ImportBinding) {
        self.imports.insert(binding.local_name.clone(), binding);
    }

    /// Get import binding by local name
    pub fn get_import(&self, local_name: &str) -> Option<ImportBinding> {
        self.imports.get(local_name).map(|b| b.clone())
    }

    /// Check if a name is imported
    pub fn is_imported(&self, local_name: &str) -> bool {
        self.imports.contains_key(local_name)
    }

    /// Resolve an import
    pub fn resolve_import(&self, local_name: &str, symbol: SymbolRef) {
        if let Some(mut binding) = self.imports.get_mut(local_name) {
            binding.resolved_symbol = Some(symbol);
        }
    }

    /// Get all imports
    pub fn all_imports(&self) -> Vec<ImportBinding> {
        self.imports.iter().map(|b| b.clone()).collect()
    }

    /// Get unresolved imports
    pub fn unresolved_imports(&self) -> Vec<ImportBinding> {
        self.imports
            .iter()
            .filter(|b| b.resolved_symbol.is_none())
            .map(|b| b.clone())
            .collect()
    }

    // === Reexport Operations ===

    /// Add a reexport
    pub fn add_reexport(&self, binding: ReexportBinding) {
        self.reexports.insert(binding.local_name.clone(), binding);
    }

    /// Get reexport by local name
    pub fn get_reexport(&self, local_name: &str) -> Option<ReexportBinding> {
        self.reexports.get(local_name).map(|b| b.clone())
    }

    /// Get all reexports
    pub fn all_reexports(&self) -> Vec<ReexportBinding> {
        self.reexports.iter().map(|b| b.clone()).collect()
    }

    /// Cache a resolved symbol for a reexport binding
    pub fn resolve_reexport(&self, local_name: &str, symbol: SymbolRef) {
        if let Some(mut binding) = self.reexports.get_mut(local_name) {
            binding.resolved_symbol = Some(symbol);
        }
    }

    // === Wildcard Import Operations ===

    /// Add a wildcard import
    pub fn add_wildcard_import(&mut self, source_module: String) {
        self.wildcard_imports.push(WildcardImport {
            source_module,
            resolved_symbols: Vec::new(),
        });
    }

    /// Get all wildcard imports
    pub fn wildcard_imports(&self) -> &[WildcardImport] {
        &self.wildcard_imports
    }

    /// Resolve symbols for a wildcard import
    pub fn resolve_wildcard(&mut self, source_module: &str, symbols: Vec<SymbolRef>) {
        if let Some(wildcard) = self
            .wildcard_imports
            .iter_mut()
            .find(|w| w.source_module == source_module)
        {
            wildcard.resolved_symbols = symbols;
        }
    }

    /// Get all symbols from wildcard imports
    pub fn wildcard_symbols(&self) -> Vec<&SymbolRef> {
        self.wildcard_imports
            .iter()
            .flat_map(|w| &w.resolved_symbols)
            .collect()
    }

    // === Combined Lookup ===

    /// Look up a name in local exports only
    ///
    /// This method only checks local exports, not imports or reexports.
    /// For full resolution, use ProjectSymbolTable.resolve_enhanced()
    pub fn lookup_local(&self, name: &str) -> Option<&SymbolMetadata> {
        self.get_export(name)
    }

    /// Look up a resolved import
    pub fn lookup_import(&self, name: &str) -> Option<SymbolRef> {
        if let Some(binding) = self.get_import(name) {
            return binding.resolved_symbol;
        }
        None
    }

    /// Look up a resolved reexport
    pub fn lookup_reexport(&self, name: &str) -> Option<SymbolRef> {
        if let Some(binding) = self.get_reexport(name) {
            return binding.resolved_symbol;
        }
        None
    }

    /// Look up a wildcard import symbol
    pub fn lookup_wildcard(&self, name: &str) -> Option<SymbolRef> {
        for wildcard in &self.wildcard_imports {
            if let Some(symbol) = wildcard.resolved_symbols.iter().find(|s| s.name() == name) {
                return Some(symbol.clone());
            }
        }
        None
    }

    /// Get scope context for this module
    pub fn scope_context(&self) -> ScopeContext {
        ScopeContext::with_module(&self.file_path, &self.package, &self.module_path)
    }

    /// Set import table
    pub fn set_import_table(&mut self, table: StandardizedImportTable) {
        self.import_table = table;
    }

    /// Get import table
    pub fn import_table(&self) -> &StandardizedImportTable {
        &self.import_table
    }

    // === Local Symbol Table Integration ===

    /// Set the local symbol table for file-level scope resolution
    pub fn set_local_table(&mut self, table: LocalSymbolTable) {
        self.local_table = Some(table);
    }

    /// Get reference to the local symbol table
    pub fn get_local_table(&self) -> Option<&LocalSymbolTable> {
        self.local_table.as_ref()
    }

    /// Resolve a name using the local scope chain (inner-to-outer).
    ///
    /// Delegates to LocalSymbolTable.resolve_by_scope for proper
    /// name shadowing resolution. Returns None if:
    /// - No local table is set
    /// - The scope chain is empty
    /// - No matching entity is found
    pub fn resolve_local_scope(&self, name: &str, scope_chain: &[EntityId]) -> Option<SymbolRef> {
        let local = self.local_table.as_ref()?;
        if scope_chain.is_empty() {
            return None;
        }
        let entity = local.resolve_by_scope(name, scope_chain)?;
        local.create_symbol_ref(entity.id)
    }

    /// Build scope chain for an entity within this module.
    ///
    /// Returns an ordered list from outermost (root) to innermost (the entity).
    /// Returns empty Vec if no local table is set.
    pub fn get_scope_chain(&self, entity_id: EntityId, max_depth: usize) -> Vec<EntityId> {
        self.local_table
            .as_ref()
            .map(|local| local.build_scope_chain_with_limit(entity_id, max_depth))
            .unwrap_or_default()
    }

    // === TypeMemberIndex Operations ===

    pub fn type_index(&self) -> &TypeMemberIndex {
        &self.type_index
    }

    pub fn type_index_mut(&mut self) -> &mut TypeMemberIndex {
        &mut self.type_index
    }

    pub fn set_type_index(&mut self, index: TypeMemberIndex) {
        self.type_index = index;
    }
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
        let mut table = ModuleSymbolTable::new(
            "cce_utils".to_string(),
            "src/utils.rs".to_string(),
            Language::Rust,
            "test".to_string(),
        );

        let meta = create_test_metadata("public_func");
        table.add_export("public_func".to_string(), meta.clone(), Visibility::Public);

        assert!(table.has_export("public_func"));
        assert!(!table.has_export("private_func"));

        let found = table.get_export("public_func");
        assert!(found.is_some());
    }

    #[test]
    fn test_visibility_levels() {
        let mut table = ModuleSymbolTable::new(
            "cce_utils".to_string(),
            "src/utils.rs".to_string(),
            Language::Rust,
            "test".to_string(),
        );

        table.add_export(
            "pub".to_string(),
            create_test_metadata("pub"),
            Visibility::Public,
        );
        table.add_export(
            "pkg".to_string(),
            create_test_metadata("pkg"),
            Visibility::Package,
        );
        table.add_export(
            "mod".to_string(),
            create_test_metadata("mod"),
            Visibility::Module,
        );

        assert_eq!(table.public_exports().len(), 1);
        assert!(table.public_exports().contains_key("pub"));
    }

    #[test]
    fn test_import_binding() {
        let table = ModuleSymbolTable::new(
            "crate::main".to_string(),
            "src/main.rs".to_string(),
            Language::Rust,
            "test".to_string(),
        );

        let binding = ImportBinding {
            local_name: "HashMap".to_string(),
            source_path: "std::collections".to_string(),
            symbol_name: Some("HashMap".to_string()),
            resolved_symbol: None,
            is_wildcard: false,
            source_type: ImportSourceType::StandardLibrary {
                lang: Language::Rust,
            },
            condition: None,
        };

        table.add_import(binding);

        assert!(table.is_imported("HashMap"));
        assert!(!table.is_imported("Vec"));

        let found = table.get_import("HashMap");
        assert!(found.is_some());
    }
}
