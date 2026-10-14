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

## 4. SIMD Distance Kernels (simvec)

**Files:**
- `crates/simvec/src/distance/avx2.rs`
- `crates/simvec/src/distance/avx512.rs`
- `crates/simvec/src/distance/neon.rs`
- `crates/simvec/src/distance/kernel.rs`

Hand-written x86-64 (AVX2+FMA, AVX-512F+DQ+VL) and aarch64 (NEON) distance
kernels. Each kernel entry point carries `#[target_feature]` and is only
invoked after its runtime probe (`is_x86_feature_detected!` /
`is_aarch64_feature_detected!`) succeeds; `kernel::selected()` resolves the
preferred available kernel once per process and every call site dispatches
through it, so an unsupported instruction stream is unreachable. Slice
arguments must have equal length; the kernels use unaligned loads
(`_mm512_loadu_ps`, `vld1q_f32`), pointer-offset prefetch hints, and (on
aarch64) one inline-asm prefetch (`prfm pldl1keep`, a pure cache hint safe
on all ARMv8 CPUs). On non-aarch64 hosts the `neon` module compiles to a
thin safe wrapper over the scalar baseline. Differential tests pin each
kernel against the naive baseline within `1e-4`.

**Status:** Necessary — direct SIMD intrinsics and inline asm for vector
distance evaluation (low-level operation).

## 5. Memory-Mapped Files and Byte Reinterpretation (simvec)

**Files:**
- `crates/simvec/src/storage/directory.rs` (`keys.bin` / `payloads.bin` views)
- `crates/simvec/src/storage/vectors.rs` (dense `vectors.bin` segments)
- `crates/simvec/src/storage/quant.rs` (quantized `quant.bin` segments)
- `crates/simvec/src/storage/payload_store/pages.rs` (payload data pages)
- `crates/simvec/src/storage/compaction.rs` (volume space probe)
- `crates/simvec/src/index/hnsw.rs`, `crates/simvec/src/index.rs` (test-only mmap helpers)

`memmap2::Mmap::map` / `MmapOptions` are unsafe by contract: the caller
guarantees the file is not truncated by another process while mapped. Every
collection file is sized up front (`set_len` on create/grow, `replace_from`
swaps in a complete temp file via rename) and never shortened in place, so the
mapped range stays backed for the lifetime of the mapping. Readers hold the
mapping behind an `ArcSwap` published only after the file is complete.

`vectors.rs` reinterprets an `&[f32]` slice as `&[u8]` (and back) for the
positional `pwrite` of one dense column. `f32` has a fully defined bit pattern
(all bit values are valid), alignment is unchanged, and the length is derived
from the same slice, so the cast cannot produce out-of-bounds access or
misaligned reads.

`compaction.rs` calls `libc::statvfs` on the collection directory to learn how
many bytes the volume still has free before compaction writes its temp files;
`statvfs` is zero-initialised before the call and its result is only read when
the call returns 0. A failure is treated as "unknown", which lets compaction
proceed and surface the real I/O error instead of refusing on a probe failure.

**Status:** Necessary — mmap and raw byte views are the storage engine's
I/O substrate, and `statvfs` is the only portable free-space query.
