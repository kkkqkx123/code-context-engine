# Examples Guide

This document describes the responsibilities and organization of the `examples/` directory in the cce-e2e-tests package.

## Overview

Examples are **demonstrative output generators** — they produce review artifacts under `outputs/` for manual inspection. They do NOT participate in assertions and are strictly separated from `tests/`.

**Total: 84 example files** across 18 subdirectories plus root-level debug tools.

## Design Principles

1. **No assertions** — Examples generate output files only; correctness is verified by regression tests
2. **Separation of concerns** — Examples are independent from test logic
3. **Reproducible** — Each example uses fixtures from the single shared `fixtures/` tree
4. **Self-documenting** — Output is written to a predictable directory structure under `outputs/`

## Example Categories

### 1. Language Export (`export_{lang}`)

**Count:** 24 examples

**Purpose:** Generate unified review exports for each supported language via the `ReviewExportJob` pipeline. Produces three types of output:
- **Summary** — Natural-language markdown documents
- **Chunks** — Segmented text in both Embedding and BM25 paths (`emb/`, `bm25/`)
- **Structured** — Symbol tables, relations, and type inference reports

| Example | Fixture(s) | Output Directory |
|---------|-----------|-----------------|
| `rust/export_rs.rs` | once_cell, ripgrep, index_sidecar, re_export, relation_demo, relation_diamond | `outputs/scenarios/rust/` |
| `python/export_py.rs` | flask, index_sidecar, re_export, wildcard | `outputs/scenarios/python/` |
| `typescript/export_ts.rs` | index_sidecar, re_export, wildcard | `outputs/scenarios/typescript/` |
| `javascript/export_js.rs` | express, re_export | `outputs/scenarios/javascript/` |
| `go/export_go.rs` | gin | `outputs/scenarios/go/` |
| `java/export_java.rs` | index_sidecar, spring_boot, jackson_core | `outputs/scenarios/java/` |
| `csharp/export_cs.rs` | MediatR | `outputs/scenarios/csharp/` |
| `kotlin/export_kt.rs` | coroutines | `outputs/scenarios/kotlin/` |
| `scala/export_scala.rs` | case_class | `outputs/scenarios/scala/` |
| `ruby/export_rb.rs` | mixin | `outputs/scenarios/ruby/` |
| `php/export_php.rs` | namespace_trait | `outputs/scenarios/php/` |
| `lua/export_lua.rs` | closure | `outputs/scenarios/lua/` |
| `dart/export_dart.rs` | mixin_async | `outputs/scenarios/dart/` |
| `c/export_c.rs` | macros | `outputs/scenarios/c/` |
| `cpp/export_cpp.rs` | templates | `outputs/scenarios/cpp/` |
| `bash/export_sh.rs` | pipeline | `outputs/scenarios/bash/` |

### 2. Type Inference Visualization (`export_type_inference`)

**Count:** 1 example

**Purpose:** Cross-language type inference visualization. Generates `TYPE_INFERENCE.md` plus per-file reports for all `type_inference/` fixtures across every language.

**Output:** `outputs/scenarios/<lang>/summary/<case>/` + `outputs/scenarios/<lang>/structured/<case>/`

### 3. Basic Indexing (`basic`, `multi_language_basic`)

**Count:** 2 examples

**Purpose:** Full index workflow via `IndexOrchestrator`, producing JSON result snapshots.

**Output:** `outputs/scenarios/rust/presentation/`, `outputs/scenarios/multi_language/`

### 4. Full Retrieval Benchmark (`benchmark_{target}`)

**Count:** 9 examples

**Purpose:** Full retrieval quality evaluation — computes Precision@K, Recall@K, MRR@K, F1@K using pre-computed vectors. Evaluates embedding + BM25 variants across three evaluation scopes (all, no-test, impl-only).

| Example | Fixture | Judgments |
|---------|---------|-----------|
| `rust/benchmark_oncecell.rs` | once_cell | `once_cell_relevance_judgments` |
| `rust/benchmark_ripgrep.rs` | ripgrep | `ripgrep_relevance_judgments` |
| `python/benchmark_flask.rs` | flask | `flask_relevance_judgments` |
| `javascript/benchmark_express.rs` | express | `express_relevance_judgments` |
| `go/benchmark_gin.rs` | gin | `gin_relevance_judgments` |
| `java/benchmark_spring_boot.rs` | spring_boot | `spring_boot_relevance_judgments` |
| `java/benchmark_jackson_core.rs` | jackson_core | `jackson_core_relevance_judgments` |
| `csharp/benchmark_mediatr.rs` | MediatR | `mediatr_relevance_judgments` |
| `cpp/benchmark_media_server.rs` | media_server | `media_server_relevance_judgments` |

