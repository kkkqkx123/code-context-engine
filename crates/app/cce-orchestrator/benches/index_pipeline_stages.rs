//! T-1 — full index pipeline end-to-end stage-breakdown benchmark.
//!
//! Drives the real `IndexOrchestrator::execute` over a materialized synthetic
//! project and times the whole pass. With `store_vectors`/`store_bm25`/
//! `build_relations` all disabled this exercises exactly the stages that have no
//! dedicated component bench: scanner walk, per-file parse, group/convert/
//! chunk, and the metadb record write. The embedding/vector/BM25/relation
//! stages are measured separately by T-2/T-5/T-6/T-7/T-8, so the per-stage
//! share of the end-to-end wall time is reconstructed from those results.
//!
//! Run: `cargo run -p cce-orchestrator --bench index_pipeline_stages`
//! Baseline numbers are appended to `benches/results/index_pipeline_stages.tsv`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_bench_kit::{DEFAULT_LANG_DIST, ProjectSize, SyntheticProject};
use cce_orchestrator::index::{IndexOptions, IndexOrchestrator};
use cce_storage_metadb_sqlite::SqliteClient;

/// Scale tier for the synthetic project.
const SIZE: ProjectSize = ProjectSize::Small;

/// Build an in-memory metadata store with one seeded project (id 1).
fn make_metadata() -> Arc<SqliteClient> {
    let client = SqliteClient::in_memory().expect("in-memory sqlite");
    client
        .with_transaction(|tx| {
            cce_storage_metadb_sqlite::ProjectRepository::insert(
                tx,
                &cce_storage_metadb_sqlite::NewProjectRecord::new(
                    "bench".to_string(),
                    "/tmp/bench".to_string(),
                ),
            )
        })
        .expect("seed project");
    Arc::new(client)
}

/// Materialize the synthetic project to a fresh temp dir and return its root.
fn materialize() -> PathBuf {
    let root = std::env::temp_dir().join(format!("cce_pipe_bench_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root dir");
    let project = SyntheticProject::generate(SIZE, DEFAULT_LANG_DIST);
    let file_count = project.files.len();
    project.materialize(&root).expect("materialize");
    println!("materialized {file_count} synthetic files at {root:?}");
    root
}

async fn run_once(root: &std::path::Path, metadata: Arc<SqliteClient>) -> u64 {
    let mut orchestrator = IndexOrchestrator::new(1)
        .expect("orchestrator")
        .with_metadata_store(metadata);
    let options = IndexOptions {
        root_dir: root.to_path_buf(),
        extensions: vec![
            "rs".to_string(),
            "py".to_string(),
            "go".to_string(),
            "ts".to_string(),
            "cpp".to_string(),
            "c".to_string(),
            "java".to_string(),
            "txt".to_string(),
        ],
        exclude_dirs: Vec::new(),
        max_concurrency: 10,
        build_relations: false,
        store_vectors: false,
        store_bm25: false,
        store_summaries: false,
        respect_gitignore: false,
        additional_ignore_patterns: Vec::new(),
        custom_gitignore_path: None,
        parallelism: None,
    };
    let result = orchestrator.execute(options).await.expect("execute");
    result.indexed_files as u64
}

fn main() {
    let root = materialize();
    let metadata = make_metadata();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("runtime");

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/index_pipeline_stages.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# tier\tfiles\tms\tfiles_per_sec\tbytes_written");
    }

    // Warm-up so tree-sitter grammars and sqlite pages are steady-state.
    let _ = rt.block_on(run_once(&root, metadata.clone()));

    // End-to-end timed pass at the fixed scale tier.
    let t = Instant::now();
    let indexed = rt.block_on(run_once(&root, metadata.clone()));
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let rate = indexed as f64 / (ms / 1000.0);
    println!(
        "end_to_end ({:?}): {indexed} files in {ms:8.1} ms -> {rate:8.1} files/s",
        SIZE
    );
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "{}\t{indexed}\t{ms:.1}\t{rate:.1}\t0", SIZE.label());
    }

    // Criterion timing for the end-to-end pass (modest sampling to bound cost).
    // Each iteration gets a fresh in-memory metadata store: reusing the
    // warmed-up store would re-index the same 500 files into ever-growing
    // tables, so the timing would measure accumulation rather than
    // steady-state indexing cost.
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(8))
        .warm_up_time(Duration::from_secs(2));
    criterion.bench_function("end_to_end_small", |b| {
        b.iter(|| {
            let fresh = make_metadata();
            let _ = rt.block_on(run_once(&root, fresh));
        })
    });
    criterion.final_summary();

    let _ = std::fs::remove_dir_all(&root);
}
