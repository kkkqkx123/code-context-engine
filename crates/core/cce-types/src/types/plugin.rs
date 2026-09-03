//! Plugin extension contract types
//!
//! This module defines the unified data contract exchanged between the host
//! pipeline and plugins (Lua / native) for the extension capabilities:
//!
//! - [`PluginEntity`]: a free-form entity produced by `FormatParse`,
//!   `EntityExtract`, or custom-language extraction.
//! - [`PluginDocument`]: the output of the `FormatParse` capability.
//! - [`PluginSymbol`] / [`PluginRelation`]: the `RelationExtract` capability.
//! - [`QueryRewriteResult`], [`FusionWeights`], [`ResultFilterEntry`]: the
//!   query-side capabilities (`QueryRewrite` / `Fusion` / `ResultFilter`).
//! - [`FileFilterDecision`]: the `FileFilter` capability.
//!
//! The host converts these into the existing pipeline types (`GroupedEntity`,
//! `entity_spans`) so downstream stages (grouper → AST→NL → chunker) remain
//! unaware of the plugin origin.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::Span;
use crate::types::import::{
    ExportKind, ExportTarget, ImportKind, StandardizedExport, StandardizedImport, TargetKind,
};

/// Context passed to the `Group` capability (both the post-processing hook
/// and the full-override tier).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupPluginContext {
    /// File path being processed.
    pub file_path: String,
    /// Detected language name (e.g. "python").
    pub language: String,
    /// Full file source text.
    pub source: String,
    /// Serialized parsed entities (populated for the `group` override tier).
    #[serde(default)]
    pub entities: Vec<PluginEntity>,
    /// Serialized raw relations (populated for the `group` override tier).
    #[serde(default)]
    pub relations: Vec<PluginRelation>,
}

/// A plugin-produced entity, shape-aligned with [`GroupedEntity`].
///
/// `kind` is a free-form string (e.g. `"route"`, `"section"`, `"function"`).
/// The optional [`Span`] is used for `source_ranges` tracking after the host
/// converts this into a [`GroupedEntity`].
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginEntity {
    /// Entity ID (unique within its document/file scope).
    pub id: String,
    /// Free-form entity kind (e.g. "route" / "section" / "function").
    pub kind: String,
    /// Entity name.
    pub name: String,
    /// Signature (e.g. "GET /users" for a route).
    #[serde(default)]
    pub signature: Option<String>,
    /// Doc comment (extracted text).
    #[serde(default)]
    pub doc_comment: Option<String>,
    /// Extension metadata (framework-specific info, annotations, etc.).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Optional source span for `source_ranges` tracking.
    #[serde(default)]
    pub span: Option<Span>,
    /// Child entities.
    #[serde(default)]
    pub children: Vec<PluginEntity>,
}

impl PluginEntity {
    /// Create a minimal entity.
    pub fn new(id: impl Into<String>, kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the signature.
    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = Some(signature.into());
        self
    }

    /// Set the doc comment.
    pub fn with_doc_comment(mut self, doc_comment: impl Into<String>) -> Self {
        self.doc_comment = Some(doc_comment.into());
        self
    }

    /// Insert a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set the source span.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

/// Output of the `FormatParse` capability: a parsed document as a list of
/// [`PluginEntity`] plus an optional title and language hint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginDocument {
    /// Optional document title.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional language hint (e.g. "python"). Used for downstream routing.
    #[serde(default)]
    pub language: Option<String>,
    /// Parsed entities.
    #[serde(default)]
    pub entities: Vec<PluginEntity>,
}

// ---------------------------------------------------------------------------
// SymbolExtract contract
// ---------------------------------------------------------------------------

/// A plugin-produced import statement (`SymbolExtract` capability).
///
/// Converted by the host into a [`crate::types::import::StandardizedImport`]
/// so downstream stages (relation index, import table) stay plugin-unaware.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginImport {
    /// Import path (e.g. "std::collections::HashMap" or "os.path").
    pub path: String,
    /// Imported symbols (None = wildcard import).
    #[serde(default)]
    pub symbols: Option<Vec<String>>,
    /// Alias (e.g. "use std::collections::HashMap as Map").
    #[serde(default)]
    pub alias: Option<String>,
    /// Whether this is a wildcard/glob import.
    #[serde(default)]
    pub is_wildcard: bool,
    /// Extension metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl PluginImport {
    /// Create a minimal import.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    /// Set the imported symbols.
    pub fn with_symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = Some(symbols);
        self
    }

    /// Set the alias.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Mark as a wildcard import.
    pub fn with_wildcard(mut self) -> Self {
        self.is_wildcard = true;
        self
    }
}

/// A plugin-produced export declaration (`SymbolExtract` capability).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginExport {
    /// Exported symbol name.
    pub name: String,
    /// Symbol kind (e.g. "function", "class", "type", "constant").
    pub kind: String,
    /// Visibility (e.g. "public", "internal").
    #[serde(default)]
    pub visibility: Option<String>,
    /// Source location.
    #[serde(default)]
    pub location: Option<PluginSymbolLocation>,
    /// Extension metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl PluginExport {
    /// Create a minimal export.
    pub fn new(name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// RelationExtract contract
// ---------------------------------------------------------------------------

/// A plugin-produced symbol, registered into the symbol table when the
/// `RelationExtract` capability is enabled (`relation.plugin_symbols_enabled`).
///
/// `kind` is a free-form string (e.g. `"service"`, `"bean"`, `"route"`).
/// `visibility` maps to the host [`crate::types::relation::Visibility`] via a
/// best-effort name match (unknown values default to private).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginSymbol {
    /// Symbol id (unique within its file scope). Defaults to `name`.
    pub id: String,
    /// Symbol name.
    pub name: String,
    /// Free-form symbol kind (e.g. "service" / "bean" / "route").
    pub kind: String,
    /// Visibility string (e.g. "public" / "private").
    #[serde(default)]
    pub visibility: Option<String>,
    /// Module path override (defaults to the declaring file's module).
    #[serde(default)]
    pub module_path: Option<String>,
    /// Source location.
    #[serde(default)]
    pub location: Option<PluginSymbolLocation>,
    /// Extension metadata (annotations, framework info, etc.).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Child symbols.
    #[serde(default)]
    pub children: Vec<PluginSymbol>,
}

