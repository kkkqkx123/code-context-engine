# Dynamic Dispatch Usage Documentation

Records all `dyn` usage in the codebase, with rationale and whether replacement is viable.

## 1. External Library Constraints

### Tantivy Query Objects

**File:** `src/storage/bm25/index/search.rs:97`

Tantivy's `Query` trait is designed around trait objects. Different query types (`TermQuery`, `BooleanQuery`, `PhraseQuery`) are composed at runtime. An enum wrapper would add maintenance burden without eliminating boxing internally.

**Status:** Necessary — library API constraint.

### tracing_subscriber Layer

**File:** `src/logger/logger.rs:35`

Format selection (json/compact/pretty) returns different concrete types at runtime. `Box<dyn Layer>` is the standard approach in the tracing ecosystem. An enum wrapper would add boilerplate with no benefit.

**Status:** Necessary — runtime format selection requires dynamic dispatch.

## 2. Type Erasure for Global Storage

### LOG_GUARD — tracing_appender WorkerGuard

**File:** `src/logger/logger.rs:17`

```rust
pub static LOG_GUARD: OnceLock<Box<dyn std::any::Any + Send + Sync>> = OnceLock::new();
```

The guard returned by `tracing_appender::non_blocking()` is an external type (`WorkerGuard`) that must be kept alive in global static storage. Since no methods are called on it — only its lifetime matters — type erasure via `Box<dyn Any>` is the cleanest approach. Generics would overcomplicate a simple lifetime-holding pattern.

**Status:** Necessary — stores an opaque external type globally.

## 3. Runtime Polymorphism

### EventListener — Operation Event Callbacks

**File:** `crates/cce_orchestrator/src/operation/events.rs:256`

```rust
pub type EventListener = Arc<dyn Fn(&OperationEvent) + Send + Sync>;
```

User-registered callbacks capture arbitrary state, producing unique types per subscriber. The event bus cannot know callback types at compile time. Alternatives (enum wrappers, generics) break extensibility or require recompilation. This is the idiomatic Rust pattern for event systems.

**Status:** Necessary — event bus fundamentally requires dynamic dispatch.

### RelationSnapshotPublisher — Runtime Publication Boundary

**File:** `crates/cce_orchestrator/src/index/relation_publisher.rs:22`

`Arc<dyn RelationSnapshotPublisher>` lets the orchestrator construct complete
canonical snapshots without depending on the server crate, while the server
implementation atomically coordinates SQLite epoch activation and the
process-local `RelationRuntime` projection.

**Status:** Necessary — avoids a crate dependency cycle while keeping a single
server-owned publication protocol.

### CodePlugin — Plugin Registry

**Files:**
- `crates/cce_core/src/plugin.rs:216-461` (`Arc<dyn CodePlugin>`)
- `crates/cce_parser/src/parser/coordinator.rs:704` (detectors)
- `crates/cce_parser/src/ast_to_nl/converter/group_converter.rs` (generators)
- `crates/cce_orchestrator/src/query/ranking/plugin_reranker.rs` (plugin reranker)

Plugins are loaded at runtime from Lua scripts or native dynamic libraries and
registered in a global registry. The registry cannot know plugin types at
compile time; `dyn CodePlugin` is the extension boundary of the plugin system.

**Thread-safety note:** the native plugin context pointer flows through the
FFI generate calls, and the batch entry points (`cce_plugin_generate_*_batch`)
let a batch of entity groups cross the FFI boundary once per plugin instead
of once per group. The pipeline-extension capability entry points
(`parse_document`, `extract_entities`, `post_group`, `chunk`, `rerank`,
`query_scheme`, `tree_sitter_language`, and the phase-2 `group` /
`extract_symbols` / `extract_relations` / `rewrite_query` / `fusion_weights` /
`filter_results` / `filter_file`) plus the matching `cce_plugin_has_*` guards
are part of the same ABI (version history reset to 1). The host does NOT
serialize concurrent calls with a mutex — a hung plugin holding a lock would
deadlock every later call and defeat the 5s timeout. Instead, thread-safety of
the shared context is an explicitly documented ABI contract (see
`docs/archive/unsafe.md`), and the Lua path achieves isolation via a per-call
VM pool.

