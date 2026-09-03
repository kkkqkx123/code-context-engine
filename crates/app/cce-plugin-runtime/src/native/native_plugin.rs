//! Native dynamic library plugin support
//!
//! This module provides full native plugin support via `libloading`.
//! Native plugins are dynamic libraries (.so / .dll / .dylib) that
//! export a set of C-compatible functions defined by the CCE Native
//! Plugin ABI. The authoritative ABI definition lives in
//! `plugin-sdk/include/cce_plugin.h`; keep this module, the header, and
//! `plugin-sdk/src/lib.rs` (the `declare_plugin!` macro) in sync.
//!
//! # ABI Overview
//!
//! Libraries **must** export these symbols:
//!
//! | Symbol | Type | Required |
//! |--------|------|----------|
//! | `cce_plugin_abi_version` | `fn() -> u32` | ✅ |
//! | `cce_plugin_metadata` | `fn() -> *mut c_char` | ✅ |
//! | `cce_plugin_has_bm25_generation` | `fn() -> bool` | ✅ |
//! | `cce_plugin_has_embedding_generation` | `fn() -> bool` | ✅ |
//! | `cce_plugin_has_lifecycle` | `fn() -> bool` | ✅ |
//! | `cce_plugin_free_string` | `fn(*mut c_char)` | ✅ |
//! | `cce_plugin_generate_bm25` | `fn(*mut c_void, *const c_char) -> *mut c_char` | ❌ |
//! | `cce_plugin_generate_embedding` | `fn(*mut c_void, *const c_char) -> *mut c_char` | ❌ |
//! | `cce_plugin_generate_bm25_batch` | `fn(*mut c_void, *const c_char) -> *mut c_char` | ❌ |
//! | `cce_plugin_generate_embedding_batch` | `fn(*mut c_void, *const c_char) -> *mut c_char` | ❌ |
//!
//! The generate functions receive the opaque context returned by
//! `cce_plugin_create` as their first argument, making plugin instance state
//! usable across calls. The batch entry points take a JSON array of entity
//! groups and return a same-length array of texts / nulls. Optional functions
//! are loaded via `library.find()`; if not exported, the corresponding
//! `CodePlugin` method falls back to per-group calls (or returns `Ok(None)`).
//!
//! # FFI Result Protocol
//!
//! All optional functions returning `*mut c_char` must return a JSON string
//! in the following format:
//!
//! ```json
//! {"result":"ok","value":<T>}
//! {"result":"none"}
//! {"result":"error","message":"...","error_type":"script"}
//! ```
//!
//! `error_type` restores the concrete [`PluginError`] variant on the host
//! side (one of `script | timeout | invalid_output | logic | resource |
//! circuit_broken | not_found | execution_failed`; defaults to `script`).
//!
//! The plugin allocates the string with `CString::into_raw()`; the host
//! frees it by calling `cce_plugin_free_string`.

use std::ffi::CStr;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use libloading::Library;

use crate::error::PluginError;
use crate::types::PluginMetadata;
use crate::utils::execute_with_timeout_blocking;
use cce_metrics::PluginMetrics;

use super::ffi_helpers::{
    AbiVersionFn, CreateContextFn, DestroyContextFn, FilterFileFn, FreeStringFn, FusionWeightsFn,
    HasCapabilityFn, MetadataFn, PluginContext, PluginStringFn, PluginStringFn2, PluginStringFn3,
    QuerySchemeFn, SendPtr, StringOnlyFn, TreeSitterLangFn, call_owned_string_fn1,
    call_owned_string_only, call_plugin_string, capability_label, parse_ffi_json_result,
    symbol_missing_err,
};

/// Minimum supported ABI version (inclusive).
///
/// The project is in the development stage, so the ABI version history has
/// been reset to 1. Plugins reporting a version below this are rejected
/// (no backward compatibility is guaranteed during development).
pub(crate) const MINIMUM_ABI_VERSION: u32 = 1;

/// The highest ABI version this host was built against.
///
/// Plugins reporting a version above this are still accepted (the
/// ABI is designed to be forward-compatible) but a warning is logged
/// so the operator knows the plugin was built against a newer SDK.
pub(crate) const CURRENT_ABI_VERSION: u32 = 1;

