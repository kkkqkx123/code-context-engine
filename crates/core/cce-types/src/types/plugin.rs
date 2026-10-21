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
//! - [`VocabularyEntry`]: the `Vocabulary` capability (domain terminology).
//!
//! The host converts these into the existing pipeline types (`GroupedEntity`,
//! `entity_spans`) so downstream stages (grouper → AST→NL → chunker) remain
//! unaware of the plugin origin.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::Span;
use crate::types::import::{
    ExportKind, ExportTarget, ImportKind, ImportTarget, StandardizedExport, StandardizedImport,
    TargetKind,
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

    /// Insert a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
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

/// Fusion algorithm override for the `Fusion` capability.
///
/// Type-safe enumeration of algorithms a plugin can request. The host maps
/// each variant to its internal `RecallFusionAlgorithm` representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginFusionAlgorithm {
    /// Weighted min-max score fusion.
    WeightedMinMax,
    /// Reciprocal rank fusion.
    Rrf,
    /// Weight-aware Borda count.
    BordaCount,
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
    /// Fusion algorithm override. Absent means "keep the configured algorithm".
    #[serde(default)]
    pub algorithm: Option<PluginFusionAlgorithm>,
    /// RRF rank-discount constant used together with `Rrf`. Absent means
    /// the configured default (60); zero is rejected by the host.
    /// Ignored for other algorithms.
    #[serde(default)]
    pub rrf_k: Option<u32>,
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
// Vocabulary contract
// ---------------------------------------------------------------------------

/// Maximum number of glossary entries the host accepts from one plugin table.
///
/// The limit bounds the memory a misbehaving plugin can force on the host;
/// the entries beyond it are dropped at the plugin boundary.
pub const MAX_VOCABULARY_ENTRIES: usize = 256;

/// Maximum length (in characters) of a single glossary expansion.
///
/// Longer expansions are truncated at the plugin boundary.
pub const MAX_VOCABULARY_EXPANSION_LENGTH: usize = 200;

/// One glossary entry contributed by the `Vocabulary` capability.
///
/// The plugin owns the terminology: `token` is the identifier that appears in
/// the source (e.g. `"stts"`) and `expansion` is the human-readable meaning
/// (e.g. `"sync time-to-sample"`). The host only matches identifier tokens
/// and renders the expansion; it never interprets the terminology.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VocabularyEntry {
    /// The identifier token to match (e.g. `"stts"`).
    pub token: String,
    /// The human-readable expansion of the token (e.g. `"sync time-to-sample"`).
    pub expansion: String,
}

impl VocabularyEntry {
    /// Create a glossary entry.
    pub fn new(token: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            expansion: expansion.into(),
        }
    }

    /// The normalized entry, or `None` when it carries no usable terminology.
    ///
    /// The token and the expansion are trimmed, and the expansion is
    /// truncated to [`MAX_VOCABULARY_EXPANSION_LENGTH`]. Both the plugin
    /// boundary and the host-side merge apply this rule, so a table is
    /// interpreted identically no matter which side inspects it.
    pub fn normalized(&self) -> Option<VocabularyEntry> {
        let token = self.token.trim();
        let expansion = self.expansion.trim();
        if token.is_empty() || expansion.is_empty() {
            return None;
        }
        let expansion: String = expansion
            .chars()
            .take(MAX_VOCABULARY_EXPANSION_LENGTH)
            .collect();
        Some(VocabularyEntry {
            token: token.to_string(),
            expansion,
        })
    }
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

/// Map a free-form target kind name to a [`TargetKind`].
fn target_kind_from_name(name: &str) -> TargetKind {
    match name.trim().to_lowercase().as_str() {
        "function" | "fn" | "method" | "proc" => TargetKind::Function,
        "class" | "struct" | "bean" | "service" | "repository" => TargetKind::Class,
        "interface" | "trait" | "protocol" => TargetKind::Interface,
        "type" | "typedef" | "alias" => TargetKind::Type,
        "variable" | "var" | "let" | "const" | "constant" | "field" => TargetKind::Variable,
        "module" | "namespace" | "package" => TargetKind::Module,
        _ => TargetKind::Other,
    }
}

/// Map a free-form export kind hint to an [`ExportKind`].
fn export_kind_from_hint(hint: &str) -> Option<ExportKind> {
    match hint.trim().to_lowercase().as_str() {
        "named" => Some(ExportKind::Named),
        "default" => Some(ExportKind::Default),
        "reexport" | "re-export" => Some(ExportKind::Reexport),
        "wildcard" | "glob" | "*" => Some(ExportKind::Wildcard),
        "commonjs" | "cjs" => Some(ExportKind::CommonJSExport),
        _ => None,
    }
}

/// Map a free-form import kind hint to an [`ImportKind`].
fn import_kind_from_hint(hint: &str) -> Option<ImportKind> {
    match hint.trim().to_lowercase().as_str() {
        "symbol" => Some(ImportKind::SymbolImport),
        "module" => Some(ImportKind::ModuleImport),
        "side_effect" | "side-effect" | "sideeffect" => Some(ImportKind::SideEffectImport),
        "dynamic" => Some(ImportKind::DynamicImport),
        "include" => Some(ImportKind::Include),
        "default" => Some(ImportKind::DefaultImport),
        "namespace" => Some(ImportKind::NamespaceImport),
        "commonjs" | "cjs" | "require" => Some(ImportKind::CommonJSRequire),
        _ => None,
    }
}

