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

use cce_storage_bm25::{Bm25Retrieval, IndexManager, batch_add_documents};
use cce_storage_common::FulltextSearchOptions;

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

fn opts(limit: usize, offset: usize) -> FulltextSearchOptions {
    FulltextSearchOptions {
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

    let cases: Vec<(&str, String, FulltextSearchOptions)> = vec![
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

    // Large-scale pagination cliff: 1000 docs, wider limit/offset grid.
    // The retrieval window caps limit+offset, so offsets near the cap show
    // whether the heap cost bends before truncation.
    {
        let tmp_big = tempfile::tempdir().expect("tempdir");
        let big_manager = IndexManager::create(tmp_big.path()).expect("create index");
        let big_schema = big_manager.schema().clone();
        batch_add_documents(&big_manager, &big_schema, make_docs(1000)).expect("index docs");
        big_manager.reload_reader().expect("reload");
        for (limit, offset) in [(10usize, 0), (10, 80), (10, 150), (50, 0), (50, 100)] {
            let o = opts(limit, offset);
            let mut hits = 0;
            let ms = bench_ms(5, || {
                let r = retrieval
                    .search(&big_manager, &big_schema, short, &o)
                    .expect("search");
                hits = r.len();
            });
            row(&format!("scale1000/l{limit}/o{offset}"), ms, hits);
        }

        // Highlight-equivalent per-hit cost: search only versus search plus
        // per-hit tokenization of title and content with query-term matching.
        // The tokenizer is constructed once per query (shared automaton
        // equivalent), then applied per hit; hit counts scale the cost.
        {
            use cce_text::MixedTokenizer;
            use std::sync::Arc;
            let tokenizer = MixedTokenizer::new();
            let query_terms: Vec<String> = tokenizer.tokenize(short);
            for limit in [10usize, 50] {
                let o = opts(limit, 0);
                let mut hits = 0;
                let search_ms = bench_ms(5, || {
                    let r = retrieval
                        .search(&big_manager, &big_schema, short, &o)
                        .expect("search");
                    hits = r.len();
                });
                row(&format!("hl off l{limit}"), search_ms, hits);
                let hl_ms = bench_ms(5, || {
                    let results = retrieval
                        .search(&big_manager, &big_schema, short, &o)
                        .expect("search");
                    hits = results.len();
                    let mut matched = 0;
                    for result in &results {
                        let title = result.title().map(String::as_str).unwrap_or("");
                        let title_tokens = tokenizer.tokenize(title);
                        let content_tokens =
                            tokenizer.tokenize(&format!("function {title} computes session"));
                        for token in title_tokens.iter().chain(content_tokens.iter()) {
                            if query_terms.iter().any(|t| t == token) {
                                matched += 1;
                            }
                        }
                    }
                    let _ = matched;
                });
                row(&format!("hl on l{limit}"), hl_ms, hits);
            }

            // Concurrent readers: shared manager across scoped threads.
            // Compares single-reader latency against contended throughput.
            let shared = Arc::new(big_manager);
            let o = opts(10, 0);
            let r = &retrieval;
            let oo = &o;
            for threads in [1usize, 4, 8] {
                let start = Instant::now();
                std::thread::scope(|scope| {
                    for _ in 0..threads {
                        let manager = Arc::clone(&shared);
                        let schema = big_schema.clone();
                        scope.spawn(move || {
                            for _ in 0..10 {
                                let _ = r.search(&manager, &schema, short, oo).expect("search");
                            }
                        });
                    }
                });
                let total_ms = start.elapsed().as_secs_f64() * 1000.0;
                row(
                    &format!("concurrent reads x{threads}"),
                    total_ms / (threads * 10) as f64,
                    threads * 10,
                );
            }
        }
    }
}