**Capability facets:** the plugin trait is organized around capability
facets (`PluginCapability`: `text_gen`, `format_parse`, `entity_extract`,
`ast_language`, `symbol_extract`, `group`, `group_override`, `chunk`,
`rerank`, `relation_extract`, `query_rewrite`, `fusion`, `result_filter`,
`file_filter`). Each integration point queries
the registry via `PluginRegistry::get_plugins(capability, path, language)`,
which routes by declared/effective `PluginMetadata.capabilities` + runtime
`supports_*` probes + `file_patterns` + `languages` + priority. See
`docs/plan/plugin_extension_design.md` §3/§6,
`docs/plan/plugin_extension_phase2_design.md` §3 and
`docs/plan/plugin_symbol_extraction_extension.md` §4.

**Status:** Necessary — plugin system requires runtime-loaded implementations.

### SymbolExtractor — Per-Language Factory

**File:** `crates/cce_parser/src/parser/extractor/symbol_extractor/traits.rs:112`

```rust
pub fn create_extractor(language: Language) -> Option<Box<dyn SymbolExtractor>>
```

Each language (rust, go, python, ...) provides its own `SymbolExtractor`
implementation selected at runtime by language detection. An enum would require
enumerating every language in the type and is not extensible by plugins.

Custom languages (`Language::Custom(_)`) route through
`create_extractor_with_registry(language, registry, file_path, language_str)`
(traits.rs:181), which wraps a `SymbolExtract`-capable plugin in
`PluginSymbolExtractor` (also `Box<dyn SymbolExtractor>`). This is the same
runtime dispatch mechanism extended to plugin-backed extractors.

The relation builder also has a tree-free plugin path for custom languages:
`relation::helpers::extract_imports_from_plugin` (invoked from
`FileProcessor::index_file_core` when `symbol_extract_enabled` is on) calls
the registry's `SymbolExtract` plugins directly on raw source text, so a
custom language without a registered tree-sitter grammar still contributes
imports to the relation index. It dispatches through
`PluginRegistry::get_plugins` (same `Arc<dyn CodePlugin>` routing as above).

**Status:** Necessary — runtime language dispatch (built-in and plugin-backed).

### ParseStage — Parser Pipeline Stage

**File:** `crates/cce_parser/src/parser/coordinator.rs:867-895`

```rust
stages: Vec<Box<dyn ParseStage>>
```

The parse pipeline composes stages (`AddStage`, `DedupStage`, etc.) in configurable
order via builder methods. Stage order is data-driven, not compile-time fixed.

**Status:** Necessary — configurable stage pipeline.

### LlmClient — removed (native async fn generic bound)

The `cce_core::llm::LlmClient` port was previously used as `Arc<dyn LlmClient>`
in `ModelEnhancedGenerator` (parser) and `IndexOrchestrator` (orchestrator).
Since `HttpLlmClient` is the only production implementation, the trait object
was replaced with a generic parameter `C: LlmClient`: `ModelEnhancedGenerator`
is parameterized over the client and instantiated with the concrete
`HttpLlmClient` at the orchestration boundary, while unit tests inject a stub.
The trait itself is now used only as a deterministic generic bound, and its
`chat` method returns `impl Future + Send` (RPITIT, no `async-trait`), so no
dynamic dispatch remains for LLM chat.

**Status:** Resolved — dynamic dispatch removed; filtered out of this list.

### Embedder — LLM Embedding Port

**Files:**
- `crates/cce_core/src/llm/embedding.rs` (trait definition)
- `crates/cce_orchestrator/src/index/storage_coordinator.rs`,
  `crates/cce_orchestrator/src/query/searcher.rs`,
  `crates/cce_orchestrator/src/query/boost/summary.rs`,
  `crates/cce_orchestrator/src/hot_update/processors/factory.rs`,
  `crates/cce_server/src/engine.rs`, `crates/cce_server/src/api/state.rs`

Embedding is a cross-cutting dependency consumed by the storage coordinator,
the searcher, hot-update processors and summary boosts. Unlike the chat port
(`LlmClient`, a deterministic generic bound), parameterizing every consumer
would ripple a generic type through the whole orchestration layer, so the
`Embedder` port is injected as `Arc<dyn Embedder>`. Its async methods use
`#[async_trait]` (boxed futures) to stay dyn-compatible; the only production
implementation is `OpenAICompatibleProvider`.

**Status:** Necessary — cross-cutting injection boundary; single documented
`dyn` in the LLM service layer.
