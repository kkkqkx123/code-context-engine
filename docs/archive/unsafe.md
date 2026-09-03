# Unsafe Usage Documentation

Records all `unsafe` usage in the codebase, with rationale. Unsafe is only allowed for low-level operations (per AGENTS.md); the test-only env var mutations exist because `std::env::set_var/remove_var` are `unsafe` in edition 2024.

## 1. Test Environment Variable Mutation

**Files:**
- `crates/cce_core/src/config/loader.rs:288`
- `crates/cce_core/src/config/env_loader.rs:239-244` (doctest) and `crates/cce_core/src/config/env_loader.rs:390-540`

`std::env::set_var` / `std::env::remove_var` are `unsafe` under edition 2024. Used only inside `#[cfg(test)]` and a doctest to control environment variables affecting config loading. Tests run single-threaded per process, so no data-race risk.

**Status:** Necessary — edition 2024 API requirement in tests.

## 2. Native Plugin FFI (libloading)

**Files:**
- `crates/app/cce_plugin_runtime/src/native.rs`
- `crates/app/plugin-sdk/src/lib.rs`

Dynamically loading CCE native plugins requires calling `extern "C"` symbols (`cce_plugin_generate_bm25`, `cce_plugin_generate_bm25_batch`, lifecycle functions), converting raw pointers via `CStr::from_ptr`, freeing returned C strings, and `unsafe impl Send/Sync` for `PluginContext` / `NativePlugin` / `SendPtr` (raw pointer wrappers guaranteed thread-safe by the FFI contract).

The generate functions receive the opaque context pointer from `cce_plugin_create` as their first argument, making plugin instance state usable across calls. The batch entry points (`cce_plugin_generate_*_batch`) take a JSON array of entity groups. The pipeline-extension capability entry points (`cce_plugin_parse_document`, `cce_plugin_extract_entities`, `cce_plugin_post_group`, `cce_plugin_chunk`, `cce_plugin_rerank`, `cce_plugin_query_scheme`, `cce_plugin_tree_sitter_language`, and the phase-2 `group` / `extract_symbols` / `extract_relations` / `rewrite_query` / `fusion_weights` / `filter_results` / `filter_file`, plus the phase-3 `extract_imports` / `extract_exports`, the `LanguageRemap` symbols `cce_plugin_has_language_remap` / `cce_plugin_remap_grammar_language`, and the `LangHeuristics` symbols `cce_plugin_has_stdlib_heuristic` / `cce_plugin_classify_stdlib` / `cce_plugin_has_test_file_heuristic` / `cce_plugin_is_test_file` / `cce_plugin_has_entity_kind_heuristic` / `cce_plugin_entity_kind`) and their `cce_plugin_has_*` guards (including `cce_plugin_has_symbol_extract`) are part of the same ABI (version history reset to 1; see `native.rs`). The context pointer is copied into a `SendPtr` wrapper to move it into `'static` worker-thread closures.

**Custom-language tree-sitter pointer:** `cce_plugin_tree_sitter_language` returns a raw `*const TSLanguage` pointer (as `*const c_void`) that is stored in the process-global plugin-language tables:
- `crates/cce_parser/src/tree_sitter_init.rs` — `PluginTsLanguage.language_ptr` with `unsafe impl Send/Sync`, and `language_from_raw_ptr` which `std::mem::transmute`s the raw pointer into a `tree_sitter::Language` (`#[repr(transparent)]` over the same pointer type). The pointer is owned by the plugin library, which the host keeps loaded for the process lifetime; it is never freed or dereferenced by the host. It is only ever re-imported into tree-sitter.
- `crates/cce_core/src/types/language.rs` — `Language::Custom(u32)` index into the plugin-language table (`OnceLock<Mutex<Vec<PluginLanguageRecord>>>`).
- `crates/cce_parser/src/tree_sitter_init.rs` — `register_plugin_language_with_builtin_grammar` (test / stand-in helper) `std::mem::transmute`s a built-in `tree_sitter::Language` into a `*const c_void` pointer to seed a custom language's grammar slot. Same invariant: the pointer is valid for the process lifetime (built-in grammars are static) and only re-imported via `language_from_raw_ptr`, never dereferenced by Rust.
- `crates/cce_parser/src/tree_sitter_init.rs` — `plugin_grammar_abi_version` re-imports a plugin grammar pointer via `language_from_raw_ptr` and reads `abi_version()` (tree-sitter ABI metadata, immutable; the same field the runtime `Parser::set_language` checks). Used by the registration-time ABI pre-check (`register_ast_language_plugins`, `plugins.grammar_abi_policy` deny/warn). Same pointer-validity invariant as above; only metadata is read, never the AST.

This mirrors the existing `SendPtr` opaque-token pattern.

**Timeout semantics:** native FFI calls run on a dedicated thread with a 5s budget (`execute_with_timeout_blocking`). The thread cannot be forcefully terminated; on timeout it lingers until the FFI call returns naturally, while the caller proceeds. Thread-safety of concurrent calls on the shared context is a documented ABI contract — plugins MUST be thread-safe. The plugin reranker (`crates/cce_orchestrator/src/query/ranking/plugin_reranker.rs`) runs sync plugin calls on `tokio::task::spawn_blocking`; the plugin adapters impose their own hard timeout.

**Panic containment:** the SDK's `declare_plugin!` macro wraps every exported function body in `std::panic::catch_unwind` so a plugin panic is reported as an FFI error result (`{"result":"error","error_type":"execution_failed"}`) instead of unwinding across the C ABI, which would be undefined behaviour.

**Status:** Necessary — direct FFI to dynamically loaded libraries (low-level operation).

## 3. Vendored Third-Party Bindings

**File:** `crates/tree-sitter-svelte/bindings/rust/lib.rs`

Auto-generated Rust bindings for the vendored tree-sitter grammar. Follows the standard tree-sitter binding pattern.

**Status:** External — vendored third-party code, not maintained in this repo.
