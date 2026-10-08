//! External library handling for symbol resolution
//!
//! Provides language-aware import resolution strategies and handlers for
//! external libraries (C/C++ headers, Python packages, JavaScript modules).
//! All dispatch is static via [`Language`] enumeration, without `dyn` indirection.

pub mod header;
pub mod javascript;
pub mod loader;
pub mod provider;
pub mod python;

use cce_types::language::Language;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::symbol::{SymbolLocation, SymbolMetadata, Visibility};
use crate::symbol_table::project::ExternalSymbolTable;
use cce_types::Span;
use cce_types::entity::EntityKind;

/// Strategy for resolving imported symbols, varying by language's module system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportResolutionStrategy {
    /// Rust `use` - symbol-level imports (e.g. `use std::collections::HashMap`)
    SymbolLevel,
    /// Python `import` - module-level imports (e.g. `import os`)
    ModuleLevel,
    /// Java / C# `import` - package-level imports (e.g. `import java.util.*`)
    PackageLevel,
    /// C / C++ `#include` - header-level inclusion
    HeaderLevel,
    /// JavaScript / TypeScript ES modules - `import`/`export` system
    ESModuleLevel,
}

impl ImportResolutionStrategy {
    /// Determine the strategy for a given language.
    pub fn for_language(language: Language) -> Self {
        match language {
            Language::Rust => Self::SymbolLevel,
            Language::Python => Self::ModuleLevel,
            Language::Java | Language::Kotlin | Language::Scala | Language::CSharp => {
                Self::PackageLevel
            }
            Language::Cpp | Language::C => Self::HeaderLevel,
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                Self::ESModuleLevel
            }
            Language::Go => Self::PackageLevel,
            Language::Php | Language::Ruby => Self::ModuleLevel,
            Language::Dart => Self::PackageLevel,
            _ => Self::ModuleLevel,
        }
    }

    /// Returns true if the strategy resolves individual symbols directly.
    pub fn is_symbol_level(self) -> bool {
        matches!(self, Self::SymbolLevel)
    }

    /// Returns true if the strategy resolves at header inclusion granularity.
    pub fn is_header_level(self) -> bool {
        matches!(self, Self::HeaderLevel)
    }
}

impl std::fmt::Display for ImportResolutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::SymbolLevel => "SymbolLevel",
            Self::ModuleLevel => "ModuleLevel",
            Self::PackageLevel => "PackageLevel",
            Self::HeaderLevel => "HeaderLevel",
            Self::ESModuleLevel => "ESModuleLevel",
        };
        write!(f, "{label}")
    }
}

/// Depth of external library resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImportResolutionDepth {
    /// Only parse the import statements themselves.
    #[default]
    ImportStatement,
    /// Parse the imported module or header.
    ImportedModule,
    /// Parse module dependencies recursively.
    ModuleDependencies,
}

/// Kind of external module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleType {
    /// Compiled library
    Library,
    /// C/C++ header
    Header,
    /// Python type stub (.pyi)
    TypeStub,
    /// NPM / package manager package
    Package,
    /// Local path dependency
    Local,
}

/// Exported symbol from an external library.
#[derive(Debug, Clone)]
pub struct ExportedSymbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: EntityKind,
    /// Visibility - external symbols are treated as Public
    pub visibility: Visibility,
    /// Optional source file
    pub source_file: Option<String>,
}

impl ExportedSymbol {
    /// Create a new exported symbol with public visibility.
    pub fn new(name: impl Into<String>, kind: EntityKind) -> Self {
        Self {
            name: name.into(),
            kind,
            visibility: Visibility::Public,
            source_file: None,
        }
    }

    pub fn with_source_file(mut self, path: impl Into<String>) -> Self {
        self.source_file = Some(path.into());
        self
    }
}

