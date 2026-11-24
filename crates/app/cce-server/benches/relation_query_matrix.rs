//! Relation graph query benchmark over a materialized synthetic project.
//!
//! Exercises the published sharded relation snapshot through the real graph
//! query handler. Every request resolves a symbol seed and traverses the
//! call graph, so the shard directory and LRU-backed components are on the
//! measured path.
//!
//! Run with `CCE_PERF_SIZE=medium cargo bench -p cce-server --bench relation_query_matrix`.
//! `CCE_PERF_SIZE=large` selects the 50,000-file fixture. Trends are appended
//! to `benches/results/relation_query_matrix.tsv`.

use std::fs;
use std::io::Write as _;
use std::time::Instant;

use axum::extract::{Path, Query as QueryParams, State};
use cce_api::models::EgoQuery;
use cce_bench_kit::{
    DEFAULT_LANG_DIST, ProjectSize, SyntheticProject, cpu_ms, resource_delta, resource_snapshot,
};
use cce_server::api::handlers::graph;
use cce_server::api::response::ApiResult;
use cce_server_shared::context::SharedContext;

#[path = "../tests/common.rs"]
mod common;

const WAVES: usize = 12;

struct RequestSample {
    latency_ms: u64,
    ok: bool,
    error: Option<String>,
    relation_epoch: Option<i64>,
    nodes: usize,
    edges: usize,
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn size_from_env() -> (ProjectSize, &'static str) {
    match std::env::var("CCE_PERF_SIZE").as_deref() {
        Ok("medium") => (ProjectSize::Medium, "medium"),
        Ok("large") => (ProjectSize::Large, "large"),
        _ => (ProjectSize::Small, "small"),
    }
}

async fn ego_query(
    state: SharedContext,
    project_id: i64,
    seed: &str,
    depth: usize,
    direction: &str,
) -> RequestSample {
    let request = EgoQuery {
        entity_id: seed.to_string(),
        depth,
        direction: direction.to_string(),
        offset: 0,
        limit: 64,
        domains: String::new(),
        relation_types: String::new(),
        include_external: true,
    };
    let started = Instant::now();
    let response =
        graph::handle_graph_ego(State(state), Path(project_id), QueryParams(request)).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match response {
        ApiResult::Success(success) => RequestSample {
            latency_ms,
            ok: true,
            error: None,
            relation_epoch: Some(success.relation_epoch),
            nodes: success.nodes.len(),
            edges: success.edges.len(),
        },
        ApiResult::Error(error) => RequestSample {
            latency_ms,
            ok: false,
            error: Some(format!("{}: {}", error.error.code, error.error.message)),
            relation_epoch: None,
            nodes: 0,
            edges: 0,
        },
    }
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime");

    let (size, tier) = size_from_env();
    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "relation_query_matrix.tsv",
        "# tier\tfiles\tmode\tphase\trequests\terrors\tfirst_error\tfirst_ms\tp50_ms\tp95_ms\tp99_ms\treq_per_sec\tcpu_ms\tpeak_rss_kb\trss_delta_kb\trelation_epoch_present\tnodes\tedges\trelation_nodes\trelation_edges\tshard_count\tshard_directory_entries\tshard_directory_estimated_bytes",
    );

    rt.block_on(async {
        let env = common::test_env().await;
        let root = tempfile::TempDir::new().expect("fixture root");
        let project = SyntheticProject::generate(size, DEFAULT_LANG_DIST);
        project.materialize(root.path()).expect("materialize");
        let src = root.path().join("src");
        fs::create_dir_all(&src).expect("probe src dir");
        for probe in 0..8usize {
            fs::write(
                src.join(format!("relation_probe_{probe:02}.rs")),
                format!(
                    "pub fn shard_callee_{probe:03}() -> i32 {{\n    41\n}}\n\npub fn shard_caller_{probe:03}() -> i32 {{\n    shard_callee_{probe:03}() + 1\n}}\n"
                ),
            )
            .expect("write relation probe");
        }
        let project_id = common::create_project(&env.state, &root.path().to_string_lossy()).await;
        let indexed = common::index_fixture_project(&env.state, project_id, root.path()).await;
        assert!(
            indexed.errors.is_empty(),
            "relation query fixture must index cleanly: {:?}",
            indexed.errors
        );
        let stages = &indexed.stages;
        println!(
            "seeded {} files; shards={} directory_entries={} directory_bytes={}",
            indexed.files_indexed,
            stages.relation_shard_count,
            stages.relation_shard_directory_entries,
            stages.relation_shard_directory_estimated_bytes,
        );

        for (mode, depth, direction) in [("caller_out", 1usize, "out"), ("both_2", 2, "both")] {
            for phase in ["cold", "warm"] {
                let mut latencies = Vec::with_capacity(WAVES);
                let mut errors = 0usize;
                let mut first_error = String::new();
                let mut first_ms = 0u64;
                let mut epoch_present = 0usize;
                let mut nodes = 0usize;
                let mut edges = 0usize;
                let before = resource_snapshot();
                let phase_started = Instant::now();
                for wave in 0..WAVES {
                    let probe = if phase == "warm" {
                        0
                    } else {
                        wave % 8
                    };
                    let seed = format!("shard_caller_{probe:03}");
                    let sample =
                        ego_query(env.state.clone(), project_id, &seed, depth, direction).await;
                    if wave == 0 {
                        first_ms = sample.latency_ms;
                    }
                    latencies.push(sample.latency_ms);
                    if sample.relation_epoch.is_some() {
                        epoch_present += 1;
                    }
                    nodes += sample.nodes;
                    edges += sample.edges;
                    if !sample.ok {
                        errors += 1;
                        if first_error.is_empty() {
                            first_error =
                                sample.error.unwrap_or_else(|| "unknown error".to_string());
                        }
                    }
                }
                let total_ms = phase_started.elapsed().as_secs_f64() * 1000.0;
                let after = resource_snapshot();
                let resources = resource_delta(before, after);
                latencies.sort_unstable();

                let requests = latencies.len();
                let rate = requests as f64 / (total_ms / 1000.0);
                println!(
                    "{mode:<11} {phase:<4} requests={requests} errors={errors} first={first_ms} p50={} p95={} p99={} rate={rate:.1}/s",
                    percentile(&latencies, 0.50),
                    percentile(&latencies, 0.95),
                    percentile(&latencies, 0.99),
                );
                let _ = writeln!(
                    out,
                    "{tier}\t{}\t{mode}\t{phase}\t{requests}\t{errors}\t{first_error}\t{first_ms}\t{}\t{}\t{}\t{rate:.1}\t{}\t{}\t{}\t{epoch_present}\t{nodes}\t{edges}\t{}\t{}\t{}\t{}\t{}",
                    indexed.files_indexed,
                    percentile(&latencies, 0.50),
                    percentile(&latencies, 0.95),
                    percentile(&latencies, 0.99),
                    cpu_ms(&resources).unwrap_or(0),
                    after.peak_rss_kb.unwrap_or(0),
                    resources.rss_kb.unwrap_or(0),
                    stages.relation_nodes,
                    stages.relation_edges,
                    stages.relation_shard_count,
                    stages.relation_shard_directory_entries,
                    stages.relation_shard_directory_estimated_bytes,
                );
            }
        }
    });
}
