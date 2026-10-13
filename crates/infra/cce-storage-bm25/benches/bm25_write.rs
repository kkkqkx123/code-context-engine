//! T-5 — BM25 write-path benchmark (covers P-4).
//!
//! The production path commits once per microbatch and once per deleted file,
//! so write throughput and query-time jitter are both gated by commit
//! frequency. This bench sweeps the bulk write commit frequency (1 / 10 / 50
//! documents per commit) and the incremental delete strategy (per-file commit
//! vs merged commit), measuring write throughput and post-write query latency.
//!
//! Requires the `local` (tantivy) feature:
//! `cargo bench -p cce-storage-bm25 --features local --bench bm25_write`

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use cce_storage_bm25::{Bm25Retrieval, IndexManager, batch_add_documents};
use cce_storage_common::{FulltextSearchOptions, TermOperator};
use cce_types::FileCategory;
use tempfile::TempDir;

const N_DOCS: usize = 20_000;
const PROJECT_ID: i64 = 1;

fn make_docs(n: usize) -> Vec<(String, HashMap<String, String>)> {
    (0..n)
        .map(|i| {
            let mut fields = HashMap::new();
            fields.insert("title".to_string(), format!("document number {i}"));
            fields.insert(
                "content".to_string(),
                format!("the quick brown fox jumps over the lazy dog in file number {i} with some code tokens fn_{i}"),
            );
            fields.insert("keywords".to_string(), format!("kw{i} kw{}", i % 100));
            fields.insert("chunk_id".to_string(), format!("chunk_{i}"));
            fields.insert("file_path".to_string(), format!("src/mod_{i:06}.rs"));
            fields.insert("project_id".to_string(), PROJECT_ID.to_string());
            fields.insert("epoch".to_string(), "1".to_string());
            (format!("doc_{i}"), fields)
        })
        .collect()
}

fn docs() -> &'static Vec<(String, HashMap<String, String>)> {
    static D: OnceLock<Vec<(String, HashMap<String, String>)>> = OnceLock::new();
    D.get_or_init(|| make_docs(N_DOCS))
}

fn opts() -> FulltextSearchOptions {
    FulltextSearchOptions {
        limit: 10,
        offset: 0,
        field_weights: HashMap::from([
            ("title".to_string(), 2.0),
            ("content".to_string(), 1.0),
        ]),
        project_id: PROJECT_ID,
        epochs: vec![1],
        excluded_files: None,
        exclude_test: false,
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        term_operator: TermOperator::Or,
    }
}

/// Write `docs` committing once per `per_commit` documents. Returns the total
/// wall time, the number of commits issued, and the live manager/schema so the
/// caller can query on the same index.
fn write_with_freq(dir: &TempDir, per_commit: usize) -> (f64, usize, IndexManager, cce_storage_bm25::IndexSchema) {
    let manager = IndexManager::create(dir.path()).expect("create index");
    let schema = manager.schema().clone();
    let docs = docs();
    let t = Instant::now();
    let mut commits = 0;
    for chunk in docs.chunks(per_commit) {
        batch_add_documents(&manager, &schema, chunk.to_vec()).expect("write batch");
        commits += 1;
    }
    manager.reload_reader().expect("reload reader");
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    (ms, commits, manager, schema)
}

fn query_jitter(manager: &IndexManager, schema: &cce_storage_bm25::IndexSchema) -> (f64, f64) {
    let retrieval = Bm25Retrieval::new();
    let o = opts();
    let mut samples = Vec::with_capacity(200);
    for i in 0..200 {
        let q = format!("fn_{}", (i * 37) % N_DOCS);
        let t = Instant::now();
        let _ = retrieval.search(manager, schema, &q, &o);
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = samples[samples.len() / 2];
    let p99 = samples[samples.len() * 99 / 100];
    (p50, p99)
}

fn main() {
    println!("bm25_write benchmark ({N_DOCS} docs, local/tantivy)");
    println!("{:<22} {:>12} {:>10} {:>12}", "scenario", "total_ms", "commits", "docs/s");

    for per_commit in [1usize, 10, 50] {
        let dir = TempDir::new().expect("tempdir");
        let (ms, commits, manager, schema) = write_with_freq(&dir, per_commit);
        let dps = (N_DOCS as f64) / (ms / 1000.0);
        println!(
            "{:<22} {:>12.1} {:>10} {:>12.0}",
            format!("bulk commit x{per_commit}"),
            ms,
            commits,
            dps
        );

        // Query jitter after a fresh index at this commit frequency.
        let (p50, p99) = query_jitter(&manager, &schema);
        println!("    query p50 {p50:7.2} ms  p99 {p99:7.2} ms");
    }

    // Incremental high-frequency delete: per-file commit vs merged commit.
    let dir = TempDir::new().expect("tempdir");
    let per_file_ms;
    {
        let manager = IndexManager::create(dir.path()).expect("create index");
        let schema = manager.schema().clone();
        batch_add_documents(&manager, &schema, docs().clone()).expect("seed");
        manager.reload_reader().expect("reload");
        let t = Instant::now();
        for i in 0..N_DOCS {
            let fp = format!("src/mod_{i:06}.rs");
            cce_storage_bm25::delete_documents_by_file_path_and_project(&manager, &schema, &fp, PROJECT_ID)
                .expect("delete file");
        }
        per_file_ms = t.elapsed().as_secs_f64() * 1000.0;
    }
    println!(
        "{:<22} {:>12.1} {:>10} {:>12.0}",
        "delete per-file",
        per_file_ms,
        N_DOCS,
        (N_DOCS as f64) / (per_file_ms / 1000.0)
    );

    let dir = TempDir::new().expect("tempdir");
    let merged_ms;
    {
        let manager = IndexManager::create(dir.path()).expect("create index");
        let schema = manager.schema().clone();
        batch_add_documents(&manager, &schema, docs().clone()).expect("seed");
        manager.reload_reader().expect("reload");
        let t = Instant::now();
        cce_storage_bm25::delete_documents_by_project(&manager, &schema, PROJECT_ID).expect("delete project");
        merged_ms = t.elapsed().as_secs_f64() * 1000.0;
    }
    println!(
        "{:<22} {:>12.1} {:>10} {:>12.0}",
        "delete merged",
        merged_ms,
        1,
        (N_DOCS as f64) / (merged_ms / 1000.0)
    );

    // Criterion timing for the bulk write at the two extremes.
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(20))
        .warm_up_time(Duration::from_secs(3));
    criterion.bench_function("bulk_commit_x1", |b| {
        b.iter(|| {
            let dir = TempDir::new().expect("tempdir");
            let (ms, _, _, _) = write_with_freq(&dir, 1);
            criterion::black_box(ms);
        })
    });
    criterion.bench_function("bulk_commit_x50", |b| {
        b.iter(|| {
            let dir = TempDir::new().expect("tempdir");
            let (ms, _, _, _) = write_with_freq(&dir, 50);
            criterion::black_box(ms);
        })
    });
    criterion.final_summary();
}