/// Aggregated information about an external module.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Module name
    pub name: String,
    /// Module file or package directory
    pub path: PathBuf,
    /// Exported symbols
    pub exports: Vec<ExportedSymbol>,
    /// Direct dependencies
    pub dependencies: Vec<String>,
    /// Module type
    pub module_type: ModuleType,
    /// Language
    pub language: Language,
}

impl ModuleInfo {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        language: Language,
        module_type: ModuleType,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            exports: Vec::new(),
            dependencies: Vec::new(),
            module_type,
            language,
        }
    }

    /// Convert into an [`ExternalSymbolTable`] suitable for registration in [`crate::symbol_table::ProjectSymbolTable`].
    pub fn into_external_table(self, version: Option<String>) -> ExternalSymbolTable {
        let mut table = ExternalSymbolTable::new(self.name.clone(), version, self.language);
        for exported in self.exports {
            let location = SymbolLocation::new(
                exported
                    .source_file
                    .clone()
                    .unwrap_or_else(|| self.path.to_string_lossy().to_string()),
                Span::default(),
                self.language,
            );
            let metadata = SymbolMetadata::new(exported.name.clone(), exported.kind, location)
                .with_visibility(exported.visibility);
            table.add_export(exported.name, metadata);
        }
        table
    }
}

/// Registry that coordinates external library resolution without dynamic dispatch.
///
/// Each language-specific handler is invoked through a static `match Language`
/// dispatch. No `Box<dyn _>` is used.
#[derive(Debug, Default)]
pub struct ExternalLibraryRegistry {
    resolution_depth: ImportResolutionDepth,
    module_cache: HashMap<String, ModuleInfo>,
}

impl ExternalLibraryRegistry {
    pub fn new() -> Self {
        Self {
            resolution_depth: ImportResolutionDepth::default(),
            module_cache: HashMap::new(),
        }
    }

    pub fn with_depth(mut self, depth: ImportResolutionDepth) -> Self {
        self.resolution_depth = depth;
        self
    }

    pub fn resolution_depth(&self) -> ImportResolutionDepth {
        self.resolution_depth
    }

    /// Resolve a library located at `library_path` for the given language.
    ///
    /// When `resolution_depth` is `ImportStatement`, only the import declaration
    /// would be considered; for `ImportedModule` and above, the handler parses
    /// the library contents. Failures are returned as `crate::error::RelationError`.
    pub fn resolve_library(
        &mut self,
        library_path: &Path,
        language: Language,
    ) -> Result<ModuleInfo, crate::error::RelationError> {
        let strategy = ImportResolutionStrategy::for_language(language);
        let cache_key = format!("{}:{}", language, library_path.display());
        if let Some(cached) = self.module_cache.get(&cache_key) {
            return Ok(cached.clone());
        }
        let info = match strategy {
            ImportResolutionStrategy::HeaderLevel => {
                let mut handler = header::HeaderFileHandler::new();
                handler.parse_header(library_path, language)
            }
            ImportResolutionStrategy::ModuleLevel if language == Language::Python => {
                let mut handler = python::PythonLibraryHandler::new();
                handler.parse_package(library_path, language)
            }
            ImportResolutionStrategy::ESModuleLevel => {
                let mut handler = javascript::JavaScriptModuleHandler::new();
                handler.parse_package(library_path, language)
            }
            ImportResolutionStrategy::PackageLevel
            | ImportResolutionStrategy::ModuleLevel
            | ImportResolutionStrategy::SymbolLevel => {
                // Generic package-level handling: treat directory as package,
                // collect top-level file names as exports when no specialized
                // handler applies.
                self.resolve_generic_package(library_path, language)
            }
        }?;
        self.module_cache.insert(cache_key, info.clone());
        Ok(info)
    }

    /// Register a resolved module into a project symbol table.
    pub fn register_into_project(
        &mut self,
        library_path: &Path,
        language: Language,
        project: &crate::symbol_table::ProjectSymbolTable,
        version: Option<String>,
    ) -> Result<(), crate::error::RelationError> {
        let module_info = self.resolve_library(library_path, language)?;
        let table = module_info.into_external_table(version);
        project.add_external_dep(table);
        Ok(())
    }

