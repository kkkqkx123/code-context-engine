//! Import source classification types
//!
//! This module provides types for classifying import sources:
//! - Standard library imports
//! - External library imports
//! - Internal module imports
//! - System library imports
//!
//! Also includes standardized import/export representations across all supported languages.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use crate::types::position::Span;

/// Import source classification
///
/// Classifies imports into different categories based on their origin:
/// - Standard library: Language built-in libraries (std::, core::, etc.)
/// - External library: Third-party dependencies from package managers
/// - Internal module: Project-local imports
/// - Unknown: Imports that couldn't be classified
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImportSource {
    /// Standard library import (e.g., std::collections::HashMap)
    StandardLibrary {
        /// Full import path
        path: String,
        /// Standard library name (e.g., "std", "core", "alloc")
        library: String,
    },
    /// External library import (from package managers)
    ExternalLibrary {
        /// Package name
        package: String,
        /// Version constraint (if available)
        version: Option<String>,
        /// Import path within the package
        path: String,
    },
    /// Project internal module import
    InternalModule {
        /// Relative path from the importing file
        relative_path: String,
        /// Absolute path (resolved)
        absolute_path: String,
    },
    /// Unknown or unclassified import
    Unknown {
        /// Original import path
        raw_path: String,
    },
}

impl ImportSource {
    /// Check if this is an internal import
    pub fn is_internal(&self) -> bool {
        matches!(self, Self::InternalModule { .. })
    }

    /// Check if this is an external import
    pub fn is_external(&self) -> bool {
        matches!(self, Self::ExternalLibrary { .. })
    }

    /// Check if this is a standard library import
    pub fn is_standard(&self) -> bool {
        matches!(self, Self::StandardLibrary { .. })
    }

    /// Get the import path (original or reconstructed)
    pub fn path(&self) -> &str {
        match self {
            Self::StandardLibrary { path, .. } => path,
            Self::ExternalLibrary { path, .. } => path,
            Self::InternalModule { relative_path, .. } => relative_path,
            Self::Unknown { raw_path } => raw_path,
        }
    }

    /// Get the package/library name
    pub fn package_name(&self) -> Option<&str> {
        match self {
            Self::StandardLibrary { library, .. } => Some(library),
            Self::ExternalLibrary { package, .. } => Some(package),
            _ => None,
        }
    }
}

impl Default for ImportSource {
    fn default() -> Self {
        Self::Unknown {
            raw_path: String::new(),
        }
    }
}

/// Import source statistics
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, RkyvSerialize, RkyvDeserialize, Archive,
)]
pub struct ImportSourceStats {
    /// Total number of imports
    pub total_imports: usize,
    /// Number of standard library imports
    pub stdlib_imports: usize,
    /// Number of external library imports
    pub external_imports: usize,
    /// Number of internal module imports
    pub internal_imports: usize,
    /// Number of system library imports
    pub system_imports: usize,
}

/// Import classification classes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportClass {
    /// Standard library (built into language)
    StandardLibrary,
    /// External package/dependency
    ExternalPackage,
    /// Internal module within the project
    InternalModule,
    /// Unknown/unclassified
    Unknown,
}

/// Additional classification metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassificationMetadata {
    /// Package name (for external packages)
    pub package_name: Option<String>,
    /// Package version (if known)
    pub version: Option<String>,
    /// Resolved path (for internal modules)
    pub resolved_path: Option<String>,
    /// Is dev dependency
    pub is_dev_dependency: bool,
    /// Is optional dependency
    pub is_optional: bool,
}

/// Classification result for an import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportClassification {
    /// Import being classified
    pub import: StandardizedImport,
    /// Classification class
    pub class: ImportClass,
    /// Confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Additional metadata
    pub metadata: ClassificationMetadata,
}

impl ImportClassification {
    pub fn new(import: StandardizedImport, class: ImportClass) -> Self {
        Self {
            import,
            class,
            confidence: 1.0,
            metadata: ClassificationMetadata::default(),
        }
    }
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
    pub fn with_metadata(mut self, metadata: ClassificationMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    pub fn is_stdlib(&self) -> bool {
        self.class == ImportClass::StandardLibrary
    }
    pub fn is_external(&self) -> bool {
        self.class == ImportClass::ExternalPackage
    }
    pub fn is_internal(&self) -> bool {
        self.class == ImportClass::InternalModule
    }
}

/// Import kind enumeration
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    RkyvSerialize,
    RkyvDeserialize,
    Archive,
    Default,
)]
pub enum ImportKind {
    #[default]
    SymbolImport,
    ModuleImport,
    SideEffectImport,
    DynamicImport,
    Include,
    DefaultImport,
    NamespaceImport,
    CommonJSRequire,
}

/// Export kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ExportKind {
    #[default]
    Named,
    Default,
    Reexport,
    Wildcard,
    CommonJSExport,
}

/// Target kind for imports/exports
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    RkyvSerialize,
    RkyvDeserialize,
    Archive,
    Default,
)]
pub enum TargetKind {
    #[default]
    Function,
    Class,
    Interface,
    Type,
    Variable,
    Module,
    Other,
}

