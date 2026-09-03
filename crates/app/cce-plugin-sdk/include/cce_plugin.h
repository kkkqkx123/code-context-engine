/*
 * cce_plugin.h — CCE Native Plugin ABI (v1)
 *
 * This header is the authoritative definition of the CCE native plugin
 * ABI. It is the single source of truth that the Rust SDK macro
 * (`declare_plugin!`) generates against and that the host loader
 * (`cce_infrastructure::plugin::native`) consumes.
 *
 * Any change to the exported symbols, signatures, or the JSON protocol
 * documented below must be reflected in all three places:
 *   1. this header,
 *   2. plugin-sdk/src/lib.rs (macro + helpers),
 *   3. crates/app/cce_plugin_runtime/src/native.rs (host loader).
 *
 * A plugin is a dynamic library (.so / .dylib / .dll) exporting the
 * functions below. The host loads it via dlopen/LoadLibrary and calls
 * the exported functions directly.
 */

#ifndef CCE_PLUGIN_H
#define CCE_PLUGIN_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * ── ABI version ──────────────────────────────────────────────────────────
 *
 * The host rejects plugins reporting an ABI version below its minimum
 * supported version. Plugins reporting a newer version are accepted with
 * a warning (the ABI is designed to be forward-compatible: capabilities
 * are opt-in via the `cce_plugin_has_*` exports and optional functions
 * are discovered by the host via symbol lookup).
 *
 * The project is in the development stage, so the ABI version history has
 * been reset to 1. Bump it on breaking upgrades; the host rejects plugins
 * reporting a version below its minimum and warns on plugins reporting a
 * newer version (capabilities are opt-in via the `cce_plugin_has_*` exports
 * and optional functions are discovered by the host via symbol lookup, so
 * the ABI is designed to be forward-compatible).
 */
#define CCE_PLUGIN_ABI_VERSION 1

/*
 * ── Required exports ─────────────────────────────────────────────────────
 *
 * The host loads these symbols unconditionally; a library missing any of
 * them is rejected.
 */

/*
 * Return the ABI version this plugin was built against. Must be ≥ the
 * host's minimum supported version.
 */
uint32_t cce_plugin_abi_version(void);

/*
 * Return the plugin metadata as a JSON C string. The payload is the RAW
 * PluginMetadata object — NOT wrapped in the FfiResult envelope used by
 * the generate functions:
 *
 *   {"id":"org/name","name":"...","version":"1.0.0","priority":10,
 *    "capability_priorities":{"fusion":5,"text_gen":1000},
 *    "description":"...","capabilities":["text_gen","entity_extract"]}
 *
 * `capabilities` is an optional array of declared capability facet names
 * (`text_gen` | `format_parse` | `entity_extract` | `ast_language` |
 * `group` | `chunk` | `rerank`); when absent the host probes the
 * `cce_plugin_has_*` exports at runtime.
 *
 * `capability_priorities` is an optional object mapping capability names to
 * per-capability priorities; capabilities not listed fall back to
 * `priority`. The host may override both via the `plugins.json` entry.
 *
 * Priorities are signed 32-bit integers. Higher values run first; 0 (the
 * default) keeps the plugin ahead of the built-in implementation. A
 * negative priority places the plugin *below* the built-in: at integration
 * points with a built-in fallback the plugin runs only after the built-in
 * produced no result.
 *
 * The caller must free the returned string via `cce_plugin_free_string`.
 */
char *cce_plugin_metadata(void);

/*
 * Whether the plugin implements BM25 natural-language generation.
 * If false, the host never calls `cce_plugin_generate_bm25*`.
 */
bool cce_plugin_has_bm25_generation(void);

/*
 * Whether the plugin implements embedding natural-language generation.
 * If false, the host never calls `cce_plugin_generate_embedding*`.
 */
bool cce_plugin_has_embedding_generation(void);

/*
 * Whether the plugin implements lifecycle management
 * (`cce_plugin_create` / `cce_plugin_destroy`). If false, the host
 * passes a null context to all generate calls.
 */
bool cce_plugin_has_lifecycle(void);