/// Default per-call timeout for native FFI calls (milliseconds).
///
/// The FFI call itself cannot be interrupted; the worker thread lingers
/// until the plugin returns, but the caller proceeds after this budget.
const NATIVE_TIMEOUT_MS: u64 = 5_000;

/// A loaded native plugin, wrapping a dynamic library handle.
///
/// All function pointers are extracted at load time so the library
/// handle only needs to stay alive (no concurrent access to `Library` itself).
///
/// If the plugin supports lifecycle management (`cce_plugin_create`/`cce_plugin_destroy`),
/// the context pointer is stored here and automatically freed on drop.
pub struct NativePlugin {
    /// Plugin metadata parsed from the library's `cce_plugin_metadata()` export.
    pub(crate) metadata: PluginMetadata,

    /// Keep the library loaded so function pointers remain valid.
    pub(crate) _library: Arc<Library>,

    // ── Cached capability bits (read once at load, no FFI on query) ──
    /// Whether the plugin reports BM25 generation support.
    pub(crate) has_bm25: bool,
    /// Whether the plugin reports embedding generation support.
    pub(crate) has_embedding: bool,
    /// Whether the plugin reports `FormatParse` support.
    pub(crate) has_parse: bool,
    /// Whether the plugin reports `EntityExtract` support.
    pub(crate) has_extract: bool,
    /// Whether the plugin reports `Group` support.
    pub(crate) has_group: bool,
    /// Whether the plugin reports `GroupOverride` support.
    pub(crate) has_group_override: bool,
    /// Whether the plugin reports `Chunk` support.
    pub(crate) has_chunk: bool,
    /// Whether the plugin reports `Rerank` support.
    pub(crate) has_rerank: bool,
    /// Whether the plugin reports `AstLanguage` support.
    pub(crate) has_ast_language: bool,
    /// Whether the plugin reports `LanguageRemap` support.
    pub(crate) has_language_remap: bool,
    /// Whether the plugin reports the `LangHeuristics` stdlib hook.
    pub(crate) has_stdlib_heuristic: bool,
    /// Whether the plugin reports the `LangHeuristics` test-file hook.
    pub(crate) has_test_file_heuristic: bool,
    /// Whether the plugin reports the `LangHeuristics` entity-kind hook.
    pub(crate) has_entity_kind_heuristic: bool,
    /// Whether the plugin reports `RelationExtract` support.
    pub(crate) has_relation_extract: bool,
    /// Whether the plugin reports `SymbolExtract` support.
    pub(crate) has_symbol_extract: bool,
    /// Whether the plugin reports `QueryRewrite` support.
    pub(crate) has_query_rewrite: bool,
    /// Whether the plugin reports `Fusion` support.
    pub(crate) has_fusion: bool,
    /// Whether the plugin reports `ResultFilter` support.
    pub(crate) has_result_filter: bool,
    /// Whether the plugin reports `FileFilter` support.
    pub(crate) has_file_filter: bool,

    // ── Required function pointers ──
    /// `cce_plugin_free_string` symbol.
    pub(crate) free_string_fn: FreeStringFn,

