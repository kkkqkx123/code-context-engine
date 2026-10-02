//! BM25 scale benchmark: document count, limit, offset, concurrency.
//!
//! Extends the small `bm25_query` bench along the dimensions the earlier
//! analysis left open: document count and limit growth, large offsets up
//! to the retrieval window truncation, per-hit document materialization
//! cost (the linear per-hit term in this codebase, where highlighting
//! lives outside the retrieval call), and concurrent-reader throughput.
//! Local Tantivy index only; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run -p cce-storage-bm25 --bench bm25_scale`
//!
//! Results are printed to stdout and appended to
//! `benches/results/bm25_scale.tsv`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_storage_bm25::{Bm25Retrieval, Bm25SearchOptions, IndexManager, batch_add_documents};

fn make_docs(n: usize) -> Vec<(String, HashMap<String, String>)> {
    (0..n)
        .map(|i| {
            let mut fields = HashMap::new();
            fields.insert(
                "title".to_string(),
                format!("calculator calculate_total amount discount session manager {i}"),
            );
            fields.insert(
                "content".to_string(),
                format!(
                    "function calculate_total_{i} computes the discounted amount for user session {i} with tax and retry logic"
                ),
            );
            fields.insert(
                "keywords".to_string(),
                format!("calculate_total calculator session {i}"),
            );
            fields.insert("chunk_id".to_string(), format!("chunk_{i}"));
            fields.insert(
                "file_path".to_string(),
                format!("src/calc_{:02}.rs", i % 10),
            );
            fields.insert("project_id".to_string(), "1".to_string());
            fields.insert("epoch".to_string(), "0".to_string());
            (format!("doc:{i}"), fields)
        })
        .collect()
}

fn opts(limit: usize, offset: usize) -> Bm25SearchOptions {
    Bm25SearchOptions {
        limit,
        offset,
        field_weights: HashMap::from([("title".to_string(), 2.0), ("content".to_string(), 1.0)]),
        project_id: 1,
        epochs: vec![0],
        excluded_files: None,
        exclude_test: false,
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
    println!("bm25_scale benchmark (debug, local index)");
    println!("{:<26} {:>12} {:>12}", "case", "ms", "hits");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/bm25_scale.tsv"))
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
    let manager = IndexManager::create(tmp.path()).expect("create index");
    let schema = manager.schema().clone();
    batch_add_documents(&manager, &schema, make_docs(2000)).expect("index docs");
    manager.reload_reader().expect("reload");
    let retrieval = Bm25Retrieval::new();
    let short = "calculate_total";

    // Limit curve at fixed offset: per-hit materialization growth.
    for limit in [10usize, 50, 100] {
        let o = opts(limit, 0);
        let mut hits = 0;
        let ms = bench_ms(5, || {
            let r = retrieval
                .search(&manager, &schema, short, &o)
                .expect("search");
            hits = r.len();
        });
        row(&format!("docs2000 limit{limit} off0"), ms, hits);
    }

    // Offset curve at fixed limit, up to the retrieval window edge.
    for offset in [0usize, 80, 150, 190] {
        let o = opts(10, offset);
        let mut hits = 0;
        let ms = bench_ms(5, || {
            let r = retrieval
                .search(&manager, &schema, short, &o)
                .expect("search");
            hits = r.len();
        });
        row(&format!("docs2000 limit10 off{offset}"), ms, hits);
    }

    // Concurrent readers: shared manager across scoped threads. The read
    // path clones the reader per call, so throughput should scale until
    // the collector heap, not a lock, becomes the bound.
    for threads in [1usize, 4, 8] {
        let o = opts(10, 0);
        let start = Instant::now();
        let iters = 20usize;
        for _ in 0..iters {
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| {
                        let r = retrieval
                            .search(&manager, &schema, short, &o)
                            .expect("search");
                        assert!(!r.is_empty());
                    });
                }
            });
        }
        let total_ms = start.elapsed().as_secs_f64() * 1000.0;
        let per_query = total_ms / (iters * threads) as f64;
        println!(
            "{:<26} {:>12.2} {:>12}",
            format!("concurrent x{threads}"),
            per_query,
            "per-query"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "concurrent x{threads}\t{per_query:.2}\t0");
        }
    }
}