    /// Expand a wildcard / re-export import if the source module is an external library.
    ///
    /// This is used when Level 2 import resolution encounters an external package
    /// import such as `from numpy import *`.
    pub fn exports_of(&self, library_name: &str) -> Option<Vec<String>> {
        self.module_cache
            .values()
            .find(|m| m.name == library_name)
            .map(|m| m.exports.iter().map(|e| e.name.clone()).collect())
    }

    fn resolve_generic_package(
        &self,
        library_path: &Path,
        language: Language,
    ) -> Result<ModuleInfo, crate::error::RelationError> {
        let name = library_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "external".to_string());
        let mut info = ModuleInfo::new(name.clone(), library_path, language, ModuleType::Package);
        // Best-effort: if the path is a directory, list direct children as potential exports.
        if library_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(library_path) {
                for entry in entries.flatten() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if let Some(stem) = file_name
                            .strip_suffix(".rs")
                            .or_else(|| file_name.strip_suffix(".py"))
                            .or_else(|| file_name.strip_suffix(".js"))
                            .or_else(|| file_name.strip_suffix(".ts"))
                        {
                            if !stem.starts_with('_') && !stem.starts_with('.') {
                                info.exports.push(ExportedSymbol::new(
                                    stem.to_string(),
                                    EntityKind::Module,
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(info)
    }

    /// Number of cached modules.
    pub fn cached_count(&self) -> usize {
        self.module_cache.len()
    }

    /// Clear the module cache.
    pub fn clear_cache(&mut self) {
        self.module_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_strategy_for_language() {
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::Rust),
            ImportResolutionStrategy::SymbolLevel
        );
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::Python),
            ImportResolutionStrategy::ModuleLevel
        );
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::Cpp),
            ImportResolutionStrategy::HeaderLevel
        );
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::JavaScript),
            ImportResolutionStrategy::ESModuleLevel
        );
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::Java),
            ImportResolutionStrategy::PackageLevel
        );
        assert_eq!(
            ImportResolutionStrategy::for_language(Language::Go),
            ImportResolutionStrategy::PackageLevel
        );
    }

    #[test]
    fn test_strategy_display() {
        assert_eq!(
            ImportResolutionStrategy::SymbolLevel.to_string(),
            "SymbolLevel"
        );
        assert_eq!(
            ImportResolutionStrategy::HeaderLevel.to_string(),
            "HeaderLevel"
        );
    }

    #[test]
    fn test_module_info_into_external_table() {
        let mut info = ModuleInfo::new(
            "mylib",
            PathBuf::from("/tmp/mylib"),
            Language::Python,
            ModuleType::Package,
        );
        info.exports
            .push(ExportedSymbol::new("foo", EntityKind::Function));
        info.exports
            .push(ExportedSymbol::new("Bar", EntityKind::Class));
        let table = info.into_external_table(Some("1.0.0".to_string()));
        assert_eq!(table.package_name, "mylib");
        assert_eq!(table.version, Some("1.0.0".to_string()));
        assert!(table.get_export("foo").is_some());
        assert!(table.get_export("Bar").is_some());
        assert!(table.get_export("missing").is_none());
    }

    #[test]
    fn test_external_registry_generic_package() {
        let mut registry = ExternalLibraryRegistry::new();
        let tmp = std::env::temp_dir().join("cce_test_external_generic");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::write(tmp.join("alpha.rs"), "pub fn alpha() {}");
        let _ = std::fs::write(tmp.join("beta.rs"), "pub fn beta() {}");
        let info = registry
            .resolve_library(&tmp, Language::Rust)
            .expect("resolve should succeed");
        assert_eq!(info.language, Language::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_import_resolution_depth_default() {
        assert_eq!(
            ImportResolutionDepth::default(),
            ImportResolutionDepth::ImportStatement
        );
    }
}
