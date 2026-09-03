//! Core plugin system types and trait
//!
//! This module provides the fundamental plugin abstractions:
//! - [`PluginError`]: Unified error type for plugin operations
//! - [`PluginMetadata`]: Plugin identity and capability description
//! - [`CodePlugin`]: Trait that all plugins must implement
//! - [`PluginSource`]: Trait for discovering and loading plugins from any source
//! - [`PluginBundle`]: A loaded plugin with its filter metadata
//! - [`PluginRegistry`]: In-memory store for registered plugins (pure, no I/O)

mod registry;

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use registry::PluginRegistry;

// ---------------------------------------------------------------------------
// PluginError – merged from cce_infrastructure variants
// ---------------------------------------------------------------------------

/// Error types that may occur during plugin execution
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// Syntax or runtime error during script (e.g. Lua) execution
    #[error("Script error: {0}")]
    ScriptError(String),

    /// Plugin execution timed out
    #[error("Plugin execution timed out")]
    Timeout,

    /// Invalid data format returned by the plugin
    #[error("Invalid output: {0}")]
    InvalidOutput(String),

    /// Internal logic error inside the plugin
    #[error("Logic error: {0}")]
    LogicError(String),

    /// External resources the plugin depends on are unavailable
    #[error("Resource error: {0}")]
    ResourceError(String),

    /// Plugin disabled by circuit breaker
    #[error("Plugin is circuit broken")]
    CircuitBroken,

    /// Plugin not found
    #[error("Plugin not found: {0}")]
    NotFound(String),

    /// Generic execution failure
    #[error("Plugin execution failed: {0}")]
    ExecutionFailed(String),
}

// ---------------------------------------------------------------------------
// PluginCapability
// ---------------------------------------------------------------------------

/// A pipeline capability facet a plugin can provide.
///
/// Plugins declare the facets they support; the registry routes queries by
/// capability + `file_patterns` + `languages` + priority.
///
/// Consumption tiers (priority = descending, ties = registration order):
/// - **Override tier** (first non-empty result wins): `TextGen`
///   (per-group fill), `FormatParse`, `GroupOverride`, `Chunk`, `Rerank`,
///   `SymbolExtract`, `Fusion`, `FileFilter`.
/// - **Chain tier** (all run in priority order, previous output is next
///   input): `Group`, `QueryRewrite`, `ResultFilter`.
/// - **Additive tier** (all run, results merged): `EntityExtract`,
///   `RelationExtract`, `AstLanguage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum PluginCapability {
    /// AST→NL text generation (existing `generate_bm25` / `generate_embedding`).
    /// Override tier — per-group fill, declined groups fall through.
    #[serde(rename = "text_gen")]
    TextGen,
    /// Plugin document format parsing (regex/node parsing → entities).
    /// Override tier.
    #[serde(rename = "format_parse")]
    FormatParse,
    /// Regex-based supplementary entity extraction on code files.
    /// Additive tier.
    #[serde(rename = "entity_extract")]
    EntityExtract,
    /// Custom language parsing (tree-sitter grammar + query scheme). Native-only.
    /// Additive tier.
    #[serde(rename = "ast_language")]
    AstLanguage,
    /// Custom language backed by a host built-in grammar (no FFI pointer).
    /// Lua + native. Additive tier.
    #[serde(rename = "language_remap")]
    LanguageRemap,
    /// Language-specific heuristics (stdlib classification, test-file
    /// detection, entity-kind mapping). Override tier — first non-`None`
    /// result wins; all methods may decline with `Ok(None)`.
    #[serde(rename = "lang_heuristics")]
    LangHeuristics,
    /// Import/export symbol extraction for custom languages (Lua / native).
    /// Override tier.
    #[serde(rename = "symbol_extract")]
    SymbolExtract,
    /// Group post-processing hook.
    /// Chain tier.
    #[serde(rename = "group")]
    Group,
    /// Group full override tier (replaces built-in grouping entirely).
    /// Override tier.
    #[serde(rename = "group_override")]
    GroupOverride,
    /// Chunk override.
    /// Override tier.
    #[serde(rename = "chunk")]
    Chunk,
    /// Query result reranking.
    /// Override tier.
    #[serde(rename = "rerank")]
    Rerank,
    /// Supplementary symbol/relation extraction into the relation index.
    /// Additive tier.
    #[serde(rename = "relation_extract")]
    RelationExtract,
    /// Query rewriting / expansion before recall.
    /// Chain tier.
    #[serde(rename = "query_rewrite")]
    QueryRewrite,
    /// Hybrid fusion weight override.
    /// Override tier — first non-`None` weight set wins.
    #[serde(rename = "fusion")]
    Fusion,
    /// Result filtering / annotation after rerank.
    /// Chain tier.
    #[serde(rename = "result_filter")]
    ResultFilter,
    /// Scanner file inclusion/exclusion decision.
    /// Override tier — first non-`Neutral` decision wins.
    #[serde(rename = "file_filter")]
    FileFilter,
}

