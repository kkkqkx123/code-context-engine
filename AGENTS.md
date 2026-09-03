# Develop Guideline

**No-backward-compatible**
At present, the project is in the development stage and there is no need to specifically consider backward compatibility. Prioritize ensuring the long-term maintainability of the architecture and refactoring design defects as early as possible.

**Document Reference Rule**
Prohibit the use of any plan identifiers (e.g., P1, P2-3, §4.1, phase3, G2, etc.) in code comments. Comments shall describe code intent only and shall not reference external temp plans.

**Language**
Always use English in code files(include config files, comments) and use Simplified Chinese in docs.

**Plan/Design Document**
Avoid including complete code snippets. Mainly using concise natural language descriptions.

**Security Assurance**
Always avoid the use of unwrap. In testing, substitute with expect.
Refrain from using unsafe methods except where directly involving low-level operations.
All instances of unsafe usage must be explicitly documented in the unsafe.md file within the docs/archive directory.

**Type Design Guidelines**
Minimise the use of dynamic dispatch forms such as `dyn`, always prioritising deterministic types.
All instances of dynamic dispatch must be explicitly documented in the `dynamic.md` file within the `docs/archive` directory.

**Native Plugin ABI Maintenance**
The CCE native plugin ABI is defined in `crates/app/cce-plugin-sdk/include/cce_plugin.h` and must stay in sync across three places: the header, `crates/app/cce-plugin-sdk/src/lib.rs` (the `declare_plugin!` macro and FFI helpers), and `crates/app/cce-plugin-runtime/src/native.rs` (the host loader). When adding or changing ABI symbols, update all three, and reflect the change in the symbol/method tables of `crates/app/cce-plugin-sdk/docs/guide.md` in the same change.

## Project Overview

The project is a server + CLI application developed in `Rust` for codebase indexing. It splits code files into entities (using tree-sitter as AST parser), groups related entities, converts them into natural language, then hands the result to the embedder (LLM service) to produce vectors. Vectors are stored in the Qdrant vector database and ready for vector query.

Beside vector-based query, the project also provides BM25 full-text search based on tantivy, and relationship queries (symbol table, call chains, dependencies) based on an inverted index stored in SQLite.

The project supports a plugin system: Lua scripts (mlua) and native dynamic libraries (libloading, cce-plugin-sdk) can override BM25 text generation and embedding generation for custom languages.

## Workspace Layout

All source crates live under `crates/`. Vendored third-party crates (tantivy, tree-sitter-svelte, tree-sitter-vue) are also present but are not described here.

### core — Domain types, configuration, and shared abstractions
Crates: cce-types, cce-config, cce-utils, cce-metrics, cce-text, cce-plugin, cce-llm

### parser — AST parsing, grouping, NL conversion, and relation indexing
Crates: cce-parser, cce-parser-core, cce-relation

### infra — Infrastructure services and storage backends
Crates: cce-circuit-breaker, cce-llm-client, cce-storage-common, cce-storage-bm25, cce-storage-qdrant, cce-storage-sqlite, cce-scanner, cce-metrics-infra

### app — Application layer, orchestration, server, CLI, and plugin runtime
Crates: cce-api, cce-plugin-runtime, cce-orchestrator, cce-server, cce-cli, plugin-sdk

### Git Submodules
- `crates/tantivy` — Vendored tantivy fork (https://github.com/kkkqkx123/tantivy.git)
- `crates/app/cce-e2e-tests` — E2E test suite (https://github.com/kkkqkx123/code-context-engine-e2e.git)

## Command Execution

**Quality Verify**

```shell
# full complie check
cargo clippy --all-targets --all-features

# format
cargo fmt
```

## Testing

The project includes a comprehensive test suite utilising Rust's standard testing framework:

1. **Running tests**:

   ```shell
   cargo test --lib -- --nocapture # Run lib tests
   cargo test <test_name> # Run specific test(s) matching pattern
   cargo test --test <integration_test_file> # Run specific integration test
   ```

2. **Test organization**:
   - Unit tests: Located in the same file as the code being tested, marked with `#[cfg(test)]`
   - Unit tests when original file is too large: Add individual test.rs, and add it to `mod.rs`
   - Integration tests: Located in the `tests/` directory
   - Benchmarks: Located in the `benches/` directory

## Workflow

### Code Files (with AST parser)

```
Scan directory → Check cache (SQLite manifest + content hash) → Detect encoding → Parse file (tree-sitter) → Grouper → AST_to_NL (dual-path: BM25 hybrid + Embedding semantic) → Chunker → Summary generation → Batch embedding → Store in vector-db + SQLite → Update cache
```

**Key implementation details:**
- **Batch processing**: Bounded concurrency via semaphore, configurable batch sizes, immediate storage per batch
- **Checkpoint/resume**: Work-unit checkpointing enables resumable operations via `CheckpointManager`
- **Dual-path chunking**: Independent chunking for BM25 (hybrid enhanced text) and Embedding (pure semantic summary), with entity-level alignment
- **Relation indexing**: Integrated during batch processing; after all batches complete, `build_and_publish_relations()` constructs the full relation graph
- **Plugin integration**: Plugins can override file filtering (`FileFilter`), BM25 text generation, and embedding generation

### Relationship Indexing

```
Parse file → Extract symbols (tree-sitter) → Build symbol table & dependency graph → Resolve relations (local + cross-file) → Publish snapshot → Store in SQLite
```

**Key implementation details:**
- **Two-phase resolution**: Local call resolution within files, then cross-file resolution via project symbol table
- **External dependencies**: Loads symbols from package manager caches (Cargo.toml, package.json, etc.)
- **Governance edges**: Adds synthetic config file nodes with dependency/governance linkage
- **Storage model**: In-memory `DashMap` for fast querying, with snapshot publishing for persistence
- **Hot updates**: Delta computation and partial graph rebuilding for incremental updates

### Other Files (markdown, txt, log, ini, config, json, toml, yaml, xml, etc.)

```
Scan directory → Check cache (SQLite manifest + content hash) → Detect encoding → Parse file (format-specific pipeline) → Grouping → Chunking → Summary generation → Batch embedding → Store in vector-db + SQLite → Update cache
```

**Key implementation details:**
- **Format-specific pipelines**: Each format has dedicated parser, grouper, chunker, and summarizer
  - Markdown: Regex-based heading/paragraph/code block parsing
  - JSON/TOML/YAML: Serde-based parsing
  - XML: quick-xml based parsing
  - Plain text: Specialized chunkers for log, RST, Makefile, INI, CSV, Docker files, etc.
- **Document classification**: Detects document types (documentation, config, code blocks) and applies appropriate processing
- **Plugin integration**: `FormatParse` plugins can override built-in document parsing (three-tier priority: override → built-in → fallback)

