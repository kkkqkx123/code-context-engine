//! Service-level backend matrix over local and remote storage branches.
//!
//! Each run creates an isolated project, index and collection, then measures
//! indexing and the vector/BM25/hybrid retrieval path. Remote services must
//! be healthy before the run starts; a missing service fails the benchmark
//! instead of being recorded as request errors.
//!
//! Run: `cargo bench -p cce-server --bench service_remote_backend_matrix`
//! Reports: `service_remote_backend_matrix.tsv`,
//! `service_remote_backend_index.tsv`, `service_remote_backend_index_stages.tsv`
//! and `service_remote_backend_stages.tsv` under `benches/results/`.

use std::fs;
use std::io::{Read, Write as _};
use std::net::TcpStream;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use cce_api::models::{QueryStageTimingsDto, SearchRequest};
use cce_bench_kit::{
    DEFAULT_LANG_DIST, ProjectSize, SyntheticProject, cpu_ms, resource_delta, resource_snapshot,
};
use cce_config::modules::{FulltextBackend, VectorBackend};
use cce_server::api::handlers::search;
use cce_server::api::response::ApiResult;
use cce_server_shared::context::SharedContext;
use tokio::task::JoinSet;

#[path = "../tests/common.rs"]
mod common;

const WAVES: usize = 3;
const INDEX_MODES: [(&str, VectorBackend, FulltextBackend); 4] = [
    ("local/local", VectorBackend::Local, FulltextBackend::Local),
    (
        "qdrant/local",
        VectorBackend::Qdrant,
        FulltextBackend::Local,
    ),
    ("local/es", VectorBackend::Local, FulltextBackend::Remote),
    ("qdrant/es", VectorBackend::Qdrant, FulltextBackend::Remote),
];

struct RequestSample {
    latency_ms: u64,
    ok: bool,
    error: Option<String>,
    stages: QueryStageTimingsDto,
    from_cache: bool,
}