impl PluginCapability {
    /// Stable string name (also used in the `.cce/plugins.json` `capabilities` list).
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginCapability::TextGen => "text_gen",
            PluginCapability::FormatParse => "format_parse",
            PluginCapability::EntityExtract => "entity_extract",
            PluginCapability::AstLanguage => "ast_language",
            PluginCapability::LanguageRemap => "language_remap",
            PluginCapability::LangHeuristics => "lang_heuristics",
            PluginCapability::SymbolExtract => "symbol_extract",
            PluginCapability::Group => "group",
            PluginCapability::GroupOverride => "group_override",
            PluginCapability::Chunk => "chunk",
            PluginCapability::Rerank => "rerank",
            PluginCapability::RelationExtract => "relation_extract",
            PluginCapability::QueryRewrite => "query_rewrite",
            PluginCapability::Fusion => "fusion",
            PluginCapability::ResultFilter => "result_filter",
            PluginCapability::FileFilter => "file_filter",
        }
    }

    /// Whether a declared `capabilities` list (empty = probe at runtime) admits
    /// this capability.
    pub fn declared(declared: &[String], capability: PluginCapability) -> bool {
        if declared.is_empty() {
            return true;
        }
        let name = capability.as_str();
        declared.iter().any(|d| {
            d.eq_ignore_ascii_case(name) || d.eq_ignore_ascii_case(&name.replace('_', "-"))
        })
    }

    /// Probe a plugin for this capability (runtime `supports_*`).
    pub fn supported(plugin: &dyn CodePlugin, capability: PluginCapability) -> bool {
        match capability {
            PluginCapability::TextGen => plugin.supports_bm25() || plugin.supports_embedding(),
            PluginCapability::FormatParse => plugin.supports_parse(),
            PluginCapability::EntityExtract => plugin.supports_extract(),
            PluginCapability::AstLanguage => plugin.supports_ast_language(),
            PluginCapability::LanguageRemap => plugin.supports_language_remap(),
            PluginCapability::LangHeuristics => plugin.supports_any_heuristic(),
            PluginCapability::SymbolExtract => plugin.supports_symbol_extract(),
            PluginCapability::Group => plugin.supports_group(),
            PluginCapability::GroupOverride => plugin.supports_group_override(),
            PluginCapability::Chunk => plugin.supports_chunk(),
            PluginCapability::Rerank => plugin.supports_rerank(),
            PluginCapability::RelationExtract => plugin.supports_relation_extract(),
            PluginCapability::QueryRewrite => plugin.supports_query_rewrite(),
            PluginCapability::Fusion => plugin.supports_fusion(),
            PluginCapability::ResultFilter => plugin.supports_result_filter(),
            PluginCapability::FileFilter => plugin.supports_file_filter(),
        }
    }

    /// Parse a capability from its stable string name ([`Self::as_str`]).
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "text_gen" => Ok(PluginCapability::TextGen),
            "format_parse" => Ok(PluginCapability::FormatParse),
            "entity_extract" => Ok(PluginCapability::EntityExtract),
            "ast_language" => Ok(PluginCapability::AstLanguage),
            "language_remap" => Ok(PluginCapability::LanguageRemap),
            "lang_heuristics" => Ok(PluginCapability::LangHeuristics),
            "symbol_extract" => Ok(PluginCapability::SymbolExtract),
            "group" => Ok(PluginCapability::Group),
            "group_override" => Ok(PluginCapability::GroupOverride),
            "chunk" => Ok(PluginCapability::Chunk),
            "rerank" => Ok(PluginCapability::Rerank),
            "relation_extract" => Ok(PluginCapability::RelationExtract),
            "query_rewrite" => Ok(PluginCapability::QueryRewrite),
            "fusion" => Ok(PluginCapability::Fusion),
            "result_filter" => Ok(PluginCapability::ResultFilter),
            "file_filter" => Ok(PluginCapability::FileFilter),
            other => Err(format!("unknown capability: {other}")),
        }
    }
}