impl PluginImport {
    /// Expand one plugin import into standardized imports, one per symbol.
    ///
    /// Symbol lists are preserved entry by entry instead of keeping only the
    /// first symbol. A single symbol combined with an alias maps to a renamed
    /// target. The metadata `kind` hint refines the import kind.
    pub fn into_standardized_list(self) -> Vec<StandardizedImport> {
        let has_symbols = self.symbols.as_deref().is_some_and(|s| !s.is_empty());
        let base_kind = self
            .metadata
            .get("kind")
            .and_then(|hint| import_kind_from_hint(hint))
            .unwrap_or(if has_symbols {
                ImportKind::SymbolImport
            } else {
                ImportKind::ModuleImport
            });
        let target_kind = self
            .metadata
            .get("target_kind")
            .map(|hint| target_kind_from_name(hint))
            .unwrap_or(TargetKind::Other);
        let is_relative = self.path.starts_with("./")
            || self.path.starts_with("../")
            || self.path.starts_with('/');
        let symbols = self.symbols.unwrap_or_default();
        if symbols.is_empty() {
            let mut standardized = StandardizedImport::new(base_kind, self.path);
            standardized.is_wildcard = self.is_wildcard;
            standardized.alias = self.alias;
            standardized.is_relative = is_relative;
            return vec![standardized];
        }
        let alias_for_single = if symbols.len() == 1 {
            self.alias.clone()
        } else {
            None
        };
        symbols
            .into_iter()
            .enumerate()
            .map(|(index, symbol)| {
                let mut standardized = StandardizedImport::new(base_kind, self.path.clone());
                standardized.is_wildcard = self.is_wildcard;
                standardized.is_relative = is_relative;
                let mut target = ImportTarget::new(symbol.clone(), target_kind);
                if index == 0 {
                    if let Some(alias) = alias_for_single.as_deref() {
                        target = ImportTarget::new(alias, target_kind).with_original_name(symbol);
                        standardized.alias = Some(alias.to_string());
                    }
                }
                standardized.target = target;
                standardized
            })
            .collect()
    }
}

impl PluginExport {
    /// Convert one plugin export into its standardized form.
    ///
    /// The free-form kind maps to a target kind. The metadata `export_kind`
    /// hint (or the `is_default` / `is_reexport` / `is_wildcard` flags) maps
    /// to an export kind. Location spans and re-export linkage are preserved.
    pub fn into_standardized(self) -> StandardizedExport {
        let target_kind = target_kind_from_name(&self.kind);
        let kind = self
            .metadata
            .get("export_kind")
            .and_then(|hint| export_kind_from_hint(hint))
            .or_else(|| {
                if self
                    .metadata
                    .get("is_reexport")
                    .is_some_and(|v| v == "true")
                {
                    Some(ExportKind::Reexport)
                } else if self
                    .metadata
                    .get("is_wildcard")
                    .is_some_and(|v| v == "true")
                {
                    Some(ExportKind::Wildcard)
                } else if self.metadata.get("is_default").is_some_and(|v| v == "true") {
                    Some(ExportKind::Default)
                } else {
                    None
                }
            })
            .unwrap_or(ExportKind::Named);
        let mut target = ExportTarget::new(self.name, target_kind);
        if let Some(original) = self.metadata.get("original_name") {
            target.original_name = Some(original.clone());
        }
        if let Some(module) = self.metadata.get("source_module") {
            target.source_module = Some(module.clone());
        }
        let mut export = StandardizedExport::new(kind, target);
        if self
            .metadata
            .get("is_reexport")
            .is_some_and(|v| v == "true")
            && export.kind == ExportKind::Named
        {
            export = export.with_reexport();
        }
        if let Some(location) = self.location {
            if let Some(span) = location.span {
                export = export.with_span(span);
            }
        }
        export
    }
}

impl From<PluginImport> for StandardizedImport {
    fn from(import: PluginImport) -> Self {
        import
            .into_standardized_list()
            .into_iter()
            .next()
            .unwrap_or_default()
    }
}

impl From<PluginExport> for StandardizedExport {
    fn from(export: PluginExport) -> Self {
        export.into_standardized()
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
    fn test_plugin_import_expands_symbol_list() {
        let expanded = PluginImport::new("std")
            .with_symbols(vec!["HashMap".to_string(), "HashSet".to_string()])
            .into_standardized_list();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].target.local_name, "HashMap");
        assert_eq!(expanded[1].target.local_name, "HashSet");
    }

    #[test]
    fn test_plugin_import_alias_maps_to_renamed_target() {
        let standardized = StandardizedImport::from(
            PluginImport::new("std")
                .with_symbols(vec!["HashMap".to_string()])
                .with_alias("Map"),
        );
        assert_eq!(standardized.target.local_name, "Map");
        assert_eq!(
            standardized.target.original_name.as_deref(),
            Some("HashMap")
        );
        assert_eq!(standardized.alias.as_deref(), Some("Map"));
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
        assert_eq!(export.target.kind, TargetKind::Function);
    }

    #[test]
    fn test_plugin_export_kind_hint_and_reexport_linkage() {
        let export = PluginExport::new("main", "class")
            .with_metadata("export_kind", "default")
            .with_metadata("source_module", "mod_a")
            .with_metadata("original_name", "Main");
        let standardized = export.into_standardized();
        assert_eq!(standardized.kind, ExportKind::Default);
        assert_eq!(standardized.target.kind, TargetKind::Class);
        assert_eq!(standardized.target.source_module.as_deref(), Some("mod_a"));
        assert_eq!(standardized.target.original_name.as_deref(), Some("Main"));
    }
}