    // ── Optional function pointers ──
    /// `cce_plugin_generate_bm25` — None if not exported.
    pub(crate) generate_bm25_fn: Option<PluginStringFn>,
    /// `cce_plugin_generate_embedding` — None if not exported.
    pub(crate) generate_embedding_fn: Option<PluginStringFn>,
    /// `cce_plugin_generate_bm25_batch` — None if not exported.
    pub(crate) generate_bm25_batch_fn: Option<PluginStringFn>,
    /// `cce_plugin_generate_embedding_batch` — None if not exported.
    pub(crate) generate_embedding_batch_fn: Option<PluginStringFn>,
    /// `cce_plugin_parse_document` — None if not exported.
    pub(crate) parse_document_fn: Option<PluginStringFn2>,
    /// `cce_plugin_extract_entities` — None if not exported.
    pub(crate) extract_entities_fn: Option<PluginStringFn3>,
    /// `cce_plugin_post_group` — None if not exported.
    pub(crate) post_group_fn: Option<PluginStringFn2>,
    /// `cce_plugin_group` — None if not exported.
    pub(crate) group_fn: Option<PluginStringFn>,
    /// `cce_plugin_chunk` — None if not exported.
    pub(crate) chunk_fn: Option<PluginStringFn2>,
    /// `cce_plugin_rerank` — None if not exported.
    pub(crate) rerank_fn: Option<PluginStringFn2>,
    /// `cce_plugin_extract_symbols` — None if not exported.
    pub(crate) extract_symbols_fn: Option<PluginStringFn3>,
    /// `cce_plugin_extract_relations` — None if not exported.
    pub(crate) extract_relations_fn: Option<PluginStringFn3>,
    /// `cce_plugin_extract_imports` — None if not exported.
    pub(crate) extract_imports_fn: Option<PluginStringFn3>,
    /// `cce_plugin_extract_exports` — None if not exported.
    pub(crate) extract_exports_fn: Option<PluginStringFn3>,
    /// `cce_plugin_rewrite_query` — None if not exported.
    pub(crate) rewrite_query_fn: Option<PluginStringFn>,
    /// `cce_plugin_fusion_weights` — None if not exported.
    pub(crate) fusion_weights_fn: Option<FusionWeightsFn>,
    /// `cce_plugin_filter_results` — None if not exported.
    pub(crate) filter_results_fn: Option<PluginStringFn2>,
    /// `cce_plugin_filter_file` — None if not exported.
    pub(crate) filter_file_fn: Option<FilterFileFn>,
    /// `cce_plugin_tree_sitter_language` — None if not exported.
    pub(crate) tree_sitter_language_fn: Option<TreeSitterLangFn>,
    /// `cce_plugin_remap_grammar_language` — None if not exported.
    pub(crate) remap_grammar_language_fn: Option<StringOnlyFn>,
    /// `cce_plugin_classify_stdlib` — None if not exported.
    pub(crate) classify_stdlib_fn: Option<PluginStringFn>,
    /// `cce_plugin_is_test_file` — None if not exported.
    pub(crate) is_test_file_fn: Option<PluginStringFn2>,
    /// `cce_plugin_entity_kind` — None if not exported.
    pub(crate) entity_kind_fn: Option<PluginStringFn>,
    /// `cce_plugin_destroy` — None if not exported.
    pub(crate) destroy_ctx_fn: Option<DestroyContextFn>,

    // ── Cached custom-language data (read once at load) ──
    /// Cached query schemes keyed by [`cce_types::QueryType`].
    pub(crate) query_schemes: std::collections::HashMap<cce_types::QueryType, String>,
    /// Cached custom language name (if any).
    pub(crate) language_name: Option<String>,
    /// Cached custom language extensions (if any).
    pub(crate) language_extensions: Vec<String>,
    /// Cached remap target (host built-in language name), if `LanguageRemap`.
    pub(crate) remap_grammar_language: Option<String>,

    // ── Lifecycle context ──
    /// Opaque plugin context, if `cce_plugin_create` returned non-null.
    pub(crate) context: Option<PluginContext>,

    // ── Runtime metrics ──
    /// Optional metrics sink for execution accounting.
    pub(crate) metrics: Option<Arc<PluginMetrics>>,
}

// SAFETY: `Library` is not `Sync`, but we never access it after
// construction. All function pointers are immutable and the library
// handle is only used for keeping the library loaded.  The `context`
// field is `Send + Sync` (see `PluginContext`).  The host must ensure
// that the plugin's FFI functions are thread-safe per the ABI contract.
unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

impl Drop for NativePlugin {
    fn drop(&mut self) {
        if let Some(ctx) = &self.context {
            if let Some(destroy_fn) = self.destroy_ctx_fn {
                // SAFETY: `destroy_fn` comes from the same library and
                // `context` was created by `cce_plugin_create`.
                unsafe { destroy_fn(ctx.0) };
            }
        }
    }
}

impl NativePlugin {
    /// Load a native plugin from a dynamic library file.
    ///
    /// Opens the library, finds all required symbols, validates the ABI
    /// version, extracts plugin metadata, and discovers optional exports.
    ///
    /// # Errors
    ///
    /// Returns `PluginError::ResourceError` if:
    /// - The library cannot be opened.
    /// - A required symbol is missing.
    /// - The ABI version is unsupported.
    /// - The metadata JSON cannot be parsed.
    ///
    /// # Safety
    ///
    /// Loading a dynamic library and calling its exported functions is
    /// inherently unsafe. The caller must ensure the library originates
    /// from a trusted source.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        // ── 1. Open the dynamic library ──
        let library = unsafe {
            Library::new(path).map_err(|e| {
                PluginError::ResourceError(format!(
                    "Failed to load native plugin from {}: {}",
                    path.display(),
                    e
                ))
            })?
        };
        let library = Arc::new(library);

