# Performance Overhead Benchmark Design

> Supplementary benchmark suite for the performance overhead analysis (points
> **P-1 … P-9** → benchmarks **T-1 … T-10**).
>
> The retrieval-quality benchmark lives in a separate doc (`design.md`); this
> file is exclusively about *performance overhead* (latency / throughput /
> cost), not ranking quality.

## 1. Goals

- Quantify each overhead point with a standalone, deterministic benchmark.
- Establish an end-to-end stage breakdown (T-1) and attribute the cost to the
  matching component benchmark.
- Give the P-1 … P-9 implementation work a before/after regression target.

## 2. Scope boundary

This repository benchmarks the **wrapper layer** only:

- The `simvec` submodule owns its internal optimizations (WAL `fsync`, HNSW
  build). This repo benchmarks the `LocalVectorStore` wrapper (T-6); simvec
  internals stay in the submodule.
- `tantivy` build/commit behavior is exercised through `cce-storage-bm25`
  (T-5), not the fork internals.

## 3. Conventions (binding)

1. **Statistical framework.** Every new benchmark uses `criterion`
   (`criterion::Criterion::default()…final_summary()`). A single timed pass plus
   a TSV append is emitted for quick reading; the criterion run is the source of
   truth.
2. **Fixed scale tiers** via `cce-bench-kit::SyntheticProject`:
   - `Small` ≈ 500 files, `Medium` ≈ 5 000 files, `Large` ≈ 50 000 files.
   - Entity density ≈ 4 entities/file → `entity_count() = files * 4`.
3. **No external dependencies.** No real LLM/embedding calls. Embedding latency
   is faked with `llm_embedding::mock::MockEmbeddingProvider` +
   `MockEmbeddingStep::Delayed { delay_ms }`; search/relation inputs are
   generated in-memory.
4. **Determinism.** The synthetic generator is a fixed xorshift, so inputs are
   reproducible across runs and machines.
5. **Baseline recording.** Each bench appends `<crate>/benches/results/<bench>.tsv`
   and prints a human summary. No CI gate yet.

## 4. Benchmark inventory

| ID | Crate / bench file | Covers | Measures |
|----|--------------------|--------|----------|
| T-1 | `cce-orchestrator/benches/index_pipeline_stages.rs` | P-1…P-9 (e2e) | Full `IndexOrchestrator::execute` wall time over a materialized `Small` project (scanner → parse → chunk → metadb write). Stage cost is attributed via the component benches below. |
| T-2 | `cce-llm-client/benches/embedding_scheduling.rs` | P-1 | `SuiteEmbeddingTransport::embed_batch` serial vs `buffer_unordered(K)` under a fixed per-call delay. |
| T-3 | `cce-orchestrator/benches/chunker_concurrency.rs` | P-2 | `FileProcessor::process_file_complete` from concurrent clones sharing the global chunker/parser mutex; serial vs bounded vs unbounded wall time. |
| T-4 | `cce-storage-metadb-sqlite/benches/metadb_write_amplification.rs` | P-3 | Row-by-row `*_upsert` vs batch `insert_batch`; per-unit vs merged checkpoint transaction count. |
| T-5 | `cce-storage-bm25/benches/bm25_write.rs` | P-4 | Bulk-write commit-frequency sweep (1/10/50 docs/commit); incremental vs merged delete; query jitter. |
| T-6 | `cce-storage-vector-local/benches/local_vector_store.rs` | P-6 | Sequential vs concurrent `upsert_points` at 1w/10w/50w vectors; per-batch p50/p99; read-while-write latency. |
| T-7 | `cce-codegraph/benches/finalize_relation.rs` | P-5 | Relation finalize: full rebuild (O(N) replay) vs incremental resume `selective_cow_copy` (O(affected)) + memory materialization. |
| T-8 | `cce-codegraph/benches/snapshot_cost.rs` | P-7 | Snapshot publish cost vs change size: `from_index_shared` (O(1)), `from_index`/`detached_clone` (O(N)), `selective_cow_copy` (O(affected)). |
| T-9 | `cce-scanner/benches/scan_parallel.rs` | P-8, P-9 | Single-thread vs bounded-concurrency hashing; per-file filter-decision overhead (empty vs heavy `ScanOptions`). |
| T-10 | `cce-server/benches/service_layer_concurrency.rs` | service layer | `handle_search` fan-out: K concurrent requests against a shared `AppState` over a seeded, indexed project. |

### Overhead point → benchmark map

- **P-1 embedding serial scheduling** → T-2 (+ embedding stage inside T-1)
- **P-2 global chunker `Mutex`** → T-3 (+ chunk stage inside T-1)
- **P-3 SQLite per-row inserts** → T-4 (+ metadb stage inside T-1)
- **P-4 tantivy per-microbatch commit** → T-5
- **P-5 relation reindex full replay** → T-7 (rebuild) + T-8 (publish)
- **P-6 simvec WAL `fsync`** → T-6 (wrapper only; internals in simvec)
- **P-7 codegraph O(N) snapshot** → T-8
- **P-8 serial scan hashing** → T-9
- **P-9 no pipeline overlap** → T-9 (concurrency sweep) + T-1 (e2e)

## 5. Shared support crate

`crates/app/cce-bench-kit` provides the deterministic generator:

- `ProjectSize::{Small, Medium, Large}` with `file_count()` / `entity_count()`.
- `SyntheticProject::generate(size, dist)` / `materialize(root)` — `dist` is a
  `'static` weighted language table (`DEFAULT_LANG_DIST`).
- `gen_content(file_index, language)` emits tree-sitter-parseable stubs so the
  real parse + chunk path is exercised.

## 6. Implementation order

1. Land **T-1** first to get the real end-to-end share.
2. Use the share to prioritize **P-1** (T-2) and **P-2** (T-3).
3. Proceed **P-3/P-4** with **T-4/T-5**, then **P-5/P-7** with **T-7/T-8**,
   **P-6** with **T-6**, and **P-8/P-9** with **T-9**.
4. **T-10** validates the service layer independently of indexing cost.

## 7. How to run

```bash
cargo run -p cce-orchestrator --release --bench index_pipeline_stages
cargo run -p cce-llm-client   --release --bench embedding_scheduling
cargo run -p cce-orchestrator --release --bench chunker_concurrency
cargo run -p cce-storage-metadb-sqlite --release --bench metadb_write_amplification
cargo run -p cce-storage-bm25 --release --bench bm25_write
cargo run -p cce-storage-vector-local --release --bench local_vector_store
cargo run -p cce-codegraph    --release --bench finalize_relation
cargo run -p cce-codegraph    --release --bench snapshot_cost
cargo run -p cce-scanner      --release --bench scan_parallel
cargo run -p cce-server       --release --bench service_layer_concurrency

# Or build all benches at once:
cargo build --benches
```

## 8. Recording baselines

Each bench appends `<crate>/benches/results/<bench>.tsv`. Copy the first clean
run of each into this directory (or a consolidated `baselines.tsv`) once the
environment is stable. Until a CI gate exists, treat these as advisory
regression references, not hard thresholds.
