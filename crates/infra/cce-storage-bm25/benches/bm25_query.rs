//! BM25 query construction / execution benchmark.
//!
//! Covers the BM25 read-path benchmark needs: query expansion, pagination and
//! materialization cost. Local Tantivy index only, small doc set (200 docs);
//! comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run --bench bm25_query`
//!
//! Results are printed to stdout and appended to
//! `benches/results/bm25_query.tsv`.

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
    println!("bm25_query benchmark (debug, 200 local docs)");
    println!("{:<22} {:>12} {:>12}", "case", "ms", "hits");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/bm25_query.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\thits");
    }
    let mut row = |label: &str, ms: f64, hits: usize| {
        println!("{label:<22} {ms:>12.2} {hits:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{hits}");
        }
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let manager = IndexManager::create(tmp.path()).expect("create index");
    let schema = manager.schema().clone();
    batch_add_documents(&manager, &schema, make_docs(200)).expect("index docs");
    manager.reload_reader().expect("reload");
    let retrieval = Bm25Retrieval::new();

    let short = "calculate_total";
    let long = "calculate_total discounted amount user session retry tax manager";
    let phrase = "\"calculate_total amount\" session";

    let cases: Vec<(&str, String, Bm25SearchOptions)> = vec![
        ("short/l10/o0", short.to_string(), opts(10, 0)),
        ("long/l10/o0", long.to_string(), opts(10, 0)),
        ("phrase/l10/o0", phrase.to_string(), opts(10, 0)),
        ("short/l50/o0", short.to_string(), opts(50, 0)),
        ("short/l10/o80", short.to_string(), opts(10, 80)),
    ];
    for (label, q, o) in &cases {
        let mut hits = 0;
        let ms = bench_ms(5, || {
            let r = retrieval.search(&manager, &schema, q, o).expect("search");
            hits = r.len();
        });
        row(label, ms, hits);
    }
}