**Output:** `outputs/benchmark/{target}/` — aggregate reports, relevance reports, run manifest, evaluation-scope CSV, test diagnostics

### 5. Benchmark Data Generation (`gen_bench_{target}`)

**Count:** 8 examples

**Purpose:** Generate `bench_data.rkyv` (pre-computed vectors + BM25 text + queries) for a target fixture with a distractor. Must be run before the corresponding benchmark.

| Example | Target | Distractor |
|---------|--------|------------|
| `rust/gen_bench_oncecell.rs` | rust_once_cell | rust_distractor |
| `rust/gen_bench_ripgrep.rs` | rust_ripgrep | rust_distractor |
| `python/gen_bench_flask.rs` | python_flask | python_basic |
| `javascript/gen_bench_express.rs` | javascript_express | javascript_basic |
| `go/gen_bench_gin.rs` | go_gin | go_basic |
| `java/gen_bench_spring_boot.rs` | java_spring_boot | java_basic |
| `java/gen_bench_jackson_core.rs` | java_jackson_core | java_basic |
| `csharp/gen_bench_mediatr.rs` | csharp_mediatr | csharp_basic |
| `cpp/gen_bench_media_server.rs` | cpp_media_server | cpp_basic |

**Output:** `data/benchmark/full_pipeline/{target}/bge-m3/bench_data.rkyv`

### 6. Retrieval Method Benchmark (`benchmark_retrieval_{target}`)

**Count:** 9 examples

**Purpose:** Compare retrieval fusion methods (emb / bm25 / minmax-* / rrf-*) using pre-computed bench data.

**Output:** `outputs/benchmark/{target}/retrieval_method/`

### 7. Rerank Generation & Benchmark

**Count:** 2 examples

**Purpose:** 
- `gen_rerank_oncecell` — Calls a real cross-encoder rerank model to produce `rerank_*.rkyv` sidecars
- `benchmark_rerank_oncecell` — Offline rerank benchmark fusing stored rerank scores with recall scores

**Output:** 
- Generation: `data/benchmark/{baseline}/once_cell/bge-m3/rerank_*.rkyv`
- Benchmark: `outputs/benchmark/once_cell/rerank_{variant}/`

### 8. Aggregation Enhance Benchmark (`benchmark_agg_{target}`)

**Count:** 3 examples

**Purpose:** Compare plain base rankings against offline relation/summary boosted rankings.

**Output:** `outputs/benchmark/{target}/aggregation_enhance/`

### 9. Cross-Language Benchmark (`cross_language_oncecell`)

**Count:** 1 example

**Purpose:** Independent cross-language evaluation. Pairs each non-English judgment with the English judgment sharing the largest judged range, then reports `language_gap` on dense retrieval. BM25 rows are diagnostic only; this job does not feed chunking, BM25-parameter or aggregation-default conclusions.

**Output:** `outputs/benchmark/cross_language/{target}/` — `cross_language_metrics.md`, `run_manifest.txt`

### 10. BM25 Parameter Sweep (`bm25_para_{target}`)

**Count:** 3 examples

**Purpose:** Sweep BM25 parameter space (k1, b, etc.) and aggregate results by parameter set. Prints MRR@10, F1@10, Recall@20 for each combination.

**Output:** `outputs/benchmark/{target}/bm25_parameter_sweep/` + console aggregates

### 11. Annotation Review (`annotation_{target}`)

**Count:** 7 examples

**Purpose:** Offline embedding recall replay + `RelationAnnotator` annotation comparison. Renders per-query raw vs annotated markdown pairs. Requires the corresponding `gen_bench_*` to have been run first.

**Output:** `outputs/scenarios/<lang>/annotation/{target}/` — `index.md` + `{query_id}.md`

### 12. Query Review (`query_review_{target}`)

**Count:** 2 examples

**Purpose:** Real retrieval chain review — runs every judgment query through the actual query pipeline (hybrid mode with embedded local vector engine), renders per-query complete results, an aggregated two-sub-query merge demo, and a category-aware aggregation benchmark across query-type pairs, merge strategies and weights.

