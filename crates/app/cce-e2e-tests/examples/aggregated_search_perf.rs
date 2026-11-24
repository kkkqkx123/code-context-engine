//! Performance sweep for the production aggregated search path.
//!
//! Indexes a small Rust fixture once, then measures wall-clock latency for a
//! matrix of sub-query counts and request concurrency. Cold requests use
//! unique query text; warm requests repeat the same text so the result cache
//! participates. Output is appended to
//! `outputs/benchmark/aggregated_search_perf.tsv`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cce_e2e_tests::fixture::{FixtureSpec, TestFixture};
use cce_e2e_tests::{EmbeddingConfig, QueryWorkflowTest};
use cce_orchestrator::query::types::{AggregatedQueryOptions, SubQuery};
use cce_orchestrator::{QueryError, SearchConfig, SearchSources};
use tokio::task::JoinSet;

const COLUMNS: &[&str] = &[
    "sub_queries",
    "concurrency",
    "phase",
    "p50_ms",
    "p95_ms",
    "total_ms",
    "req_per_sec",
    "executed_sum",
    "cache_hits_sum",
    "from_cache_count",
];

fn options(project_id: i64, texts: Vec<String>) -> AggregatedQueryOptions {
    AggregatedQueryOptions {
        original_query: texts.join(" + "),
        project_id,
        sub_queries: texts
            .into_iter()
            .map(|text| SubQuery {
                text,
                sources: SearchSources::none().with_bm25(),
                weight: 1.0,
            })
            .collect(),
        global_config: SearchConfig::default(),
        ..Default::default()
    }
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TestFixture::load(FixtureSpec::rust_basic())?;
    let mut workflow = QueryWorkflowTest::new(fixture, EmbeddingConfig::mock())
        .with_sources(SearchSources::none().with_bm25());
    workflow.index().await?;
    workflow.ensure_coordinator().await?;
    let coordinator = Arc::new(workflow.coordinator().clone());
    let project_id = 1;
    let base = [
        "main".to_string(),
        "helper".to_string(),
        "calculate".to_string(),
        "function".to_string(),
        "parse".to_string(),
        "config".to_string(),
        "error".to_string(),
        "index".to_string(),
    ];

    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("outputs/benchmark");
    std::fs::create_dir_all(&output_dir)?;
    let path = output_dir.join("aggregated_search_perf.tsv");
    let mut report = cce_bench_kit::TsvReport::open_at(&path, COLUMNS);

    for sub_count in [1usize, 2, 4, 8] {
        for concurrency in [1usize, 4, 16] {
            for phase in ["cold", "warm"] {
                let warm_texts = (0..sub_count)
                    .map(|sub_index| format!("{} warm-{sub_count}", base[sub_index]))
                    .collect::<Vec<_>>();
                if phase == "warm" {
                    coordinator
                        .search_aggregated(&options(project_id, warm_texts.clone()))
                        .await?;
                }

                let started = Instant::now();
                let mut tasks = JoinSet::new();
                for request_index in 0..concurrency {
                    let coordinator = Arc::clone(&coordinator);
                    let texts = if phase == "warm" {
                        warm_texts.clone()
                    } else {
                        (0..sub_count)
                            .map(|sub_index| {
                                format!(
                                    "{} cold-{sub_count}-{concurrency}-{request_index}",
                                    base[sub_index]
                                )
                            })
                            .collect()
                    };
                    let options = options(project_id, texts);
                    tasks.spawn(async move {
                        let call_start = Instant::now();
                        let result = coordinator.search_aggregated(&options).await?;
                        Ok::<_, QueryError>((
                            call_start.elapsed().as_millis() as u64,
                            result.executed_sub_queries,
                            result.sub_query_cache_hits,
                            result.from_cache,
                        ))
                    });
                }
                let mut latencies = Vec::with_capacity(concurrency);
                let mut executed_sum = 0;
                let mut cache_hits_sum = 0;
                let mut from_cache_count = 0;
                while let Some(join) = tasks.join_next().await {
                    let (ms, executed, hits, from_cache) = join??;
                    latencies.push(ms);
                    executed_sum += executed;
                    cache_hits_sum += hits;
                    from_cache_count += usize::from(from_cache);
                }
                latencies.sort_unstable();
                let total_ms = started.elapsed().as_millis() as u64;
                let rate = concurrency as f64 / (total_ms.max(1) as f64 / 1000.0);
                let p50 = percentile(&latencies, 0.50);
                let p95 = percentile(&latencies, 0.95);
                println!(
                    "sub_queries={sub_count:<2} concurrency={concurrency:<2} {phase:<4} p50={p50:>5}ms p95={p95:>5}ms rate={rate:>7.1}/s"
                );
                report.write_row(&[
                    sub_count.to_string(),
                    concurrency.to_string(),
                    phase.to_string(),
                    p50.to_string(),
                    p95.to_string(),
                    total_ms.to_string(),
                    format!("{rate:.1}"),
                    executed_sum.to_string(),
                    cache_hits_sum.to_string(),
                    from_cache_count.to_string(),
                ])?;
            }
        }
    }

    workflow.cleanup().await;
    println!("Report written to {}", path.display());
    Ok(())
}