        // ── 2. Load required symbols ──

        // cce_plugin_abi_version
        let abi_version_fn: AbiVersionFn = *unsafe {
            library
                .get(b"cce_plugin_abi_version\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_abi_version"))?
        };

        // cce_plugin_metadata
        let metadata_fn: MetadataFn = *unsafe {
            library
                .get(b"cce_plugin_metadata\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_metadata"))?
        };

        // cce_plugin_has_bm25_generation
        let has_bm25_fn: HasCapabilityFn = *unsafe {
            library
                .get(b"cce_plugin_has_bm25_generation\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_has_bm25_generation"))?
        };

        // cce_plugin_has_embedding_generation
        let has_embedding_fn: HasCapabilityFn = *unsafe {
            library
                .get(b"cce_plugin_has_embedding_generation\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_has_embedding_generation"))?
        };

        // cce_plugin_free_string
        let free_string_fn: FreeStringFn = *unsafe {
            library
                .get(b"cce_plugin_free_string\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_free_string"))?
        };

        // cce_plugin_has_lifecycle
        let has_lifecycle_fn: HasCapabilityFn = *unsafe {
            library
                .get(b"cce_plugin_has_lifecycle\0")
                .map_err(|_| symbol_missing_err(path, "cce_plugin_has_lifecycle"))?
        };

        // ── 3. Validate ABI version ──
        let abi_version = unsafe { abi_version_fn() };
        if abi_version < MINIMUM_ABI_VERSION {
            return Err(PluginError::ResourceError(format!(
                "Native plugin '{}' reports ABI version {} which is below the minimum supported version {}",
                path.display(),
                abi_version,
                MINIMUM_ABI_VERSION
            )));
        }
        if abi_version > CURRENT_ABI_VERSION {
            tracing::warn!(
                "Native plugin '{}' was built against ABI version {} (newer than host's {}); forward-compatibility assumed",
                path.display(),
                abi_version,
                CURRENT_ABI_VERSION
            );
        }

        // ── 4. Extract metadata ──
        let metadata_json_ptr = unsafe { metadata_fn() };
        let metadata: PluginMetadata = {
            let c_str = unsafe { CStr::from_ptr(metadata_json_ptr) };
            let json_str = c_str.to_str().map_err(|_| {
                PluginError::InvalidOutput("Plugin metadata is not valid UTF-8".to_string())
            })?;
            let meta: PluginMetadata = serde_json::from_str(json_str).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to parse plugin metadata JSON: {}", e))
            })?;
            unsafe { free_string_fn(metadata_json_ptr) };
            meta
        };

        // ── 5. Load optional symbols ──

        // cce_plugin_generate_bm25
        let generate_bm25_fn: Option<PluginStringFn> =
            unsafe { library.get(b"cce_plugin_generate_bm25\0").ok().map(|s| *s) };

        // cce_plugin_generate_embedding
        let generate_embedding_fn: Option<PluginStringFn> = unsafe {
            library
                .get(b"cce_plugin_generate_embedding\0")
                .ok()
                .map(|s| *s)
        };

        // cce_plugin_generate_bm25_batch
        let generate_bm25_batch_fn: Option<PluginStringFn> = unsafe {
            library
                .get(b"cce_plugin_generate_bm25_batch\0")
                .ok()
                .map(|s| *s)
        };

        // cce_plugin_generate_embedding_batch
        let generate_embedding_batch_fn: Option<PluginStringFn> = unsafe {
            library
                .get(b"cce_plugin_generate_embedding_batch\0")
                .ok()
                .map(|s| *s)
        };

        // cce_plugin_has_parse
        let has_parse_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_parse\0").ok().map(|s| *s) };
        // cce_plugin_has_extract
        let has_extract_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_extract\0").ok().map(|s| *s) };
        // cce_plugin_has_group
        let has_group_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_group\0").ok().map(|s| *s) };
        // cce_plugin_has_chunk
        let has_chunk_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_chunk\0").ok().map(|s| *s) };
        // cce_plugin_has_rerank
        let has_rerank_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_rerank\0").ok().map(|s| *s) };
        // cce_plugin_has_ast_language
        let has_ast_language_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_ast_language\0")
                .ok()
                .map(|s| *s)
        };
        // cce_plugin_has_language_remap
        let has_language_remap_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_language_remap\0")
                .ok()
                .map(|s| *s)
        };
        // LangHeuristics capability guards
        let has_stdlib_heuristic_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_stdlib_heuristic\0")
                .ok()
                .map(|s| *s)
        };
        let has_test_file_heuristic_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_test_file_heuristic\0")
                .ok()
                .map(|s| *s)
        };
        let has_entity_kind_heuristic_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_entity_kind_heuristic\0")
                .ok()
                .map(|s| *s)
        };
        // Capability guards
        let has_group_override_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_group_override\0")
                .ok()
                .map(|s| *s)
        };
        let has_relation_extract_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_relation_extract\0")
                .ok()
                .map(|s| *s)
        };
        let has_symbol_extract_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_symbol_extract\0")
                .ok()
                .map(|s| *s)
        };
        let has_query_rewrite_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_query_rewrite\0")
                .ok()
                .map(|s| *s)
        };
        let has_fusion_fn: Option<HasCapabilityFn> =
            unsafe { library.get(b"cce_plugin_has_fusion\0").ok().map(|s| *s) };
        let has_result_filter_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_result_filter\0")
                .ok()
                .map(|s| *s)
        };
        let has_file_filter_fn: Option<HasCapabilityFn> = unsafe {
            library
                .get(b"cce_plugin_has_file_filter\0")
                .ok()
                .map(|s| *s)
        };

        // cce_plugin_parse_document / post_group / chunk / rerank
        let parse_document_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_parse_document\0").ok().map(|s| *s) };
        let extract_entities_fn: Option<PluginStringFn3> = unsafe {
            library
                .get(b"cce_plugin_extract_entities\0")
                .ok()
                .map(|s| *s)
        };
        let post_group_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_post_group\0").ok().map(|s| *s) };
        let group_fn: Option<PluginStringFn> =
            unsafe { library.get(b"cce_plugin_group\0").ok().map(|s| *s) };
        let chunk_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_chunk\0").ok().map(|s| *s) };
        let rerank_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_rerank\0").ok().map(|s| *s) };
        let extract_symbols_fn: Option<PluginStringFn3> = unsafe {
            library
                .get(b"cce_plugin_extract_symbols\0")
                .ok()
                .map(|s| *s)
        };
        let extract_relations_fn: Option<PluginStringFn3> = unsafe {
            library
                .get(b"cce_plugin_extract_relations\0")
                .ok()
                .map(|s| *s)
        };
        let extract_imports_fn: Option<PluginStringFn3> = unsafe {
            library
                .get(b"cce_plugin_extract_imports\0")
                .ok()
                .map(|s| *s)
        };
        let extract_exports_fn: Option<PluginStringFn3> = unsafe {
            library
                .get(b"cce_plugin_extract_exports\0")
                .ok()
                .map(|s| *s)
        };
        let rewrite_query_fn: Option<PluginStringFn> =
            unsafe { library.get(b"cce_plugin_rewrite_query\0").ok().map(|s| *s) };
        let fusion_weights_fn: Option<FusionWeightsFn> =
            unsafe { library.get(b"cce_plugin_fusion_weights\0").ok().map(|s| *s) };
        let filter_results_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_filter_results\0").ok().map(|s| *s) };
        let filter_file_fn: Option<FilterFileFn> =
            unsafe { library.get(b"cce_plugin_filter_file\0").ok().map(|s| *s) };

        // AstLanguage symbols
        let query_scheme_fn: Option<QuerySchemeFn> =
            unsafe { library.get(b"cce_plugin_query_scheme\0").ok().map(|s| *s) };
        let tree_sitter_language_fn: Option<TreeSitterLangFn> = unsafe {
            library
                .get(b"cce_plugin_tree_sitter_language\0")
                .ok()
                .map(|s| *s)
        };
        let language_name_fn: Option<StringOnlyFn> =
            unsafe { library.get(b"cce_plugin_language_name\0").ok().map(|s| *s) };
        let language_extensions_fn: Option<StringOnlyFn> = unsafe {
            library
                .get(b"cce_plugin_language_extensions\0")
                .ok()
                .map(|s| *s)
        };
        let remap_grammar_language_fn: Option<StringOnlyFn> = unsafe {
            library
                .get(b"cce_plugin_remap_grammar_language\0")
                .ok()
                .map(|s| *s)
        };
        // LangHeuristics entry points
        let classify_stdlib_fn: Option<PluginStringFn> = unsafe {
            library
                .get(b"cce_plugin_classify_stdlib\0")
                .ok()
                .map(|s| *s)
        };
        let is_test_file_fn: Option<PluginStringFn2> =
            unsafe { library.get(b"cce_plugin_is_test_file\0").ok().map(|s| *s) };
        let entity_kind_fn: Option<PluginStringFn> =
            unsafe { library.get(b"cce_plugin_entity_kind\0").ok().map(|s| *s) };

        // cce_plugin_create / cce_plugin_destroy (lifecycle)
        let create_ctx_fn: Option<CreateContextFn> =
            unsafe { library.get(b"cce_plugin_create\0").ok().map(|s| *s) };
        let destroy_ctx_fn: Option<DestroyContextFn> =
            unsafe { library.get(b"cce_plugin_destroy\0").ok().map(|s| *s) };

        // ── 6. Query capability bits once and cache them ──
        let has_bm25 = unsafe { has_bm25_fn() };
        let has_embedding = unsafe { has_embedding_fn() };
        let has_parse = has_parse_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_extract = has_extract_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_group = has_group_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_chunk = has_chunk_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_rerank = has_rerank_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_ast_language = has_ast_language_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_language_remap = has_language_remap_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_stdlib_heuristic = has_stdlib_heuristic_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_test_file_heuristic = has_test_file_heuristic_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_entity_kind_heuristic = has_entity_kind_heuristic_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_group_override = has_group_override_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_relation_extract = has_relation_extract_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_symbol_extract = has_symbol_extract_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_query_rewrite = has_query_rewrite_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_fusion = has_fusion_fn.map(|f| unsafe { f() }).unwrap_or(false);
        let has_result_filter = has_result_filter_fn
            .map(|f| unsafe { f() })
            .unwrap_or(false);
        let has_file_filter = has_file_filter_fn.map(|f| unsafe { f() }).unwrap_or(false);

        // ── 7. Create plugin context if lifecycle is supported ──
        let context = if unsafe { has_lifecycle_fn() } {
            match create_ctx_fn {
                Some(create_fn) => {
                    let ptr = unsafe { create_fn() };
                    if ptr.is_null() {
                        None
                    } else {
                        Some(PluginContext(ptr))
                    }
                }
                None => None,
            }
        } else {
            None
        };

        // ── 8. Cache custom-language data (query schemes, name, extensions) ──
        let context_ptr = context.as_ref().map(|c| SendPtr(c.0));
        let mut query_schemes = std::collections::HashMap::new();
        if let Some(qs_fn) = query_scheme_fn {
            for qt in cce_types::QueryType::ALL {
                // SAFETY: `qs_fn` comes from the same library; `context_ptr`
                // was created by that library (or is null when unused).
                let raw = unsafe {
                    call_owned_string_fn1(
                        |ctx| qs_fn(ctx, qt.as_u32()),
                        free_string_fn,
                        context_ptr,
                    )
                };
                if let Ok(Some(scheme)) = raw {
                    if !scheme.is_empty() {
                        query_schemes.insert(qt, scheme);
                    }
                }
            }
        }
        let language_name = language_name_fn.and_then(|f| {
            // SAFETY: `f` is a valid function pointer from the same library.
            let raw = unsafe { call_owned_string_only(f, free_string_fn, "language_name") };
            raw.ok().flatten()
        });
        let language_extensions = language_extensions_fn
            .and_then(|f| {
                // SAFETY: `f` is a valid function pointer from the same library.
                let raw =
                    unsafe { call_owned_string_only(f, free_string_fn, "language_extensions") };
                raw.ok().flatten()
            })
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default();
        let remap_grammar_language = remap_grammar_language_fn.and_then(|f| {
            // SAFETY: `f` is a valid function pointer from the same library.
            let raw =
                unsafe { call_owned_string_only(f, free_string_fn, "remap_grammar_language") };
            raw.ok().flatten()
        });

        Ok(Self {
            metadata,
            _library: library,
            has_bm25,
            has_embedding,
            has_parse,
            has_extract,
            has_group,
            has_group_override,
            has_chunk,
            has_rerank,
            has_ast_language,
            has_language_remap,
            has_stdlib_heuristic,
            has_test_file_heuristic,
            has_entity_kind_heuristic,
            has_relation_extract,
            has_symbol_extract,
            has_query_rewrite,
            has_fusion,
            has_result_filter,
            has_file_filter,
            free_string_fn,
            generate_bm25_fn,
            generate_embedding_fn,
            generate_bm25_batch_fn,
            generate_embedding_batch_fn,
            parse_document_fn,
            extract_entities_fn,
            post_group_fn,
            group_fn,
            chunk_fn,
            rerank_fn,
            extract_symbols_fn,
            extract_relations_fn,
            extract_imports_fn,
            extract_exports_fn,
            rewrite_query_fn,
            fusion_weights_fn,
            filter_results_fn,
            filter_file_fn,
            tree_sitter_language_fn,
            remap_grammar_language_fn,
            classify_stdlib_fn,
            is_test_file_fn,
            entity_kind_fn,
            destroy_ctx_fn,
            query_schemes,
            language_name,
            language_extensions,
            remap_grammar_language,
            context,
            metrics: None,
        })
    }

    /// Attach an optional metrics sink for execution accounting.
    pub fn with_metrics(mut self, metrics: Arc<PluginMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Run `f` on a dedicated thread with a hard timeout and record metrics.
    ///
    /// The thread cannot be forcefully terminated; on timeout it lingers
    /// until the FFI call returns naturally. The caller proceeds after
    /// [`NATIVE_TIMEOUT_MS`].
    pub(crate) fn execute_with_timeout<T>(
        &self,
        operation: &str,
        f: impl FnOnce() -> Result<T, PluginError> + Send + 'static,
    ) -> Result<T, PluginError>
    where
        T: Send + 'static,
    {
        let plugin_id = self.metadata.id.clone();
        let start = Instant::now();
        let result =
            execute_with_timeout_blocking(|_token| f(), NATIVE_TIMEOUT_MS, &plugin_id, operation);
        if let Some(m) = &self.metrics {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            match &result {
                Ok(_) => {
                    m.record_capability_execution(capability_label(operation), latency_ms, true)
                }
                Err(_) => {
                    m.record_capability_execution(capability_label(operation), latency_ms, false);
                    m.record_execution_error(&plugin_id);
                }
            }
        }
        result
    }
}