/*
 * Allocate an opaque context for this plugin instance. The returned
 * pointer is passed as the first argument to every generate call and is
 * freed via `cce_plugin_destroy`. Return NULL if no context is needed.
 *
 * The context is shared across threads: the host does NOT serialize
 * concurrent generate calls, so plugins MUST make their context
 * thread-safe.
 */
void *cce_plugin_create(void);

/*
 * Destroy a context previously returned by `cce_plugin_create`.
 * Receives NULL when no context was created.
 */
void cce_plugin_destroy(void *ctx);

/*
 * Free a C string previously returned by this plugin (either from
 * `cce_plugin_metadata` or from a generate function).
 */
void cce_plugin_free_string(char *ptr);

/*
 * ── Optional exports: NL generation ───────────────────────────────────────
 *
 * These symbols are discovered by the host via symbol lookup; they may
 * be absent. When present they MUST follow the FFI result protocol and
 * memory rules described below.
 *
 * All four generate functions share the signature:
 *   (void *ctx, const char *json) -> char *result_json
 *
 * `ctx` is the context returned by `cce_plugin_create` (NULL when the
 * plugin does not use lifecycle state). The input JSON is a UTF-8,
 * null-terminated C string. The returned string is allocated by the
 * plugin (malloc) and freed by the host via `cce_plugin_free_string`.
 */

/*
 * Generate BM25 natural-language text for one entity group.
 * `group_json` is a single JSON-serialized EntityGroup object.
 */
char *cce_plugin_generate_bm25(void *ctx, const char *group_json);

/*
 * Generate embedding natural-language text for one entity group.
 * `group_json` is a single JSON-serialized EntityGroup object.
 */
char *cce_plugin_generate_embedding(void *ctx, const char *group_json);

/*
 * Generate BM25 natural-language text for a batch of entity groups.
 * `groups_json` is a JSON ARRAY of EntityGroup objects:
 *
 *   [ {"group_id":"g1",...}, {"group_id":"g2",...} ]
 *
 * On success the result `value` MUST be an array of the SAME length as
 * the input; each element is either a JSON string (generated text) or
 * null (skip this group). Returning null for a group means the host
 * falls back to its built-in converter for that group only.
 */
char *cce_plugin_generate_bm25_batch(void *ctx, const char *groups_json);

/*
 * Generate embedding natural-language text for a batch of entity groups.
 * Same contract as `cce_plugin_generate_bm25_batch`.
 */
char *cce_plugin_generate_embedding_batch(void *ctx, const char *groups_json);

/*
 * ── Optional exports: capability guards ──────────────────────────────────
 *
 * Each capability entry point has a matching `cce_plugin_has_*` guard.
 * The host never calls a capability function whose guard is false (or
 * whose symbol is absent).
 */

/* Whether the plugin implements `FormatParse`. */
bool cce_plugin_has_parse(void);
/* Whether the plugin implements `EntityExtract`. */
bool cce_plugin_has_extract(void);
/* Whether the plugin implements the `Group` post-processing hook. */
bool cce_plugin_has_group(void);
/* Whether the plugin implements the `Chunk` override. */
bool cce_plugin_has_chunk(void);
/* Whether the plugin implements `Rerank`. */
bool cce_plugin_has_rerank(void);
/* Whether the plugin provides a custom tree-sitter language. */
bool cce_plugin_has_ast_language(void);
/* Whether the plugin implements the `GroupOverride` full-override tier. */
bool cce_plugin_has_group_override(void);
/* Whether the plugin implements `RelationExtract`. */
bool cce_plugin_has_relation_extract(void);
/* Whether the plugin implements `SymbolExtract` (import/export extraction). */
bool cce_plugin_has_symbol_extract(void);
/* Whether the plugin implements `QueryRewrite`. */
bool cce_plugin_has_query_rewrite(void);
/* Whether the plugin implements `Fusion`. */
bool cce_plugin_has_fusion(void);
/* Whether the plugin implements `ResultFilter`. */
bool cce_plugin_has_result_filter(void);
/* Whether the plugin implements `FileFilter`. */
bool cce_plugin_has_file_filter(void);