/// Import target information
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    RkyvSerialize,
    RkyvDeserialize,
    Archive,
    Default,
)]
pub struct ImportTarget {
    pub local_name: String,
    pub original_name: Option<String>,
    pub kind: TargetKind,
}

impl ImportTarget {
    pub fn new(local_name: impl Into<String>, kind: TargetKind) -> Self {
        Self {
            local_name: local_name.into(),
            original_name: None,
            kind,
        }
    }
    pub fn with_original_name(mut self, name: impl Into<String>) -> Self {
        self.original_name = Some(name.into());
        self
    }
}

/// Export target information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportTarget {
    pub name: String,
    pub original_name: Option<String>,
    pub kind: TargetKind,
    pub source_module: Option<String>,
}

impl ExportTarget {
    pub fn new(name: impl Into<String>, kind: TargetKind) -> Self {
        Self {
            name: name.into(),
            original_name: None,
            kind,
            source_module: None,
        }
    }
    pub fn with_source_module(mut self, module: impl Into<String>) -> Self {
        self.source_module = Some(module.into());
        self
    }
}

/// Standardized import representation
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    RkyvSerialize,
    RkyvDeserialize,
    Archive,
    Default,
)]
pub struct StandardizedImport {
    pub kind: ImportKind,
    pub source: String,
    pub target: ImportTarget,
    pub alias: Option<String>,
    pub is_wildcard: bool,
    pub is_default: bool,
    pub is_system_header: bool,
    pub is_relative: bool,
    pub span: Option<Span>,
}

impl StandardizedImport {
    pub fn new(kind: ImportKind, source: impl Into<String>) -> Self {
        Self {
            kind,
            source: source.into(),
            target: ImportTarget::default(),
            alias: None,
            is_wildcard: false,
            is_default: false,
            is_system_header: false,
            is_relative: false,
            span: None,
        }
    }
    pub fn with_target(mut self, target: ImportTarget) -> Self {
        self.target = target;
        self
    }
    pub fn with_wildcard(mut self) -> Self {
        self.is_wildcard = true;
        self
    }
    pub fn with_default(mut self) -> Self {
        self.is_default = true;
        self
    }
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
    pub fn effective_name(&self) -> &str {
        self.alias
            .as_deref()
            .or_else(|| Some(&self.target.local_name))
            .unwrap_or(&self.source)
    }
}

/// Standardized export representation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandardizedExport {
    pub kind: ExportKind,
    pub target: ExportTarget,
    pub is_reexport: bool,
    pub span: Option<Span>,
}

impl StandardizedExport {
    pub fn new(kind: ExportKind, target: ExportTarget) -> Self {
        Self {
            kind,
            target,
            is_reexport: false,
            span: None,
        }
    }
    pub fn with_reexport(mut self) -> Self {
        self.is_reexport = true;
        self
    }
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

/// Standardized import table for a file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandardizedImportTable {
    pub file_id: String,
    pub imports: Vec<StandardizedImport>,
}

impl StandardizedImportTable {
    pub fn new(file_id: impl Into<String>) -> Self {
        Self {
            file_id: file_id.into(),
            imports: Vec::new(),
        }
    }
    pub fn add_import(&mut self, import: StandardizedImport) {
        self.imports.push(import);
    }
    pub fn by_kind(&self, kind: ImportKind) -> Vec<&StandardizedImport> {
        self.imports.iter().filter(|i| i.kind == kind).collect()
    }
    pub fn wildcards(&self) -> Vec<&StandardizedImport> {
        self.imports.iter().filter(|i| i.is_wildcard).collect()
    }
}

/// Standardized export table for a file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandardizedExportTable {
    pub file_id: String,
    pub exports: Vec<StandardizedExport>,
}

impl StandardizedExportTable {
    pub fn new(file_id: impl Into<String>) -> Self {
        Self {
            file_id: file_id.into(),
            exports: Vec::new(),
        }
    }
    pub fn add_export(&mut self, export: StandardizedExport) {
        self.exports.push(export);
    }
}

/// A named re-export carried from the extractor through `ParsedFile` /
/// spool to the symbol table builder, which materializes it as a
/// `ReexportBinding` in the owning module.
///
/// `chain_depth` records how many re-export hops a record has already
/// taken so the resolver can cap re-export chains (and avoid cycles).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    RkyvSerialize,
    RkyvDeserialize,
    Archive,
    Default,
)]
pub struct ReexportRecord {
    /// Name the re-exported symbol is visible under locally
    pub local_name: String,
    /// Module path the re-exported symbol lives in
    pub original_module: String,
    /// Original symbol name inside `original_module`
    pub original_name: String,
    /// Number of re-export hops taken so far
    pub chain_depth: u8,
}

impl ReexportRecord {
    /// Create a record with a zero starting depth.
    pub fn new(
        local_name: impl Into<String>,
        original_module: impl Into<String>,
        original_name: impl Into<String>,
    ) -> Self {
        Self {
            local_name: local_name.into(),
            original_module: original_module.into(),
            original_name: original_name.into(),
            chain_depth: 0,
        }
    }
}