// ── FFI dispatch helpers ─────────────────────────────────────────────────

impl NativePlugin {
    /// Call a single-group FFI function and parse its text result.
    pub(crate) fn call_single_fn(
        &self,
        func: PluginStringFn,
        operation: &str,
        group_json: String,
        result_kind: &str,
    ) -> Result<Option<String>, PluginError> {
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let result_kind = result_kind.to_string();
        self.execute_with_timeout(operation, move || {
            let json_str = unsafe { call_plugin_string(func, free_string_fn, ctx, &group_json) }?;
            let val = parse_ffi_json_result(&json_str)?;
            match val {
                Some(v) => {
                    let text: String = serde_json::from_value(v).map_err(|e| {
                        PluginError::InvalidOutput(format!(
                            "Failed to deserialize {result_kind} result: {e}"
                        ))
                    })?;
                    Ok(Some(text))
                }
                None => Ok(None),
            }
        })
    }

    /// Call a batch FFI function and parse a same-length array of texts/nulls.
    ///
    /// A `{"result":"none"}` reply is interpreted as "skip every group".
    pub(crate) fn call_batch_fn(
        &self,
        func: PluginStringFn,
        operation: &str,
        groups_json: String,
        expected_len: usize,
        result_kind: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let result_kind = result_kind.to_string();
        self.execute_with_timeout(operation, move || {
            let json_str = unsafe { call_plugin_string(func, free_string_fn, ctx, &groups_json) }?;
            let val = parse_ffi_json_result(&json_str)?;
            match val {
                Some(v) => {
                    let results: Vec<Option<String>> = serde_json::from_value(v).map_err(|e| {
                        PluginError::InvalidOutput(format!(
                            "Failed to deserialize {result_kind} batch result: {e}"
                        ))
                    })?;
                    if results.len() != expected_len {
                        return Err(PluginError::InvalidOutput(format!(
                            "{result_kind} batch result length mismatch: expected {expected_len}, got {}",
                            results.len()
                        )));
                    }
                    Ok(results)
                }
                None => Ok(vec![None; expected_len]),
            }
        })
    }
}