/*
 * ── Optional exports: capability entry points ─────────────────────────────
 *
 * All functions follow the FFI result protocol and memory rules below.
 * Returning `{"result":"none"}` declines the capability for this call.
 */

/*
 * Parse a document into a PluginDocument.
 *   (void *ctx, const char *content, const char *file_path) -> char *result_json
 *
 * PluginDocument schema:
 *   {"title":"...","language":"python",
 *    "entities":[{"id":"...","kind":"route","name":"/users",
 *                  "signature":"GET /users","doc_comment":"...",
 *                  "metadata":{},"span":{"start_byte":0,"end_byte":5,
 *                    "start_position":{"row":0,"column":0},
 *                    "end_position":{"row":0,"column":5}},
 *                  "children":[]}]}
 */
char *cce_plugin_parse_document(void *ctx, const char *content, const char *file_path);

/*
 * Extract supplementary entities from a code file.
 *   (void *ctx, const char *content, const char *file_path, const char *language)
 *     -> char *result_json
 *
 * Result `value` is a JSON array of PluginEntity objects (schema above).
 */
char *cce_plugin_extract_entities(void *ctx, const char *content, const char *file_path,
                                  const char *language);

/*
 * Post-process groups after built-in grouping.
 *   (void *ctx, const char *groups_json, const char *context_json) -> char *result_json
 *
 * `groups_json` is a JSON array of EntityGroup objects; `context_json` is
 * a GroupPluginContext: {"file_path":"...","language":"...","source":"..."}.
 * Result `value` is a JSON array of EntityGroup objects.
 */
char *cce_plugin_post_group(void *ctx, const char *groups_json, const char *context_json);

/*
 * Override chunking for converted groups.
 *   (void *ctx, const char *conversions_json, const char *file_path) -> char *result_json
 *
 * `conversions_json` is a JSON array of GroupConversions:
 *   [{"group":<EntityGroup>,"header_conversion":<ConversionResult|null>,
 *     "member_conversions":[<ConversionResult>,...]}]
 *
 * Result `value` is a JSON array of ChunkedResult:
 *   [{"chunk_id":"...","source_group_id":"...","path":"bm25",
 *     "group_type":"Standalone","chunk_index":0,"total_chunks":1,
 *     "text":"...","bm25_title":"...","bm25_keywords":[],"token_count":0,
 *     "start_byte":0,"end_byte":0,"self_contained":false,
 *     "metadata":{"content_type":"document","file_path":"...",
 *                  "source_span":{...},"segment_id":"..."}}]
 */
char *cce_plugin_chunk(void *ctx, const char *conversions_json, const char *file_path);

/*
 * Rerank query candidates.
 *   (void *ctx, const char *query, const char *candidates_json) -> char *result_json
 *
 * `candidates_json` is a JSON array of RerankCandidate:
 *   [{"id":"...","content":"...","file_path":"...","initial_score":0.5,
 *     "entity_type":"function","metadata":{}}]
 *
 * Result `value` is a RerankResult:
 *   {"reranked_candidates":[{"id":"...","rerank_score":0.9,
 *     "initial_score":0.5,"final_score":0.9,"rank_change":-1,
 *     "reasoning":"..."}]}
 */
char *cce_plugin_rerank(void *ctx, const char *query, const char *candidates_json);

/*
 * ── Optional exports: phase-2 capability entry points ────────────────────
 *
 * Same FFI result protocol and memory rules as the capability entry points.
 */

/*
 * Fully replace built-in grouping for a parsed file.
 *   (void *ctx, const char *context_json) -> char *result_json
 *
 * `context_json` is a GroupPluginContext extended with the serialized parsed
 * entities and raw relations:
 *   {"file_path":"...","language":"...","source":"...",
 *    "entities":[<PluginEntity>,...],"relations":[<PluginRelation>,...]}
 *
 * Result `value` is a JSON array of EntityGroup objects (built-in grouping is
 * skipped when the plugin returns a non-empty array).
 */
char *cce_plugin_group(void *ctx, const char *context_json);

/*
 * Extract supplementary symbols from a code file.
 *   (void *ctx, const char *content, const char *file_path, const char *language)
 *     -> char *result_json
 *
 * Result `value` is a JSON array of PluginSymbol:
 *   [{"id":"svc","name":"UserService","kind":"service","visibility":"public",
 *     "module_path":"app.services","location":{"span":{...}},
 *     "metadata":{},"children":[]}]
 */
