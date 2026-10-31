//! BM25 cache-hit and concurrent-read benchmark (§5.7, §5.8).
//!
//! Quantifies repeat-query stability (same query back to back), pagination
//! walk jitter (growing offsets at fixed limit) and concurrent-read
//! throughput (1/2/4/8 reader threads over one shared manager). Local
//! Tantivy index only, 500 docs; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run -p cce-storage-bm25 --bench bm25_cache --features local`
//!
//! Results are printed to stdout and appended to
//! `benches/results/bm25_cache.tsv`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cce_storage_bm25::{Bm25Retrieval, IndexManager, batch_add_documents};
use cce_storage_common::FulltextSearchOptions;

fn make_docs(n: usize) -> Vec<(String, HashMap<String, String>)> {
    (0..n)
        .map(|i| {
            let mut fields = HashMap::new();
            fields.insert(
                "title".to_string(),
                format!("handler process_request queue worker retry session manager {i}"),
            );
            fields.insert(
                "content".to_string(),
                format!(
                    "function process_request_{i} retries the queued session {i} with backoff and timeout handling"
                ),
            );
            fields.insert("keywords".to_string(), format!("process_request session retry {i}"));
            fields.insert("chunk_id".to_string(), format!("chunk_{i}"));
            fields.insert("file_path".to_string(), format!("src/worker_{:02}.rs", i % 10));
            fields.insert("project_id".to_string(), "1".to_string());
            fields.insert("epoch".to_string(), "0".to_string());
            (format!("doc:{i}"), fields)
        })
        .collect()
}

fn opts(limit: usize, offset: usize) -> FulltextSearchOptions {
    FulltextSearchOptions {
        limit,
        offset,
        field_weights: HashMap::from([("title".to_string(), 2.0), ("content".to_string(), 1.0)]),
        project_id: 1,
        epochs: vec![0],
        excluded_files: None,
        exclude_test: false,
        exclude_generated: false,
        exclude_dependency: false,
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        term_operator: Default::default(),
    }
}

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("bm25_cache benchmark (debug, 500 local docs)");
    println!("{:<26} {:>12} {:>12}", "case", "ms", "hits");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/bm25_cache.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\thits");
    }
    let mut row = |label: &str, ms: f64, hits: usize| {
        println!("{label:<26} {ms:>12.2} {hits:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{hits}");
        }
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let manager = Arc::new(IndexManager::create(tmp.path()).expect("create index"));
    let schema = manager.schema().clone();
    batch_add_documents(&manager, &schema, make_docs(500)).expect("index docs");
    manager.reload_reader().expect("reload");
    let retrieval = Arc::new(Bm25Retrieval::new());
    let query = "process_request session retry";

    // Repeat-query stability: identical query and options back to back.
    {
        let o = opts(10, 0);
        let mut hits = 0;
        let ms = bench_ms(10, || {
            let r = retrieval
                .search(&manager, &schema, query, &o)
                .expect("search");
            hits = r.len();
        });
        row("repeat/l10/o0", ms, hits);
    }

    // Pagination walk: fixed limit, growing offsets; per-step cost shows
    // whether heap skip-ahead bends before the retrieval window cap.
    for offset in [0usize, 10, 20, 40, 80] {
        let o = opts(10, offset);
        let mut hits = 0;
        let ms = bench_ms(5, || {
            let r = retrieval
                .search(&manager, &schema, query, &o)
                .expect("search");
            hits = r.len();
        });
        row(&format!("walk/l10/o{offset}"), ms, hits);
    }

    // Concurrent readers: N threads, each running the same query K times;
    // reports wall time and per-query mean so serial bottlenecks surface as
    // flat or falling throughput with rising concurrency.
    for threads in [1usize, 2, 4, 8] {
        let iters = 8usize;
        let start = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..threads {
                let manager = Arc::clone(&manager);
                let retrieval = Arc::clone(&retrieval);
                let schema = schema.clone();
                let o = opts(10, 0);
                scope.spawn(move || {
                    for _ in 0..iters {
                        let r = retrieval
                            .search(&manager, &schema, query, &o)
                            .expect("search");
                        assert!(!r.is_empty(), "fixture query must hit");
                    }
                });
            }
        });
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let total = (threads * iters) as f64;
        row(
            &format!("concurrent/t{threads}"),
            wall_ms / total,
            threads * iters,
        );
    }
}