fn fulltext_label(backend: FulltextBackend) -> &'static str {
    match backend {
        FulltextBackend::Local => "local",
        FulltextBackend::Remote => "es",
    }
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Minimal HTTP probe over a plain TCP connection. The benchmark crate stays
/// dependency-free and fails fast when a service does not answer.
fn http_probe(base_url: &str, path: &str) -> Result<(u16, String), String> {
    let rest = base_url
        .strip_prefix("http://")
        .ok_or_else(|| format!("only http URLs are supported: {base_url}"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let mut stream = TcpStream::connect(authority)
        .map_err(|error| format!("connect {authority} failed: {error}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|error| format!("set read timeout failed: {error}"))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("probe write failed: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("probe read failed: {error}"))?;
    let text = String::from_utf8_lossy(&response);
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("malformed probe response from {authority}{path}"))?;
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Ok((status, body))
}

fn require_healthy(base_url: &str, health_path: &str, version_path: &str, label: &str) -> String {
    let (status, health) = http_probe(base_url, health_path)
        .unwrap_or_else(|error| panic!("{label} health check failed: {error}"));
    assert!(
        (200..300).contains(&status),
        "{label} health check returned {status}: {health}"
    );
    let (_, version_body) = http_probe(base_url, version_path)
        .unwrap_or_else(|error| panic!("{label} version probe failed: {error}"));
    let version = serde_json::from_str::<serde_json::Value>(&version_body)
        .ok()
        .and_then(|value| {
            value
                .get("version")
                .and_then(|version| {
                    version
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| version.get("number")?.as_str().map(str::to_string))
                })
                .or_else(|| {
                    value
                        .get("result")?
                        .get("version")?
                        .as_str()
                        .map(str::to_string)
                })
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("{label} healthy; version={version}");
    version
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
            let latency_ms = started.elapsed().as_millis() as u64;
            let (ok, error, stages, from_cache) = match response {
                ApiResult::Success(success) => (true, None, success.stages, success.from_cache),
                ApiResult::Error(failure) => (
                    false,
                    Some(format!("{}: {}", failure.error.code, failure.error.message)),
                    QueryStageTimingsDto::default(),
                    false,
                ),
            };
            RequestSample {
                latency_ms,
                ok,
                error,
                stages,
                from_cache,
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

    let qdrant_url = std::env::var("CCE_TEST_QDRANT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:16333".to_string());
    let es_url =
        std::env::var("CCE_TEST_ES_URL").unwrap_or_else(|_| "http://127.0.0.1:19200".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_nanos();
    let run_id = format!("{:x}", nanos & 0xffff_ffff_ffff);
    let service_version =
        std::env::var("CCE_SERVICE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
    let size = match std::env::var("CCE_BENCH_SIZE").as_deref() {
        Ok("medium") => ProjectSize::Medium,
        _ => ProjectSize::Small,
    };
    let needed_qdrant = INDEX_MODES
        .iter()
        .any(|(_, vector, _)| matches!(vector, VectorBackend::Qdrant));
    let needed_es = INDEX_MODES
        .iter()
        .any(|(_, _, fulltext)| matches!(fulltext, FulltextBackend::Remote));
    let qdrant_version = if needed_qdrant {
        require_healthy(&qdrant_url, "/healthz", "/", "qdrant")
    } else {
        "not-used".to_string()
    };
    let es_version = if needed_es {
        require_healthy(&es_url, "/", "/", "elasticsearch")
    } else {
        "not-used".to_string()
    };

    let mut index_out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_remote_backend_index.tsv",
        "# run_id\tservice_version\tvector_backend\tfulltext_backend\tbackend_combination\tfile_tier\tfiles\tfiles_indexed\terrors\tindex_wall_ms\tcpu_ms\tpeak_rss_kb\trss_delta_kb\tread_bytes_delta\twrite_bytes_delta\tqdrant_version\tes_version\thnsw_preset",
    );
    let mut index_stages_out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_remote_backend_index_stages.tsv",
        "# run_id\tservice_version\tbackend_combination\tfile_tier\tscan_ms\tbatch_process_ms\tbatch_setup_ms\tentity_store_ms\tvector_store_ms\tbm25_store_ms\tsummary_ms\trelation_ms\texport_ms\trelation_spool_ms\trelation_spool_write_ms\trelation_symbols_ms\tsymbol_modules_ms\tsymbol_type_index_ms\tsymbol_overload_ms\tsymbol_flat_ms\tsymbol_cross_file_ms\texport_spool_ms\tmanifest_ms\tbatch_overhead_ms\tbatches\tvector_store_calls\tbm25_store_calls",
    );
    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_remote_backend_matrix.tsv",
        "# run_id\tservice_version\tvector_backend\tfulltext_backend\tbackend_combination\tfile_tier\tmode\tconcurrency\tphase\trequests\terrors\tfirst_error\tp50_ms\tp95_ms\tp99_ms\treq_per_sec\tcpu_ms\tpeak_rss_kb\trss_delta_kb\tread_bytes_delta\twrite_bytes_delta",
    );
    let mut stage_out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "service_remote_backend_stages.tsv",
        "# run_id\tservice_version\tbackend_combination\tfile_tier\tmode\tconcurrency\tphase\trequests\tresult_cache_hits\tembedding_cache_hits\tavg_coordinator_ms\tavg_filter_ms\tavg_cache_key_ms\tavg_rewrite_ms\tavg_embedding_ms\tavg_embedding_first_ms\tavg_embedding_provider_ms\tavg_embedding_overhead_ms\tavg_vector_ms\tavg_bm25_ms\tavg_fusion_ms\tavg_post_ms\tavg_response_render_ms",
    );

    rt.block_on(async {
        for (combination, vector, fulltext) in INDEX_MODES {
            // Every combination gets its own project, collection and index.
            let slug = combination.replace('/', "_");
            let root = tempfile::TempDir::new().expect("fixture root");
            let project = SyntheticProject::generate(size, DEFAULT_LANG_DIST);
            project.materialize(root.path()).expect("materialize");
            let backends = common::TestBackends {
                vector_backend: vector,
                fulltext_backend: fulltext,
                qdrant_url: Some(qdrant_url.clone()),
                qdrant_collection: Some(format!("cce_bench_{}_{}", run_id, slug)),
                fulltext_url: Some(es_url.clone()),
                fulltext_index: Some(format!("cce-bench-{run_id}-{slug}")),
                metadata_backend: cce_config::modules::MetadataBackend::Local,
                metadata_url: None,
                embedding_rate_limit: 0,
            };
            let env = common::test_env_with_backends(backends).await;
            let project_id =
                common::create_project(&env.state, &root.path().to_string_lossy()).await;
            let before_index = resource_snapshot();
            let index_started = Instant::now();
            let indexed =
                common::index_fixture_project(&env.state, project_id, root.path()).await;
            let index_wall_ms = index_started.elapsed().as_millis() as u64;
            let after_index = resource_snapshot();
            let index_resources = resource_delta(before_index, after_index);
            assert!(
                indexed.errors.is_empty(),
                "{combination} fixture must index cleanly: {:?}",
                indexed.errors
            );
            let _ = writeln!(
                index_out,
                "{run_id}\t{service_version}\t{}\t{}\t{combination}\t{}\t{}\t{}\t0\t{index_wall_ms}\t{}\t{}\t{}\t{}\t{}\t{qdrant_version}\t{es_version}\tm16-ef100-search64-threshold10000",
                vector.as_str(),
                fulltext_label(fulltext),
                size.label(),
                size.file_count(),
                indexed.files_indexed,
                cpu_ms(&index_resources).unwrap_or(0),
                after_index.peak_rss_kb.unwrap_or(0),
                index_resources.rss_kb.unwrap_or(0),
                index_resources.read_bytes.unwrap_or(0),
                index_resources.write_bytes.unwrap_or(0),
            );
            let stages = &indexed.stages;
            let _ = writeln!(
                index_stages_out,
                "{run_id}\t{service_version}\t{combination}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                size.label(),
                stages.scan_ms,
                stages.batch_process_ms,
                stages.batch_setup_ms,
                stages.entity_store_ms,
                stages.vector_store_ms,
                stages.bm25_store_ms,
                stages.summary_ms,
                stages.relation_ms,
                stages.export_ms,
                stages.relation_spool_ms,
                stages.relation_spool_write_ms,
                stages.relation_symbols_ms,
                stages.symbol_modules_ms,
                stages.symbol_type_index_ms,
                stages.symbol_overload_ms,
                stages.symbol_flat_ms,
                stages.symbol_cross_file_ms,
                stages.export_spool_ms,
                stages.manifest_ms,
                stages.batch_overhead_ms,
                stages.batches,
                stages.vector_store_calls,
                stages.bm25_store_calls,
            );
            println!(
                "{combination}: indexed {} files in {index_wall_ms} ms",
                indexed.files_indexed
            );

            for mode in ["vector", "bm25", "hybrid"] {
                for concurrency in [1usize, 4, 16] {
                    for phase in ["cold", "warm"] {
                        let mut latencies = Vec::with_capacity(WAVES * concurrency);
                        let mut errors = 0usize;
                        let mut first_error = String::new();
                        let mut stage_sums = QueryStageTimingsDto::default();
                        let mut result_cache_hits = 0usize;
                        let before = resource_snapshot();
                        let wave_started = Instant::now();
                        for wave in 0..WAVES {
                            let query = if phase == "warm" {
                                "fn_00001".to_string()
                            } else {
                                format!("fn_{:05} wave-{wave}-{mode}-{concurrency}", wave + 1)
                            };
                            for sample in run_wave(
                                env.state.clone(),
                                project_id,
                                mode,
                                concurrency,
                                query,
                            )
                            .await
                            {
                                latencies.push(sample.latency_ms);
                                if sample.from_cache {
                                    result_cache_hits += 1;
                                }
                                stage_sums.coordinator_ms += sample.stages.coordinator_ms;
                                stage_sums.filter_ms += sample.stages.filter_ms;
                                stage_sums.cache_key_ms += sample.stages.cache_key_ms;
                                stage_sums.rewrite_ms += sample.stages.rewrite_ms;
                                stage_sums.embedding_ms += sample.stages.embedding_ms;
                                stage_sums.embedding_first_ms += sample.stages.embedding_first_ms;
                                stage_sums.embedding_provider_ms +=
                                    sample.stages.embedding_provider_ms;
                                stage_sums.embedding_overhead_ms +=
                                    sample.stages.embedding_overhead_ms;
                                stage_sums.embedding_cache_hits +=
                                    sample.stages.embedding_cache_hits;
                                stage_sums.vector_search_ms += sample.stages.vector_search_ms;
                                stage_sums.bm25_retrieval_ms += sample.stages.bm25_retrieval_ms;
                                stage_sums.fusion_ms += sample.stages.fusion_ms;
                                stage_sums.post_process_ms += sample.stages.post_process_ms;
                                stage_sums.response_render_ms += sample.stages.response_render_ms;
                                if !sample.ok {
                                    errors += 1;
                                    if first_error.is_empty() {
                                        first_error = sample
                                            .error
                                            .unwrap_or_else(|| "unknown error".to_string());
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
                        let _ = writeln!(
                            stage_out,
                            "{run_id}\t{service_version}\t{combination}\t{}\t{mode}\t{concurrency}\t{phase}\t{requests}\t{result_cache_hits}\t{}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                            size.label(),
                            stage_sums.embedding_cache_hits,
                            avg(stage_sums.coordinator_ms),
                            avg(stage_sums.filter_ms),
                            avg(stage_sums.cache_key_ms),
                            avg(stage_sums.rewrite_ms),
                            avg(stage_sums.embedding_ms),
                            avg(stage_sums.embedding_first_ms),
                            avg(stage_sums.embedding_provider_ms),
                            avg(stage_sums.embedding_overhead_ms),
                            avg(stage_sums.vector_search_ms),
                            avg(stage_sums.bm25_retrieval_ms),
                            avg(stage_sums.fusion_ms),
                            avg(stage_sums.post_process_ms),
                            avg(stage_sums.response_render_ms),
                        );
                        let _ = writeln!(
                            out,
                            "{run_id}\t{service_version}\t{}\t{}\t{combination}\t{}\t{mode}\t{concurrency}\t{phase}\t{requests}\t{errors}\t{first_error}\t{}\t{}\t{}\t{rate:.1}\t{}\t{}\t{}\t{}\t{}",
                            vector.as_str(),
                            fulltext_label(fulltext),
                            size.label(),
                            percentile(&latencies, 0.50),
                            percentile(&latencies, 0.95),
                            percentile(&latencies, 0.99),
                            cpu_ms(&resources).unwrap_or(0),
                            after.peak_rss_kb.unwrap_or(0),
                            resources.rss_kb.unwrap_or(0),
                            resources.read_bytes.unwrap_or(0),
                            resources.write_bytes.unwrap_or(0),
                        );
                    }
                }
            }
            let _ = fs::metadata(root.path());
        }
    });
    let _ = write!(
        std::io::stdout(),
        "backend matrix complete; run_id={run_id}\n"
    );
}