/// Upper bound of the recommended priority interval (0–9999).
pub(crate) const PRIORITY_MAX: i32 = 9999;

/// Lower bound of the recommended priority interval. Negative priorities
/// place a plugin *below* the built-in implementation at the integration
/// points where a built-in fallback exists: the plugin is consulted only
/// after the built-in produced no result (fallback tier).
pub(crate) const PRIORITY_MIN: i32 = -9999;

// ---------------------------------------------------------------------------
// PluginMetadata
// ---------------------------------------------------------------------------

/// Metadata describing a plugin's identity and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique identifier for the plugin
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Version string
    pub version: String,
    /// Priority (higher values are executed first).
    ///
    /// The host may override this per-plugin via the bundle/source
    /// configuration; ties are resolved by registration order.
    ///
    /// A negative priority places the plugin below the built-in
    /// implementation: at integration points where a built-in fallback
    /// exists, the plugin runs only after the built-in produced no result.
    /// `0` (the default) keeps the plugin ahead of the built-in.
    pub priority: i32,
    /// Per-capability priority overrides (capability name → priority).
    ///
    /// Capabilities not listed fall back to [`Self::priority`]. See
    /// [`PluginCapability::as_str`] for the canonical capability names.
    /// Negative values follow the same below-built-in fallback semantics
    /// as [`Self::priority`].
    #[serde(default)]
    pub capability_priorities: HashMap<String, i32>,
    /// Description of what the plugin does
    pub description: Option<String>,
    /// Declared capability facets. Empty = the host probes `supports_*` at
    /// runtime (backward-compatible default).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Default for PluginMetadata {
    fn default() -> Self {
        Self {
            id: "unknown".to_string(),
            name: "Unknown Plugin".to_string(),
            version: "0.1.0".to_string(),
            priority: 0,
            capability_priorities: HashMap::new(),
            description: None,
            capabilities: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// CodePlugin trait
// ---------------------------------------------------------------------------

/// Unified code plugin interface
///
/// All methods are optional – plugins only need to implement the
/// functionality they provide.  Capability reporting methods
/// (`supports_*`) let consumers skip plugins that don't provide the
/// required feature.
pub trait CodePlugin: Send + Sync {
    // ── Required ──────────────────────────────────────────────────────

    /// Return the plugin's metadata
    fn metadata(&self) -> &PluginMetadata;

    // ── Capability reporting (override to advertise support) ──────────

    /// Whether this plugin can generate BM25 text
    fn supports_bm25(&self) -> bool {
        false
    }

    /// Whether this plugin can generate embedding text
    fn supports_embedding(&self) -> bool {
        false
    }

    /// Whether this plugin can parse a document format (`FormatParse`).
    fn supports_parse(&self) -> bool {
        false
    }

    /// Whether this plugin can extract supplementary entities (`EntityExtract`).
    fn supports_extract(&self) -> bool {
        false
    }

    /// Whether this plugin provides a custom tree-sitter language (`AstLanguage`).
    fn supports_ast_language(&self) -> bool {
        false
    }

    /// Whether this plugin can extract import/export symbols (`SymbolExtract`).
    fn supports_symbol_extract(&self) -> bool {
        false
    }

    /// Whether this plugin provides a post-grouping hook (`Group`).
    fn supports_group(&self) -> bool {
        false
    }

    /// Whether this plugin provides a full grouping override (`GroupOverride`).
    fn supports_group_override(&self) -> bool {
        false
    }

    /// Whether this plugin can override chunking (`Chunk`).
    fn supports_chunk(&self) -> bool {
        false
    }

    /// Whether this plugin can rerank query results (`Rerank`).
    fn supports_rerank(&self) -> bool {
        false
    }

    /// Whether this plugin can extract supplementary symbols/relations into
    /// the relation index (`RelationExtract`).
    fn supports_relation_extract(&self) -> bool {
        false
    }

    /// Whether this plugin can rewrite / expand queries (`QueryRewrite`).
    fn supports_query_rewrite(&self) -> bool {
        false
    }

    /// Whether this plugin can override fusion weights (`Fusion`).
    fn supports_fusion(&self) -> bool {
        false
    }

    /// Whether this plugin can filter query results (`ResultFilter`).
    fn supports_result_filter(&self) -> bool {
        false
    }

    /// Whether this plugin can make file inclusion/exclusion decisions
    /// during scanning (`FileFilter`).
    fn supports_file_filter(&self) -> bool {
        false
    }

    // ── Optional feature methods ──────────────────────────────────────

    /// Generate BM25 search text for an entity group
    fn generate_bm25(
        &self,
        _group: &cce_types::grouper::EntityGroup,
    ) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    /// Generate embedding text for an entity group
    fn generate_embedding(
        &self,
        _group: &cce_types::grouper::EntityGroup,
    ) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    /// Batch generate BM25 texts (default: fall back to individual calls)
    fn generate_bm25_batch(
        &self,
        groups: &[&cce_types::grouper::EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_bm25(group)?);
        }
        Ok(results)
    }

    /// Batch generate embedding texts (default: fall back to individual calls)
    fn generate_embedding_batch(
        &self,
        groups: &[&cce_types::grouper::EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_embedding(group)?);
        }
        Ok(results)
    }

    // ── FormatParse ───────────────────────────────────────────────────

    /// Parse a document into plugin entities.
    ///
    /// Return `Ok(None)` to decline (the built-in pipeline is used).
    fn parse_document(
        &self,
        _content: &str,
        _file_path: &str,
    ) -> Result<Option<cce_types::PluginDocument>, PluginError> {
        Ok(None)
    }

    // ── EntityExtract ─────────────────────────────────────────────────

    /// Extract supplementary entities from a code file's content.
    ///
    /// Return `Ok(None)` to decline. `language` is the detected language name.
    fn extract_entities(
        &self,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<cce_types::PluginEntity>>, PluginError> {
        Ok(None)
    }

    // ── AstLanguage (Native-only) ─────────────────────────────────────

    /// Return a raw pointer to the tree-sitter `TSLanguage` for the custom
    /// language. `None` for plugins without a grammar.
    ///
    /// # Safety
    ///
    /// The returned pointer must remain valid for the plugin's lifetime and
    /// must be a `*const tree_sitter::ffi::TSLanguage`.
    fn tree_sitter_language(&self) -> Option<*const std::ffi::c_void> {
        None
    }

    /// Return the tree-sitter query string for `query_type`, if provided.
    fn query_scheme(&self, _query_type: cce_types::QueryType) -> Option<String> {
        None
    }

    /// The custom language name (e.g. "zig").
    fn language_name(&self) -> Option<String> {
        None
    }

    /// File extensions for the custom language (e.g. ["zig", "zir"]).
    fn language_extensions(&self) -> Vec<String> {
        Vec::new()
    }

    // ── LanguageRemap (Lua + native, no FFI pointer) ──────────────────

    /// Whether the plugin remaps a custom language onto a host built-in
    /// grammar (see [`Self::remap_grammar_language`]).
    fn supports_language_remap(&self) -> bool {
        false
    }

    /// The host built-in language name whose grammar backs the custom
    /// language (e.g. "JavaScript"). `None` unless [`Self::supports_language_remap`].
    fn remap_grammar_language(&self) -> Option<String> {
        None
    }

    // ── LangHeuristics (Lua + native) ────────────────────────────────

    /// Whether the plugin provides at least one of the three language
    /// heuristics (stdlib / test-file / entity-kind). The per-method
    /// `supports_*` guards below select which are consulted.
    fn supports_any_heuristic(&self) -> bool {
        self.supports_stdlib_heuristic()
            || self.supports_test_file_heuristic()
            || self.supports_entity_kind_heuristic()
    }

    /// Whether the plugin maps module paths to stdlib categories.
    fn supports_stdlib_heuristic(&self) -> bool {
        false
    }

    /// Classify `module_path` (e.g. an import path or entity name) as a
    /// standard-library item. Return the stdlib category name
    /// (`"Collection"`, `"Io"`, …) or `None` to decline / mark not-stdlib.
    fn classify_stdlib(&self, _module_path: &str) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    /// Whether the plugin can decide test-file status by path/content.
    fn supports_test_file_heuristic(&self) -> bool {
        false
    }

    /// Decide whether `file_path`/`content` is a test file. `Ok(None)`
    /// defers to the built-in path/AST rules.
    fn is_test_file(&self, _file_path: &str, _content: &str) -> Result<Option<bool>, PluginError> {
        Ok(None)
    }

    /// Whether the plugin maps tree-sitter capture names to entity kinds.
    fn supports_entity_kind_heuristic(&self) -> bool {
        false
    }

    /// Map a tree-sitter query capture name (e.g. `"entity.tpl_block"`) to
    /// an entity kind name (`"function"`, `"class"`, …). `Ok(None)` defers
    /// to the built-in capture→kind mapping.
    fn entity_kind(&self, _query_capture_name: &str) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    // ── SymbolExtract ─────────────────────────────────────────────────

    /// Extract import statements from source code.
    ///
    /// Return `Ok(None)` to decline. `language` is the detected language name.
    fn extract_imports(
        &self,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<cce_types::PluginImport>>, PluginError> {
        Ok(None)
    }

    /// Extract export declarations from source code.
    ///
    /// Return `Ok(None)` to decline. `language` is the detected language name.
    fn extract_exports(
        &self,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<cce_types::PluginExport>>, PluginError> {
        Ok(None)
    }

    // ── Group ─────────────────────────────────────────────────────────

    /// Post-process groups after the built-in grouping completes.
    ///
    /// Return `Ok(None)` to keep the built-in groups unchanged.
    fn post_group(
        &self,
        _groups: Vec<cce_types::grouper::EntityGroup>,
        _context: cce_types::plugin::GroupPluginContext,
    ) -> Result<Option<Vec<cce_types::grouper::EntityGroup>>, PluginError> {
        Ok(None)
    }

    // ── Chunk ─────────────────────────────────────────────────────────

    /// Override chunking for converted groups.
    ///
    /// Return `Ok(None)` to fall back to the built-in chunker.
    fn chunk(
        &self,
        _conversions: Vec<cce_types::GroupConversions>,
        _file_path: &str,
    ) -> Result<Option<Vec<cce_types::ChunkedResult>>, PluginError> {
        Ok(None)
    }

    // ── Rerank ────────────────────────────────────────────────────────

    /// Rerank query candidates.
    ///
    /// Return `Ok(None)` to decline (the original order is kept).
    fn rerank(
        &self,
        _query: &str,
        _candidates: Vec<cce_types::RerankCandidate>,
    ) -> Result<Option<cce_types::RerankResult>, PluginError> {
        Ok(None)
    }

    // ── Group override tier ────────────────────────────────────────────

    /// Fully replace the built-in grouping for a parsed file.
    ///
    /// The `context` carries the serialized parsed entities and raw relations
    /// (see [`cce_types::plugin::GroupPluginContext`]). Return `Ok(None)`
    /// to keep the built-in grouping.
    fn group(
        &self,
        _context: cce_types::plugin::GroupPluginContext,
    ) -> Result<Option<Vec<cce_types::grouper::EntityGroup>>, PluginError> {
        Ok(None)
    }

    // ── RelationExtract ────────────────────────────────────────────────

    /// Extract supplementary symbols from a code file's content.
    ///
    /// Return `Ok(None)` to decline. `language` is the detected language name.
    fn extract_symbols(
        &self,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginSymbol>>, PluginError> {
        Ok(None)
    }

    /// Extract explicit relations between symbols.
    ///
    /// Return `Ok(None)` to decline. Unresolvable targets are dropped by the
    /// host resolver (never abort the build).
    fn extract_relations(
        &self,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginRelation>>, PluginError> {
        Ok(None)
    }

    // ── QueryRewrite ───────────────────────────────────────────────────

    /// Rewrite / expand a query before recall.
    ///
    /// Return `Ok(None)` to keep the original query.
    fn rewrite_query(
        &self,
        _query: &str,
    ) -> Result<Option<cce_types::plugin::QueryRewriteResult>, PluginError> {
        Ok(None)
    }

    // ── Fusion ─────────────────────────────────────────────────────────

    /// Override hybrid fusion weights.
    ///
    /// Override tier: the first plugin (by priority) returning a non-`None`
    /// weight set takes effect; remaining plugins are not queried. Return
    /// `Ok(None)` to keep the configured weights. Provided weights are
    /// validated to `[0, 1]` by the host before use.
    fn fusion_weights(
        &self,
        _query: &str,
        _vector_count: usize,
        _bm25_count: usize,
    ) -> Result<Option<cce_types::plugin::FusionWeights>, PluginError> {
        Ok(None)
    }

    // ── ResultFilter ───────────────────────────────────────────────────

    /// Filter / boost / annotate candidates after reranking.
    ///
    /// `results` are [`RerankCandidate`]-shaped candidates (id = entity id,
    /// else segment id, else chunk id). Return `Ok(None)` to keep them.
    fn filter_results(
        &self,
        _query: &str,
        _results: Vec<cce_types::RerankCandidate>,
    ) -> Result<Option<Vec<cce_types::plugin::ResultFilterEntry>>, PluginError> {
        Ok(None)
    }

    // ── FileFilter ─────────────────────────────────────────────────────

    /// Decide whether a path should be included in / excluded from scanning.
    ///
    /// Return `Ok(None)` to defer to the built-in `PatternMatcher`.
    fn filter_file(
        &self,
        _file_path: &str,
        _is_directory: bool,
        _size: u64,
    ) -> Result<Option<cce_types::plugin::FileFilterDecision>, PluginError> {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// PluginBundle — a plugin + its filter metadata from a source
// ---------------------------------------------------------------------------

/// A plugin instance bundled with filter metadata from its source.
///
/// Source implementations (e.g. JSON file loader) attach file patterns
/// and language constraints so the registry can filter plugins per-file.
#[derive(Clone)]
pub struct PluginBundle {
    /// The loaded plugin.
    pub plugin: Arc<dyn CodePlugin>,
    /// Glob patterns for files this plugin applies to (None = all files).
    pub file_patterns: Option<Vec<String>>,
    /// Languages this plugin supports (None = all languages).
    pub languages: Option<Vec<String>>,
    /// Declared capability facets overriding the plugin's own metadata
    /// (None = use the plugin's `metadata().capabilities` / runtime probe).
    pub capabilities: Option<Vec<String>>,
    /// Host-side priority override (None = use the plugin's own metadata).
    pub priority: Option<i32>,
    /// Host-side per-capability priority overrides (capability name →
    /// priority; None = use the plugin's own `metadata().capability_priorities`).
    /// Only applies to capabilities listed in `capabilities` / runtime probe.
    pub capability_priorities: Option<HashMap<String, i32>>,
    /// Digest of the plugin's load artifact (e.g. SHA-256 of the Lua script
    /// source or the native library bytes), supplied by the loading source.
    /// Lets consumers detect content changes that do not bump `version`.
    pub content_digest: Option<String>,
}

impl PluginBundle {
    /// Create a bundle for a plugin with no file/language constraints.
    pub fn new(plugin: Arc<dyn CodePlugin>) -> Self {
        Self {
            plugin,
            file_patterns: None,
            languages: None,
            capabilities: None,
            priority: None,
            capability_priorities: None,
            content_digest: None,
        }
    }

    /// Attach a digest of the plugin's load artifact (script source or
    /// library bytes).
    pub fn with_content_digest(mut self, digest: String) -> Self {
        self.content_digest = Some(digest);
        self
    }

    /// Attach file glob patterns.
    pub fn with_file_patterns(mut self, patterns: Vec<String>) -> Self {
        self.file_patterns = Some(patterns);
        self
    }

    /// Attach supported languages.
    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.languages = Some(languages);
        self
    }

    /// Override the declared capability facets (see [`PluginCapability`]).
    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Override the plugin priority (higher values are executed first).
    ///
    /// When set, the bundle priority wins over the plugin's own metadata
    /// declaration; ties are resolved by registration order. Negative
    /// values place the plugin below the built-in fallback.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Override per-capability priorities (see [`PluginMetadata::capability_priorities`]).
    pub fn with_capability_priorities(mut self, priorities: HashMap<String, i32>) -> Self {
        self.capability_priorities = Some(priorities);
        self
    }
}

// ---------------------------------------------------------------------------
// PluginSource — trait for discovering and loading plugins
// ---------------------------------------------------------------------------

/// Trait for sources that discover and load plugins.
///
/// Implementations handle I/O and parsing — reading files, opening
/// libraries, fetching from network, etc. The registry stays pure
/// in-memory and knows nothing about where plugins come from.
pub trait PluginSource: Send + Sync {
    /// Collect all available plugins from this source.
    ///
    /// Returns a list of bundles, each containing a loaded plugin and
    /// its optional filter metadata.
    fn collect(&self) -> Result<Vec<PluginBundle>, PluginError>;
}

/// Override-tier plugin split at the built-in boundary.
///
/// First element: plugins with an effective priority ≥ 0 (run before the
/// built-in implementation). Second element: plugins with a negative
/// priority (fallback tier, run only when the built-in produced nothing).
pub type OverridePluginSplit<'a> = (Vec<&'a Arc<dyn CodePlugin>>, Vec<&'a Arc<dyn CodePlugin>>);
