//! Service-level retrieval matrix benchmark.
//!
//! Extends `service_layer_concurrency` with a materialized project, three
//! source modes, cold/warm phases and process resource counters. The handler
//! is driven through the same Axum extractor as the existing benchmark, so the
//! measured path includes project resolution, query planning, backend fan-out,
//! fusion and response construction.
//!
//! Run: `cargo bench -p cce-server --bench service_retrieval_matrix`
//! Latency trends are appended to `benches/results/service_retrieval_matrix.tsv`;
//! per-stage accounting is appended to `benches/results/service_retrieval_stages.tsv`.

use std::fs;
use std::io::Write as _;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use cce_api::models::{QueryStageTimingsDto, SearchRequest};
use cce_bench_kit::{
    DEFAULT_LANG_DIST, ProjectSize, SyntheticProject, cpu_ms, resource_delta, resource_snapshot,
};
use cce_server::api::handlers::search;
use cce_server::api::response::ApiResult;
use cce_server_shared::context::SharedContext;
use tokio::task::JoinSet;

#[path = "../tests/common.rs"]
mod common;

const WAVES: usize = 4;

struct RequestSample {
    latency_ms: u64,
    ok: bool,
    error: Option<String>,
    stages: QueryStageTimingsDto,
    from_cache: bool,
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

async fn run_wave(
    state: SharedContext,
    project_id: i64,
    query_type: &str,
    concurrency: usize,
    query: String,
) -> Vec<RequestSample> {
    let mut set = JoinSet::new();
    for _ in 0..concurrency {
        let state = state.clone();
        let query_type = query_type.to_string();
        let query = query.clone();
        set.spawn(async move {
            let request = SearchRequest {
                project_id: Some(project_id),
                query,
                query_type,
                limit: 10,
                ..Default::default()
            };
            let started = Instant::now();
            let response = search::handle_search(State(state), Json(request)).await;
            let elapsed = started.elapsed().as_millis() as u64;
            let outcome = match response {
                ApiResult::Success(success) => (true, None, success.stages, success.from_cache),
                ApiResult::Error(error) => (
                    false,
                    Some(format!("{}: {}", error.error.code, error.error.message)),
                    QueryStageTimingsDto::default(),
                    false,
                ),
            };
            RequestSample {
                latency_ms: elapsed,
                ok: outcome.0,
                error: outcome.1,
                stages: outcome.2,
                from_cache: outcome.3,
            }
        });
    }
    let mut samples = Vec::with_capacity(concurrency);
    while let Some(result) = set.join_next().await {
        if let Ok(sample) = result {
            samples.push(sample);
        }
    }
    samples
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");

    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_retrieval_matrix.tsv",
        "# mode\tconcurrency\tphase\trequests\terrors\tfirst_error\tp50_ms\tp95_ms\tp99_ms\treq_per_sec\tcpu_ms\tpeak_rss_kb\trss_delta_kb\tread_bytes_delta\twrite_bytes_delta",
    );
    let mut stage_out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_retrieval_stages.tsv",
        "# mode\tconcurrency\tphase\trequests\terrors\tfirst_error\tcache_hits\tavg_coordinator_ms\tavg_filter_ms\tavg_cache_key_ms\tavg_rewrite_ms\tavg_embedding_ms\tavg_embedding_provider_ms\tavg_embedding_overhead_ms\tavg_vector_ms\tavg_bm25_ms\tavg_fusion_ms\tavg_post_ms",
    );

    rt.block_on(async {
        let env = common::test_env().await;
        let root = tempfile::TempDir::new().expect("fixture root");
        let project = SyntheticProject::generate(ProjectSize::Small, DEFAULT_LANG_DIST);
        project.materialize(root.path()).expect("materialize");
        let project_id =
            common::create_project(&env.state, &root.path().to_string_lossy()).await;
        let indexed = common::index_fixture_project(&env.state, project_id, root.path()).await;
        assert!(
            indexed.errors.is_empty(),
            "retrieval matrix fixture must index cleanly: {:?}",
            indexed.errors
        );
        println!(
            "seeded {} files; running retrieval matrix",
            indexed.files_indexed
        );

        for mode in ["vector", "bm25", "hybrid"] {
            for concurrency in [1usize, 4, 16] {
                for phase in ["cold", "warm"] {
                    let mut latencies = Vec::with_capacity(WAVES * concurrency);
                    let mut errors = 0usize;
                    let mut first_error = String::new();
                    let mut cache_hits = 0usize;
                    let mut stage_sums = QueryStageTimingsDto::default();
                    let before = resource_snapshot();
                    let wave_started = Instant::now();
                    for wave in 0..WAVES {
                        let query = if phase == "warm" {
                            "fn_00001".to_string()
                        } else {
                            format!("fn_{:05} wave-{wave}-{mode}-{concurrency}", wave + 1)
                        };
                        let samples = run_wave(
                            env.state.clone(),
                            project_id,
                            mode,
                            concurrency,
                            query,
                        )
                        .await;
                        for sample in samples {
                            latencies.push(sample.latency_ms);
                            if sample.from_cache {
                                cache_hits += 1;
                            }
                            stage_sums.coordinator_ms += sample.stages.coordinator_ms;
                            stage_sums.filter_ms += sample.stages.filter_ms;
                            stage_sums.cache_key_ms += sample.stages.cache_key_ms;
                            stage_sums.rewrite_ms += sample.stages.rewrite_ms;
                            stage_sums.embedding_ms += sample.stages.embedding_ms;
                            stage_sums.embedding_provider_ms += sample.stages.embedding_provider_ms;
                            stage_sums.embedding_overhead_ms += sample.stages.embedding_overhead_ms;
                            stage_sums.vector_search_ms += sample.stages.vector_search_ms;
                            stage_sums.bm25_retrieval_ms += sample.stages.bm25_retrieval_ms;
                            stage_sums.fusion_ms += sample.stages.fusion_ms;
                            stage_sums.post_process_ms += sample.stages.post_process_ms;
                            if !sample.ok {
                                errors += 1;
                                if first_error.is_empty() {
                                    first_error =
                                        sample.error.unwrap_or_else(|| "unknown error".to_string());
                                }
                            }
                        }
                    }
                    let total_ms = wave_started.elapsed().as_secs_f64() * 1000.0;
                    let after = resource_snapshot();
                    let resources = resource_delta(before, after);
                    latencies.sort_unstable();

                    let requests = latencies.len();
                    let rate = requests as f64 / (total_ms / 1000.0);
                    let avg = |value: u64| -> f64 {
                        if requests == 0 {
                            0.0
                        } else {
                            value as f64 / requests as f64
                        }
                    };
                    println!(
                        "{mode:<7} c={concurrency:<2} {phase:<4} p50={} p95={} p99={} rate={rate:.1}/s",
                        percentile(&latencies, 0.50),
                        percentile(&latencies, 0.95),
                        percentile(&latencies, 0.99),
                    );
                    let _ = writeln!(
                        out,
                        "{mode}\t{concurrency}\t{phase}\t{requests}\t{errors}\t{first_error}\t{}\t{}\t{}\t{rate:.1}\t{}\t{}\t{}\t{}\t{}",
                        percentile(&latencies, 0.50),
                        percentile(&latencies, 0.95),
                        percentile(&latencies, 0.99),
                        cpu_ms(&resources).unwrap_or(0),
                        after.peak_rss_kb.unwrap_or(0),
                        resources.rss_kb.unwrap_or(0),
                        resources.read_bytes.unwrap_or(0),
                        resources.write_bytes.unwrap_or(0),
                    );
                    let _ = writeln!(
                        stage_out,
                        "{mode}\t{concurrency}\t{phase}\t{requests}\t{errors}\t{first_error}\t{cache_hits}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                        avg(stage_sums.coordinator_ms),
                        avg(stage_sums.filter_ms),
                        avg(stage_sums.cache_key_ms),
                        avg(stage_sums.rewrite_ms),
                        avg(stage_sums.embedding_ms),
                        avg(stage_sums.embedding_provider_ms),
                        avg(stage_sums.embedding_overhead_ms),
                        avg(stage_sums.vector_search_ms),
                        avg(stage_sums.bm25_retrieval_ms),
                        avg(stage_sums.fusion_ms),
                        avg(stage_sums.post_process_ms),
                    );
                }
            }
        }

        let _ = fs::metadata(root.path());
    });
}
