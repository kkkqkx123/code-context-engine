//! Fusion candidate truncation and query-cache benchmark.
//!
//! Covers two tail benchmark needs with the exact production code paths:
//! fusion sorting plus candidate truncation cost under different
//! vector/BM25 alignment overlaps (`fuse_hybrid_results`), and query-cache
//! hit/miss/eviction cost plus repeat-query stability (`QueryCache`).
//! All inputs are synthetic; no retrieval services are involved.
//!
//! Run with: `cargo run -p cce-orchestrator --bench fusion_cache`
//!
//! Results are printed to stdout and appended to
//! `benches/results/fusion_cache.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_orchestrator::query::cache::{CacheConfig, QueryCache};
use cce_orchestrator::query::filter::QueryFilter;
use cce_orchestrator::query::retrieval::post_processing::fusion::{
    compute_alignment_coverage, expand_multi_entity_results, fuse_hybrid_results,
    HybridFusionConfig,
};
use cce_orchestrator::query::types::{QueryConfigBuilder, QueryResult, SearchResult, SearchSources};
use cce_types::EntityId;

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn make_result(i: usize, score: f32, multi: bool) -> SearchResult {
    let entity_ids = if multi {
        vec![EntityId(i as u64), EntityId((i + 10_000) as u64)]
    } else {
        vec![EntityId(i as u64)]
    };
    SearchResult {
        id: format!("chunk_{i:05}"),
        entity_ids,
        kind: "function".to_string(),
        name: format!("func_{i}"),
        file_path: format!("src/mod_{:02}.rs", i % 20),
        score,
        original_score: score,
        vector_score: score,
        ..Default::default()
    }
}

/// Vector ids 0..n; BM25 ids shifted by `shift` to control overlap:
/// shift 0 = full overlap, n/2 = half, n = none.
fn make_pair(n: usize, shift: usize, multi: bool) -> (Vec<SearchResult>, Vec<SearchResult>) {
    let vector: Vec<SearchResult> = (0..n)
        .map(|i| make_result(i, 1.0 - i as f32 / (2.0 * n as f32), multi))
        .collect();
    let bm25: Vec<SearchResult> = (0..n)
        .map(|i| make_result(i + shift, 0.9 - i as f32 / (2.0 * n as f32), multi))
        .collect();
    (vector, bm25)
}

fn main() {
    println!("fusion_cache benchmark (debug, synthetic recall sets)");
    println!("{:<30} {:>12} {:>12}", "case", "ms", "extra");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/fusion_cache.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\textra");
    }
    let mut row = |label: &str, ms: f64, extra: &str| {
        println!("{label:<30} {ms:>12.2} {extra:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{extra}");
        }
    };

    let config = HybridFusionConfig::default();

    // Alignment sweep at fixed candidate size.
    for (label, shift) in [("overlap full", 0), ("overlap half", 100), ("overlap none", 200)] {
        let (vector, bm25) = make_pair(200, shift, false);
        let stats = compute_alignment_coverage(&vector, &bm25);
        let ms = bench_ms(10, || {
            let _ = fuse_hybrid_results(vector.clone(), bm25.clone(), &config);
        });
        row(
            &format!("fuse n200 {label}"),
            ms,
            &format!("matched={}", stats.matched_keys),
        );
    }

    // Candidate truncation curve: fuse cost versus input size, full overlap.
    for n in [100usize, 200, 500] {
        let (vector, bm25) = make_pair(n, 0, false);
        let ms = bench_ms(10, || {
            let _ = fuse_hybrid_results(vector.clone(), bm25.clone(), &config);
        });
        row(&format!("fuse overlap-full n{n}"), ms, "out>=1");
    }

    // Multi-entity expansion cost: one-result-per-chunk versus two entities
    // per chunk (expansion doubles the fusion input).
    let (vector, bm25) = make_pair(200, 0, true);
    let expand_ms = bench_ms(10, || {
        let _ = expand_multi_entity_results(vector.clone());
    });
    row("expand multi-entity n200", expand_ms, "2x rows");
    let fuse_ms = bench_ms(5, || {
        let _ = fuse_hybrid_results(vector.clone(), bm25.clone(), &config);
    });
    row("fuse multi-entity n200", fuse_ms, "expanded");

    // Empty-path behavior: one side empty must not pay double normalization.
    let (vector, _) = make_pair(200, 0, false);
    let empty_ms = bench_ms(20, || {
        let _ = fuse_hybrid_results(vector.clone(), Vec::new(), &config);
    });
    row("fuse vector-only n200", empty_ms, "one path");

    // Query cache: put N distinct queries, then hit/miss/repeat/invalidate.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let view = QueryFilter::new(0).expect("filter");
    let result = QueryResult {
        items: (0..10).map(|i| make_result(i, 0.9, false)).collect(),
        total: 10,
        elapsed_ms: 3,
        sources: vec!["vector".to_string(), "bm25".to_string()],
        sub_queries_count: 1,
        failed_sub_queries: Vec::new(),
    };
    rt.block_on(async {
        let cache = QueryCache::new(CacheConfig::default());
        let start = Instant::now();
        for i in 0..200 {
            let options = QueryConfigBuilder::new(1)
                .build(format!("benchmark query {i}"))
                .with_sources(SearchSources::default())
                .with_limit(10);
            cache.put_result_for_view(&options, &view, result.clone()).await;
        }
        row("cache put x200", start.elapsed().as_secs_f64() * 1000.0 / 200.0, "per put");

        let start = Instant::now();
        let mut hits = 0;
        for i in 0..200 {
            let options = QueryConfigBuilder::new(1)
                .build(format!("benchmark query {i}"))
                .with_sources(SearchSources::default())
                .with_limit(10);
            if cache.get_result_for_view(&options, &view).await.is_some() {
                hits += 1;
            }
        }
        row(
            "cache get hit x200",
            start.elapsed().as_secs_f64() * 1000.0 / 200.0,
            &format!("hits={hits}"),
        );

        let start = Instant::now();
        let mut hits = 0;
        for i in 200..400 {
            let options = QueryConfigBuilder::new(1)
                .build(format!("benchmark query {i}"))
                .with_sources(SearchSources::default())
                .with_limit(10);
            if cache.get_result_for_view(&options, &view).await.is_some() {
                hits += 1;
            }
        }
        row(
            "cache get miss x200",
            start.elapsed().as_secs_f64() * 1000.0 / 200.0,
            &format!("hits={hits}"),
        );

        let options = QueryConfigBuilder::new(1)
            .build("repeated benchmark query".to_string())
            .with_sources(SearchSources::default())
            .with_limit(10);
        cache
            .put_result_for_view(&options, &view, result.clone())
            .await;
        let start = Instant::now();
        for _ in 0..50 {
            let _ = cache.get_result_for_view(&options, &view).await;
        }
        row(
            "cache repeat hit x50",
            start.elapsed().as_secs_f64() * 1000.0 / 50.0,
            "stable",
        );

        let start = Instant::now();
        cache.invalidate_all().await;
        row("cache invalidate_all", start.elapsed().as_secs_f64() * 1000.0, "one shot");
        let after = cache.get_result_for_view(&options, &view).await;
        row("cache get after inval", 0.0, if after.is_none() { "empty" } else { "leak" });
    });
}