char *cce_plugin_extract_symbols(void *ctx, const char *content, const char *file_path,
                                 const char *language);

/*
 * Extract explicit relations between symbols.
 *   (void *ctx, const char *content, const char *file_path, const char *language)
 *     -> char *result_json
 *
 * Result `value` is a JSON array of PluginRelation:
 *   [{"from":"svc","to":"repo","relation_type":"injects","metadata":{}}]
 * Unresolvable targets are dropped by the host resolver (never abort).
 */
char *cce_plugin_extract_relations(void *ctx, const char *content, const char *file_path,
                                   const char *language);

/*
 * Extract import statements from a code file (`SymbolExtract`).
 *   (void *ctx, const char *content, const char *file_path, const char *language)
 *     -> char *result_json
 *
 * Result `value` is a JSON array of PluginImport:
 *   [{"path":"std","symbols":null,"alias":null,"is_wildcard":false,"metadata":{}}]
 * Returning `{"result":"none"}` declines extraction (host uses no imports).
 */
char *cce_plugin_extract_imports(void *ctx, const char *content, const char *file_path,
                                 const char *language);

/*
 * Extract export declarations from a code file (`SymbolExtract`).
 *   (void *ctx, const char *content, const char *file_path, const char *language)
 *     -> char *result_json
 *
 * Result `value` is a JSON array of PluginExport:
 *   [{"name":"main","kind":"function","visibility":"public",
 *     "location":null,"metadata":{}}]
 */
char *cce_plugin_extract_exports(void *ctx, const char *content, const char *file_path,
                                 const char *language);

/*
 * Rewrite / expand a query before recall.
 *   (void *ctx, const char *query) -> char *result_json
 *
 * Result `value` is a QueryRewriteResult:
 *   {"rewritten_query":"...","expansion_terms":["...",...]}
 */
char *cce_plugin_rewrite_query(void *ctx, const char *query);

/*
 * Override hybrid fusion weights.
 *   (void *ctx, const char *query, size_t vector_count, size_t bm25_count)
 *     -> char *result_json
 *
 * Result `value` is a FusionWeights:
 *   {"vector_weight":0.7,"bm25_weight":0.3,"min_score":0.2}
 * Weights are validated to [0,1] by the host before use.
 */
char *cce_plugin_fusion_weights(void *ctx, const char *query, size_t vector_count,
                                size_t bm25_count);

/*
 * Filter / boost / annotate candidates after reranking.
 *   (void *ctx, const char *query, const char *results_json) -> char *result_json
 *
 * `results_json` is a JSON array of RerankCandidate. Result `value` is a JSON
 * array of ResultFilterEntry:
 *   [{"id":"...","remove":true,"boost":0.1,"note":"..."}]
 */
char *cce_plugin_filter_results(void *ctx, const char *query, const char *results_json);

/*
 * Decide whether a path should be included/excluded during scanning.
 *   (void *ctx, const char *file_path, bool is_directory, uint64_t size)
 *     -> char *result_json
 *
 * Result `value` is a FileFilterDecision:
 *   "include" | "exclude" | "neutral"  (or {"result":"none"} to defer).
 */
char *cce_plugin_filter_file(void *ctx, const char *file_path, bool is_directory,
                             uint64_t size);

/*
 * Return the tree-sitter query string for a query type (index 0..7:
 * entity, call, control_flow, behavior, dependency, comment, embedded,
 * structural), or NULL when no scheme is provided for that type.
 *   (void *ctx, uint32_t query_type) -> char *query_string_or_null
 *
 * The returned string is allocated by the plugin and freed by the host.
 */
char *cce_plugin_query_scheme(void *ctx, uint32_t query_type);

/*
 * Return a raw pointer to the tree-sitter TSLanguage for the custom
 * language, or NULL. The pointer must remain valid for the plugin's
 * lifetime. This is a bare C pointer crossing the FFI boundary — it is
 * only ever re-imported into tree-sitter by the host and never freed by
 * the host.
 */