**Output:** `outputs/scenarios/<lang>/query_review/{target}/` — `run_manifest.txt`, `index.md`, `{query_id}.md`, `aggregated_demo.md`, `aggregation_metrics.md`

### 13. Relation Review (`relation_review_{target}`)

**Count:** 2 examples

**Purpose:** Relation graph visualization — indexes the fixture, resolves seed entities from judgment expectations plus hub names, renders per-seed call relations, call chains, and ego graphs with mermaid diagrams. Fully offline.

**Output:** `outputs/scenarios/<lang>/relation_review/{target}/` — `run_manifest.txt`, `index.md`, `entity/{seed}.md`, `graph/{seed}.json`

### 14. Document Export (`export_doc`, `summary_doc`, `dump_document_chunks`)

**Count:** 3 examples

**Purpose:** Document format pipeline testing — processes JSON, XML, YAML, TOML, and other structured document formats through `FileProcessor` (not tree-sitter).

**Output:** `outputs/scenarios/documents/`, `outputs/demo/documents/`

### 15. Alignment Report (`alignment_report`)

**Count:** 1 example

**Purpose:** Hybrid cross-path alignment review — runs vector, BM25, and hybrid searches for sample queries, computes alignment keys with the production `FusionAlignmentStats`, and reports per-query and overall match, single-path, and unalignable ratios.

**Output:** `outputs/scenarios/rust/alignment/`

### 16. Tool Export (`export_file_fold`)

**Count:** 1 example

**Purpose:** Stateless `FileFoldTool` demonstration — folds source files at three detail levels (detailed/minimal/tight) and writes human-review skeletons with fold statistics.

**Output:** `outputs/tools/file-fold/`

### 17. Debug / Diagnostic Tools

**Count:** 5 examples

**Purpose:** Temporary diagnostics for development. Not standard pipeline outputs.

| Example | Purpose |
|---------|---------|
| `dbg_check.rs` | Type-inference debug — walks files, prints entity metadata and type bindings |
| `dbg_issue_tmp.rs` | Temporary entity/type-binding dump for selected fixtures |
| `dbg_meta.rs` | Python discriminated-union metadata dump |
| `debug_convert.rs` | Chunk merge debug — dumps pre/post-merge chunks for specific groups |
| `debug_matrix.rs` | Side-by-side BM25 vs embedding score comparison matrix |
| `zz_diag_rerank.rs` | Rerank input dump for a single query |

## Output Directory Structure

```
outputs/
├── scenarios/           # Per-language scenario outputs
│   ├── <lang>/
│   │   ├── summary/     # NL markdown exports
│   │   ├── chunks/      # Chunk segmentation (emb/ + bm25/)
│   │   ├── structured/  # Symbol tables, relations, type inference
│   │   ├── annotation/  # Raw vs annotated comparison
│   │   ├── query_review/# Full query result visualization
│   │   ├── relation_review/ # Graph visualization
│   │   ├── presentation/# Index workflow snapshots
│   │   └── alignment/   # Cross-path alignment reports
│   ├── rust/
│   ├── python/
│   └── ... (per-language)
├── benchmark/           # Benchmark results
│   └── {target}/
│       ├── retrieval_method/
│       ├── aggregation_enhance/
│       ├── bm25_parameter_sweep/
│       └── rerank_{variant}/
├── demo/                # Demo outputs (document chunks)
└── debug/               # Debug comparison outputs
```

## Naming Conventions

- **Export examples:** `export_{lang}` (e.g., `export_rs`, `export_py`)
- **Benchmarks:** `benchmark_{target}` (e.g., `benchmark_oncecell`, `benchmark_flask`), `cross_language_{target}`
- **Benchmark data gen:** `gen_bench_{target}`
- **Annotation review:** `annotation_{target}`
- **Query review:** `query_review_{target}`
- **Relation review:** `relation_review_{target}`
- **Debug tools:** `debug_*`, `dbg_*`, `zz_diag_*`

## How to Add a New Example

1. Create a `.rs` file under the appropriate language subdirectory in `examples/`
2. Add a `[[example]]` entry in `Cargo.toml`
3. Use fixtures from the shared `fixtures/` tree via `src/fixture.rs` loaders
4. Write output to `outputs/scenarios/` or `outputs/benchmark/`
5. Examples must NOT contain assertions — use `tests/regression/` for assertion logic