impl PluginSymbol {
    /// Create a minimal symbol.
    pub fn new(id: impl Into<String>, kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the visibility.
    pub fn with_visibility(mut self, visibility: impl Into<String>) -> Self {
        self.visibility = Some(visibility.into());
        self
    }

    /// Insert a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set the module path.
    pub fn with_module_path(mut self, module_path: impl Into<String>) -> Self {
        self.module_path = Some(module_path.into());
        self
    }
}

/// Source location of a [`PluginSymbol`].
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginSymbolLocation {
    /// Source span.
    #[serde(default)]
    pub span: Option<Span>,
}

/// A plugin-produced explicit relation between two symbols.
///
/// `from` / `to` reference a [`PluginSymbol`] `id` or a built-in entity name.
/// The host resolver maps them to `SymbolKey → entity_id`; unresolvable
/// targets are dropped with a warning (they never abort the build).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginRelation {
    /// Source symbol reference.
    pub from: String,
    /// Target symbol reference.
    pub to: String,
    /// Relation type (e.g. "call" / "import" / "extends" / "implements" /
    /// "uses" / "injects").
    pub relation_type: String,
    /// Extension metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl PluginRelation {
    /// Create a minimal relation.
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        relation_type: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            relation_type: relation_type.into(),
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Query-side contracts
// ---------------------------------------------------------------------------

/// Output of the `QueryRewrite` capability: a rewritten query plus optional
/// expansion terms used as additional recall queries.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryRewriteResult {
    /// Rewritten query text.
    pub rewritten_query: String,
    /// Additional expansion terms (each used as an extra recall query).
    #[serde(default)]
    pub expansion_terms: Vec<String>,
}

/// Weight override for hybrid fusion (`Fusion` capability).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FusionWeights {
    /// Vector recall weight.
    pub vector_weight: Option<f32>,
    /// BM25 recall weight.
    pub bm25_weight: Option<f32>,
    /// Minimum fusion score threshold.
    pub min_score: Option<f32>,
}

/// One entry of the `ResultFilter` capability output: keep/remove/boost a
/// candidate by id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultFilterEntry {
    /// Candidate id (entity id, else segment id, else chunk id).
    pub id: String,
    /// Whether to remove the candidate.
    #[serde(default)]
    pub remove: bool,
    /// Optional additive boost (in score units, applied before final sort).
    #[serde(default)]
    pub boost: Option<f32>,
    /// Optional note (only used for logging/inspection).
    #[serde(default)]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Scan contract
// ---------------------------------------------------------------------------

/// Decision produced by the `FileFilter` capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileFilterDecision {
    /// Force-include the path (overrides built-in exclusion).
    #[serde(rename = "include")]
    Include,
    /// Force-exclude the path.
    #[serde(rename = "exclude")]
    Exclude,
    /// Defer to the built-in `PatternMatcher`.
    #[serde(rename = "neutral")]
    Neutral,
}

// ---------------------------------------------------------------------------
// SymbolExtract → standardized import/export conversions
// ---------------------------------------------------------------------------

impl From<PluginImport> for StandardizedImport {
    fn from(import: PluginImport) -> Self {
        let has_symbols = import.symbols.as_deref().is_some_and(|s| !s.is_empty());
        let kind = if has_symbols {
            ImportKind::SymbolImport
        } else {
            ImportKind::ModuleImport
        };
        let mut standardized = StandardizedImport::new(kind, import.path);
        standardized.is_wildcard = import.is_wildcard;
        standardized.alias = import.alias;
        if let Some(symbols) = import.symbols {
            if let Some(first) = symbols.first() {
                standardized.target.local_name = first.clone();
            }
        }
        standardized
    }
}

impl From<PluginExport> for StandardizedExport {
    fn from(export: PluginExport) -> Self {
        StandardizedExport::new(
            ExportKind::Named,
            ExportTarget::new(export.name, TargetKind::Other),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_import_to_standardized_symbol() {
        let standardized = StandardizedImport::from(
            PluginImport::new("std").with_symbols(vec!["HashMap".to_string()]),
        );
        assert_eq!(standardized.source, "std");
        assert_eq!(standardized.kind, ImportKind::SymbolImport);
        assert_eq!(standardized.target.local_name, "HashMap");
        assert!(!standardized.is_wildcard);
    }

    #[test]
    fn test_plugin_import_to_standardized_module() {
        let standardized = StandardizedImport::from(PluginImport::new("std").with_wildcard());
        assert_eq!(standardized.source, "std");
        assert_eq!(standardized.kind, ImportKind::ModuleImport);
        assert!(standardized.is_wildcard);
    }

    #[test]
    fn test_plugin_export_to_standardized() {
        let export = StandardizedExport::from(PluginExport::new("main", "function"));
        assert_eq!(export.kind, ExportKind::Named);
        assert_eq!(export.target.name, "main");
    }
}