const void *cce_plugin_tree_sitter_language(void);

/*
 * Return the custom language name (e.g. "zig") as a C string, or NULL.
 */
char *cce_plugin_language_name(void);

/*
 * Return the custom language extensions as a JSON array, or NULL:
 *   ["zig","zir"]
 */
char *cce_plugin_language_extensions(void);

/*
 * Whether the plugin remaps a custom language onto a host built-in
 * grammar (no tree-sitter grammar is embedded in the plugin). When true,
 * `cce_plugin_language_name` / `cce_plugin_language_extensions` describe
 * the custom language and `cce_plugin_remap_grammar_language` names the
 * host built-in language backing it.
 */
bool cce_plugin_has_language_remap(void);

/*
 * Return the host built-in language name whose grammar backs the custom
 * language (e.g. "JavaScript"), as a C string, or NULL. Only consulted
 * when `cce_plugin_has_language_remap` returns true.
 */
char *cce_plugin_remap_grammar_language(void);

/*
 * ── LangHeuristics (language heuristics) ─────────────────────────────────
 *
 * Three independent, optional hooks that improve custom-language precision.
 * Each has its own capability guard; unexported hooks are simply skipped.
 */

/* Whether the plugin maps module paths to stdlib categories. */
bool cce_plugin_has_stdlib_heuristic(void);

/*
 * Classify `module_path` (import path / entity name) as a standard-library
 * item. Return the category name ("Collection", "Io", "Concurrency",
 * "Utility", "String", "Numeric", "Error", "Macro", "Trait", "Other") as a
 * C string, or NULL to decline / mark not-stdlib.
 *   (void *ctx, const char *module_path) -> char *category_or_null
 */
char *cce_plugin_classify_stdlib(void *ctx, const char *module_path);

/* Whether the plugin can decide test-file status by path/content. */
bool cce_plugin_has_test_file_heuristic(void);

/*
 * Decide whether `file_path`/`content` is a test file. Return the C string
 * "true" or "false", or NULL to defer to the built-in path/AST rules.
 *   (void *ctx, const char *file_path, const char *content)
 */
char *cce_plugin_is_test_file(void *ctx, const char *file_path, const char *content);

/* Whether the plugin maps tree-sitter capture names to entity kinds. */
bool cce_plugin_has_entity_kind_heuristic(void);

/*
 * Map a tree-sitter query capture name (e.g. "entity.tpl_block") to an
 * entity kind name ("function", "class", "enum_variant", ...), or NULL to
 * defer to the built-in capture→kind mapping.
 *   (void *ctx, const char *capture_name) -> char *kind_or_null
 */
char *cce_plugin_entity_kind(void *ctx, const char *capture_name);

/*
 * ── FFI result protocol ──────────────────────────────────────────────────
 *
 * Every capability function returns a JSON string in one of these shapes:
 *
 *   // success, with a value (single: value is the text string;
 *   // batch: value is an array of strings / null)
 *   {"result":"ok","value":<json>}
 *
 *   // success, no value (host treats the group(s) as uncovered and
 *   // falls back to its built-in converter)
 *   {"result":"none"}
 *
 *   // error — the host records the error and continues with the
 *   // built-in converter / logs it. `error_type` maps to the host's
 *   // PluginError variant: "script" | "timeout" | "invalid_output" |
 *   // "logic" | "resource" | "circuit_broken" | "not_found" |
 *   // "execution_failed" (defaults to "script" when absent).
 *   {"result":"error","message":"human-readable message","error_type":"script"}
 *
 * ── Memory rules ─────────────────────────────────────────────────────────
 *
 * - The plugin allocates every returned char* (malloc) and the host
 *   frees it via `cce_plugin_free_string`.
 * - The plugin must NOT free or modify strings passed to it.
 *
 * ── Thread-safety ────────────────────────────────────────────────────────
 *
 * The host may call the generate functions concurrently from multiple
 * threads using the SAME context pointer. Plugin implementations and
 * their context MUST be thread-safe. This is an explicit ABI contract.
 */

#ifdef __cplusplus
}
#endif

#endif /* CCE_PLUGIN_H */
