//! T-10 — service-layer search concurrency benchmark (covers the API/handler
//! fan-out).
//!
//! The service layer serves `handle_search` from a shared `AppState`. This bench
//! builds a real engine over the standard 3-file fixture (via the project's own
//! test harness), indexes it, then issues `K` concurrent `handle_search` calls
//! and measures wall time. The one-time index is shared; only the request fan-out
//! is timed, so the numbers describe how the handler/engine scale under
//! concurrent load rather than indexing cost.
//!
//! Run: `cargo run -p cce-server --release --bench service_layer_concurrency`
//! Baseline numbers are appended to `benches/results/service_layer_concurrency.tsv`.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use cce_api::models::SearchRequest;
use cce_server::api::handlers::search;
use cce_server::api::state::AppState;
use tokio::task::JoinSet;

// Reuse the project's own integration-test harness to stand up an engine and a
// seeded, fully-indexed project without re-implementing the LLM/BM25/vector stub.
#[path = "../tests/common.rs"]
mod common;

/// Issue `k` concurrent `handle_search` calls against `state` and return how
/// many completed. `SearchRequest` is not `Clone`, so each task builds its own
/// (they are cheap and deterministic for a fixed project + query).
async fn run_concurrent(state: AppState, project_id: i64, k: usize) -> usize {
    let mut set = JoinSet::new();
    for _ in 0..k {
        let state = state.clone();
        set.spawn(async move {
            let request = SearchRequest {
                project_id: Some(project_id),
                query: "fixture_caller".to_string(),
                query_type: "hybrid".to_string(),
                limit: 10,
                ..Default::default()
            };
            let _ = search::handle_search(State(state), Json(request)).await;
        });
    }
    let mut done = 0;
    while let Some(res) = set.join_next().await {
        res.expect("search task join");
        done += 1;
    }
    done
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        // Build engine + index the standard fixture (one-time setup).
        let (env, _root, project_id) = common::seeded_project().await;
        let state = env.state.clone();
        println!("seeded + indexed project {project_id}; running concurrent searches");

        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("benches/results/service_layer_concurrency.tsv"),
            )
            .ok();
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "# concurrency\trequests\tms\treq_per_sec");
        }

        for k in [1usize, 16, 64] {
            let t = Instant::now();
            let done = run_concurrent(state.clone(), project_id, k).await;
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let rate = done as f64 / (ms / 1000.0);
            println!("concurrency={k:<3} : {done} searches in {ms:8.1} ms -> {rate:8.1} req/s");
            if let Some(f) = out.as_mut() {
                let _ = writeln!(f, "{k}\t{done}\t{ms:.1}\t{rate:.1}");
            }
        }

        // Criterion timing for the mid-range concurrency level.
        let mut criterion = criterion::Criterion::default()
            .sample_size(10)
            .measurement_time(Duration::from_secs(10))
            .warm_up_time(Duration::from_secs(3));
        criterion.bench_function("concurrent_16", |b| {
            b.iter(|| {
                rt.block_on(run_concurrent(state.clone(), project_id, 16));
            })
        });
        criterion.final_summary();
    });
}
