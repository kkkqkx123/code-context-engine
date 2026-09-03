use cce_plugin::{PluginError, PluginMetadata};
use cce_types::grouper::EntityGroup;
use cce_types::plugin::{
    FileFilterDecision, FusionWeights, PluginDocument, PluginEntity, PluginExport, PluginImport,
    PluginRelation, PluginSymbol, QueryRewriteResult, ResultFilterEntry,
};
use cce_types::{ChunkedResult, RerankResult};

/// The trait every native CCE plugin must implement (FFI-facing).
///
/// All methods are optional — plugins only need to implement the
/// functionality they provide.  Capability reporting methods
/// (`supports_*`) let consumers skip plugins that don't provide the
/// required feature.
///
/// # Required methods
///
/// - [`metadata`](FfiPlugin::metadata) — describes the plugin to the host.
///
/// # Optional methods
///
/// | Method | Capability | Guard |
/// |--------|-----------|-------|
/// | [`generate_bm25`](FfiPlugin::generate_bm25) | BM25 NL generation | [`supports_bm25`](FfiPlugin::supports_bm25)→`true` |
/// | [`generate_embedding`](FfiPlugin::generate_embedding) | Embedding NL generation | [`supports_embedding`](FfiPlugin::supports_embedding)→`true` |
/// | [`generate_bm25_batch`](FfiPlugin::generate_bm25_batch) | Batched BM25 NL generation | [`supports_bm25`](FfiPlugin::supports_bm25)→`true` |
/// | [`generate_embedding_batch`](FfiPlugin::generate_embedding_batch) | Batched embedding NL generation | [`supports_embedding`](FfiPlugin::supports_embedding)→`true` |
/// | [`create_context`](FfiPlugin::create_context) | Allocate plugin state | [`supports_lifecycle`](FfiPlugin::supports_lifecycle)→`true` |
/// | [`destroy_context`](FfiPlugin::destroy_context) | Free plugin state | `create_context` returned non-null |
///
/// The `supports_*` guard methods default to `false`. Override them to `true` so
/// the host knows to query your plugin for the corresponding capability.
///
/// The NL methods receive the entity group as a **JSON string** rather than
/// a typed struct, because the host's `EntityGroup` uses internal types
/// (`CompactString`, `SmallVec`, `Arc<str>`, etc.) that the SDK does not
/// re-export. Simply call `serde_json::from_str(group_json)` in your plugin
/// to obtain a serde_json::Value with the full entity-group structure, or
/// cherry-pick the fields you need.
pub trait FfiPlugin: Send + Sync + 'static {
    /// Return the plugin's metadata.
    ///
    /// This is **required** — the host calls this once during loading.
    fn metadata(&self) -> PluginMetadata;

    // ── Capability guards ──

    /// Whether the plugin implements BM25 NL generation.
    fn supports_bm25(&self) -> bool {
        false
    }

    /// Whether the plugin implements embedding NL generation.
    fn supports_embedding(&self) -> bool {
        false
    }

    /// Whether the plugin implements `FormatParse` (document format parsing).
    fn supports_parse(&self) -> bool {
        false
    }

    /// Whether the plugin implements `EntityExtract` (supplementary entities).
    fn supports_extract(&self) -> bool {
        false
    }

    /// Whether the plugin implements the `Group` post-processing hook.
    fn supports_group(&self) -> bool {
        false
    }

    /// Whether the plugin implements the `Chunk` override.
    fn supports_chunk(&self) -> bool {
        false
    }

    /// Whether the plugin implements `Rerank`.
    fn supports_rerank(&self) -> bool {
        false
    }

    /// Whether the plugin provides a custom tree-sitter language (`AstLanguage`).
    fn supports_ast_language(&self) -> bool {
        false
    }

    /// Whether the plugin provides a full grouping override (`GroupOverride`).
    fn supports_group_override(&self) -> bool {
        false
    }

    /// Whether the plugin provides `RelationExtract` (symbols/relations).
    fn supports_relation_extract(&self) -> bool {
        false
    }

    /// Whether the plugin provides `SymbolExtract` (import/export extraction).
    fn supports_symbol_extract(&self) -> bool {
        false
    }

    /// Whether the plugin provides `QueryRewrite`.
    fn supports_query_rewrite(&self) -> bool {
        false
    }

    /// Whether the plugin provides `Fusion` weight override.
    fn supports_fusion(&self) -> bool {
        false
    }

    /// Whether the plugin provides `ResultFilter`.
    fn supports_result_filter(&self) -> bool {
        false
    }

    /// Whether the plugin provides `FileFilter`.
    fn supports_file_filter(&self) -> bool {
        false
    }

    /// Whether the plugin has lifecycle management (create_context / destroy_context).
    fn supports_lifecycle(&self) -> bool {
        false
    }

    // ── Optional: lifecycle ──

    /// Allocate an opaque context for this plugin instance.
    ///
    /// The returned pointer is passed to all subsequent FFI calls and
    /// freed via [`destroy_context`](FfiPlugin::destroy_context).
    /// Return `None` (the default) if no context is needed.
    ///
    /// The host may call the generate methods **concurrently** on the same
    /// context, so the context MUST be thread-safe.
    fn create_context(&self) -> Option<*mut std::ffi::c_void> {
        None
    }

    /// Destroy a context previously returned by [`create_context`](FfiPlugin::create_context).
    ///
    /// # Safety
    ///
    /// `ctx` must have been returned by `create_context` and not yet freed.
    unsafe fn destroy_context(&self, _ctx: *mut std::ffi::c_void) {}

    // ── Optional: NL generation (single) ──

    /// Generate BM25 natural language text for an entity group.
    ///
    /// `ctx` is the opaque context returned by [`create_context`], or null
    /// if the plugin does not use lifecycle state.
    /// `group_json` is a JSON-serialized [`EntityGroup`] from the host.
    /// Return the generated text, or `None` to skip this group.
    fn generate_bm25(
        &self,
        _ctx: *mut std::ffi::c_void,
        _group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    /// Generate embedding natural language text for an entity group.
    ///
    /// `ctx` is the opaque context returned by [`create_context`], or null
    /// if the plugin does not use lifecycle state.
    /// `group_json` is a JSON-serialized [`EntityGroup`] from the host.
    /// Return the generated text, or `None` to skip this group.
    fn generate_embedding(
        &self,
        _ctx: *mut std::ffi::c_void,
        _group_json: &str,
    ) -> Result<Option<String>, PluginError> {
        Ok(None)
    }

    // ── Optional: NL generation (batch) ──

    /// Generate BM25 natural language text for a batch of entity groups.
    ///
    /// `groups_json` is a JSON **array** of serialized [`EntityGroup`]
    /// objects. Return one element per input group: `Some(text)` for
    /// generated text, `None` for a group the plugin wants to skip.
    ///
    /// The default implementation parses the array and falls back to
    /// [`generate_bm25`](FfiPlugin::generate_bm25) per group. Override this
    /// for single-pass batch generation (recommended for throughput).
    fn generate_bm25_batch(
        &self,
        ctx: *mut std::ffi::c_void,
        groups_json: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        let groups: Vec<serde_json::Value> = serde_json::from_str(groups_json).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to parse group batch JSON: {e}"))
        })?;
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_bm25(ctx, &group.to_string())?);
        }
        Ok(results)
    }

    /// Generate embedding natural language text for a batch of entity groups.
    ///
    /// Same contract as [`generate_bm25_batch`](FfiPlugin::generate_bm25_batch).
    fn generate_embedding_batch(
        &self,
        ctx: *mut std::ffi::c_void,
        groups_json: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        let groups: Vec<serde_json::Value> = serde_json::from_str(groups_json).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to parse group batch JSON: {e}"))
        })?;
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_embedding(ctx, &group.to_string())?);
        }
        Ok(results)
    }

    // ── Optional: FormatParse ──

    /// Parse a document into a [`PluginDocument`].
    ///
    /// Return `Ok(None)` to decline (the built-in document pipeline is used).
    fn parse_document(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
    ) -> Result<Option<PluginDocument>, PluginError> {
        Ok(None)
    }

    // ── Optional: EntityExtract ──

    /// Extract supplementary entities from a code file.
    ///
    /// Return `Ok(None)` to decline.
    fn extract_entities(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        Ok(None)
    }

    // ── Optional: Group ──

    /// Post-process groups after built-in grouping.
    ///
    /// `groups_json` is a JSON array of [`EntityGroup`] objects;
    /// `context_json` is a [`GroupPluginContext`]. Return `Ok(None)` to keep
    /// the built-in groups unchanged.
    fn post_group(
        &self,
        _ctx: *mut std::ffi::c_void,
        _groups_json: &str,
        _context_json: &str,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        Ok(None)
    }

    // ── Optional: Chunk ──

    /// Override chunking for converted groups.
    ///
    /// `conversions_json` is a JSON array of [`GroupConversions`] objects.
    /// Return `Ok(None)` to fall back to the built-in chunker.
    fn chunk(
        &self,
        _ctx: *mut std::ffi::c_void,
        _conversions_json: &str,
        _file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        Ok(None)
    }

    // ── Optional: Rerank ──

    /// Rerank query candidates.
    ///
    /// `candidates_json` is a JSON array of [`RerankCandidate`] objects.
    /// Return `Ok(None)` to decline (the original order is kept).
    fn rerank(
        &self,
        _ctx: *mut std::ffi::c_void,
        _query: &str,
        _candidates_json: &str,
    ) -> Result<Option<RerankResult>, PluginError> {
        Ok(None)
    }

    // ── Optional: Group override tier ──

    /// Fully replace built-in grouping for a parsed file.
    ///
    /// `context_json` is a JSON-serialized [`GroupPluginContext`] (carrying the
    /// serialized parsed entities and raw relations). Return `Ok(None)` to keep
    /// the built-in grouping.
    fn group(
        &self,
        _ctx: *mut std::ffi::c_void,
        _context_json: &str,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        Ok(None)
    }

    // ── Optional: RelationExtract ──

    /// Extract supplementary symbols from a code file.
    ///
    /// Return `Ok(None)` to decline.
    fn extract_symbols(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginSymbol>>, PluginError> {
        Ok(None)
    }

    /// Extract explicit relations between symbols.
    ///
    /// Return `Ok(None)` to decline.
    fn extract_relations(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginRelation>>, PluginError> {
        Ok(None)
    }

    // ── Optional: SymbolExtract ──

    /// Extract import statements from a code file.
    ///
    /// Return `Ok(None)` to decline.
    fn extract_imports(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginImport>>, PluginError> {
        Ok(None)
    }

    /// Extract export declarations from a code file.
    ///
    /// Return `Ok(None)` to decline.
    fn extract_exports(
        &self,
        _ctx: *mut std::ffi::c_void,
        _content: &str,
        _file_path: &str,
        _language: &str,
    ) -> Result<Option<Vec<PluginExport>>, PluginError> {
        Ok(None)
    }

    // ── Optional: QueryRewrite ──

    /// Rewrite / expand a query before recall.
    ///
    /// Return `Ok(None)` to keep the original query.
    fn rewrite_query(
        &self,
        _ctx: *mut std::ffi::c_void,
        _query: &str,
    ) -> Result<Option<QueryRewriteResult>, PluginError> {
        Ok(None)
    }

    // ── Optional: Fusion ──

    /// Override hybrid fusion weights.
    ///
    /// Return `Ok(None)` to keep the configured weights.
    fn fusion_weights(
        &self,
        _ctx: *mut std::ffi::c_void,
        _query: &str,
        _vector_count: usize,
        _bm25_count: usize,
    ) -> Result<Option<FusionWeights>, PluginError> {
        Ok(None)
    }

    // ── Optional: ResultFilter ──

    /// Filter / boost / annotate candidates after reranking.
    ///
    /// `results_json` is a JSON array of [`RerankCandidate`] objects.
    /// Return `Ok(None)` to keep them unchanged.
    fn filter_results(
        &self,
        _ctx: *mut std::ffi::c_void,
        _query: &str,
        _results_json: &str,
    ) -> Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        Ok(None)
    }

    // ── Optional: FileFilter ──

    /// Decide whether a path should be included/excluded during scanning.
    ///
    /// Return `Ok(None)` to defer to the built-in matcher.
    fn filter_file(
        &self,
        _ctx: *mut std::ffi::c_void,
        _file_path: &str,
        _is_directory: bool,
        _size: u64,
    ) -> Result<Option<FileFilterDecision>, PluginError> {
        Ok(None)
    }

    // ── Optional: AstLanguage (Native-only) ──

    /// Return a raw pointer to the tree-sitter `TSLanguage` for the custom
    /// language, or null.
    ///
    /// # Safety
    ///
    /// The returned pointer must remain valid for the plugin's lifetime.
    fn tree_sitter_language(&self) -> Option<*mut std::ffi::c_void> {
        None
    }

    /// Return the tree-sitter query string for a [`cce_types::QueryType`]
    /// (0..=7), or `None` when no scheme is provided for that query type.
    fn query_scheme(&self, _ctx: *mut std::ffi::c_void, _query_type: u32) -> Option<String> {
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

    // ── Optional: LanguageRemap (Lua + native; no embedded grammar) ──

    /// Whether the plugin remaps a custom language onto a host built-in
    /// grammar (see [`Self::remap_grammar_language`]).
    fn supports_language_remap(&self) -> bool {
        false
    }

    /// The host built-in language name whose grammar backs the custom
    /// language (e.g. "JavaScript").
    fn remap_grammar_language(&self) -> Option<String> {
        None
    }

    // ── Optional: LangHeuristics (language heuristics) ───────────────

    /// Whether the plugin maps module paths to stdlib categories.
    fn supports_stdlib_heuristic(&self) -> bool {
        false
    }

    /// Classify `module_path` (import path / entity name) as a standard-
    /// library item. Return the category name (e.g. `"Collection"`) or
    /// `None` to decline / mark not-stdlib.
    fn classify_stdlib(&self, _ctx: *mut std::ffi::c_void, _module_path: &str) -> Option<String> {
        None
    }

    /// Whether the plugin can decide test-file status by path/content.
    fn supports_test_file_heuristic(&self) -> bool {
        false
    }

    /// Decide whether `file_path`/`content` is a test file. `None` defers
    /// to the built-in path/AST rules.
    fn is_test_file(
        &self,
        _ctx: *mut std::ffi::c_void,
        _file_path: &str,
        _content: &str,
    ) -> Option<bool> {
        None
    }

    /// Whether the plugin maps tree-sitter capture names to entity kinds.
    fn supports_entity_kind_heuristic(&self) -> bool {
        false
    }

    /// Map a tree-sitter query capture name to an entity kind name
    /// (e.g. `"entity.tpl_block"` → `"function"`). `None` defers to the
    /// built-in capture→kind mapping.
    fn entity_kind(&self, _ctx: *mut std::ffi::c_void, _capture_name: &str) -> Option<String> {
        None
    }
}
