# Configuration Architecture

## Overview

The configuration system is organized into a global settings layer and modular feature-specific configs. All configs support serde serialization, validation, and environment variable overrides.

## Config Layers

### Global Layer (`cce-config/src/global/`)

| Config | Scope | Description |
|--------|-------|-------------|
| `AppConfig` | Application | Server, database, logging settings |
| `LoggingConfig` | Application | Log level, format, output |
| `DatabaseConfig` | Application | SQLite connection settings |
| `ServerConfig` | Application | HTTP server bind address, port |

### Module Layer (`cce-config/src/modules/`)

| Config | Pipeline Stage | Description |
|--------|---------------|-------------|
| `ScannerConfig` | Scan | File discovery patterns, size limits, gitignore |
| `BatchConfig` | Orchestration | Concurrency, batch sizes, embedding delays |
| `IndexerConfig` | Orchestration | Extensions, exclude dirs, feature toggles |
| `HotUpdateConfig` | Orchestration | Debounce, file watch, incremental behavior |
| `CacheConfig` | Orchestration | Chunk cache size, enable/disable |
| `NestProcessorConfig` | Group | Class-method association, call merging, test grouping |
| `AstToNlConfig` | AstToNl | Output mode, BM25/Embedding/Chunking sub-configs |
| `ChunkingConfig` | Chunk | Token/word limits, overlap, split strategies |
| `Bm25GeneratorConfig` | AstToNl | Max keywords for BM25 text |
| `EmbeddingGeneratorConfig` | AstToNl | Max summary words, docstring inclusion |
| `SummaryConfig` | Summary | Summary generation strategy |
| `RelationConfig` | Relation | Relation indexing behavior |
| `EmbedderConfig` | Storage | Embedder API settings |
| `QdrantConfig` | Storage | Vector DB connection |
| `Bm25Config` | Storage | Tantivy BM25 index settings |
| `SymbolResolutionConfig` | Relation | Cross-file symbol resolution |
| `LicenseHeaderConfig` | Parse | License header filtering rules |
| `RerankConfig` | Search | Reranking behavior |
| `PreprocessorConfig` | Parse | Pre-processing options |

## Config Flow

```
CLI/API params
      │
      ▼
ConfigLoader ──► merge ──► Settings
      │                       │
      ▼                       ▼
ProjectConfig           AppConfig
      │                       │
      ▼                       ▼
Module configs          Global configs
```

## Validation

All configs implement `Validate` trait with `validate_structured()` method. Validation errors are collected and reported as `ConfigValidationError`.

## Environment Variables

Configs support env var overrides via `env_loader.rs`. Pattern: `CCE_{MODULE}_{FIELD}` (e.g., `CCE_SCANNER_MAX_FILE_SIZE`).

## Presets

Some configs provide preset constructors:
- `BatchConfig::small_project()` / `large_project()` / `low_memory()`
- `NestProcessorConfig::small_codebase()` / `large_codebase()` / `disabled()` / `basic()` / `pattern_optimized()` / `test_optimized()`
