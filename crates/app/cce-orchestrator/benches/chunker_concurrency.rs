//! T-3 — chunker concurrency benchmark (covers P-2).
//!
//! The full-index hot path is lock-free: `FileProcessor` shares its
//! `GroupChunker` by reference and each worker owns its parse coordinator,
//! so concurrent files do not serialize on a chunker mutex. The remaining
//! ceiling is executor governance: parse, grouping, conversion and chunking
//! are blocking CPU work, so the production path reads on the async workers
//! and runs the CPU stage on the blocking pool
//! (`FileProcessor::process_entry_blocking`, mirroring
//! `process_batch_with_mode`). This bench drives that same entry point from
//! many concurrent clones and measures wall time at increasing concurrency.
//!
//! The bounded scenario uses the host's logical CPU count; the unbounded
//! scenario pins 64 as an oversubscription guard — throughput parity between
//! the two means adding threads past the core count buys nothing.
//!
//! Run: `cargo bench -p cce-orchestrator --bench chunker_concurrency`
//! Baseline numbers are appended to `benches/results/chunker_concurrency.tsv`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cce_bench_kit::{DEFAULT_LANG_DIST, ProjectSize, SyntheticProject};
use cce_orchestrator::index::FileProcessor;
use cce_scanner::FileEntry;
use cce_types::OutputMode;

/// How many files to process per timed pass (subset of the materialized tier).
const N: usize = 200;
/// Fixed scale tier for the synthetic project.
const SIZE: ProjectSize = ProjectSize::Small;

struct Fixture {
    entries: Vec<FileEntry>,
}

/// Materialize the synthetic project once and build `FileEntry` records that
/// point at the real on-disk files (so `process_file_complete` exercises the
/// full read -> parse -> chunk path).
fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("cce_chunk_bench_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root dir");
    let project = SyntheticProject::generate(SIZE, DEFAULT_LANG_DIST);
    project.materialize(&root).expect("materialize");

    let entries = project
        .files
        .iter()
        .take(N)
        .map(|f| {
            let abs = root.join(&f.path);
            let meta = std::fs::metadata(&abs).expect("stat materialized file");
            FileEntry::new(
                abs,
                f.path.clone(),
                meta.len(),
                meta.modified().expect("mtime").into(),
            )
            .with_language_info(cce_types::language::LanguageInfo::detect_from_path(
                &f.path.to_string_lossy(),
            ))
        })
        .collect();
    Fixture { entries }
}

/// Bounded concurrency for this host: the logical CPU count. CPU-bound
/// parse/chunk work scales with cores and merely flattens past them.
fn blocking_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .max(1)
}

/// Sequential pass: files are processed one at a time through the same
/// production entry point (async read, blocking-pool CPU stage).
async fn run_serial(fp: &FileProcessor, entries: &[FileEntry]) {
    for entry in entries {
        fp.clone()
            .process_entry_blocking(entry.clone(), OutputMode::Both)
            .await
            .expect("process_entry_blocking");
    }
}

/// Unbounded pass: every file gets its own task; all run concurrently.
async fn run_unbounded(fp: &FileProcessor, entries: &[FileEntry]) {
    let mut set = tokio::task::JoinSet::new();
    for entry in entries {
        let e = entry.clone();
        let fp = fp.clone();
        set.spawn(async move {
            fp.process_entry_blocking(e, OutputMode::Both)
                .await
                .expect("process_entry_blocking")
        });
    }
    while let Some(res) = set.join_next().await {
        res.expect("task join");
    }
}

/// Bounded pass: a semaphore caps simultaneous work at `concurrency`.
async fn run_bounded(fp: &FileProcessor, entries: &[FileEntry], concurrency: usize) {
    let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut set = tokio::task::JoinSet::new();
    for entry in entries {
        let e = entry.clone();
        let fp = fp.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore");
            fp.process_entry_blocking(e, OutputMode::Both)
                .await
                .expect("process_entry_blocking")
        });
    }
    while let Some(res) = set.join_next().await {
        res.expect("task join");
    }
}

fn main() {
    let fixture = fixture();
    let entries = &fixture.entries;
    let fp = FileProcessor::default();
    let bounded = blocking_concurrency();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(bounded)
        .enable_all()
        .build()
        .expect("runtime");

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benches/results/chunker_concurrency.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# scenario\tconcurrency\tfiles\tms\tfiles_per_sec");
    }

    // One warm-up so tree-sitter grammars and query caches are steady-state.
    rt.block_on(run_bounded(&fp, entries, bounded));

    let scenarios: Vec<(&str, usize)> =
        vec![("serial", 1), ("bounded", bounded), ("unbounded", 64)];
    let mut timings: Vec<(&str, usize, f64)> = Vec::new();
    for (name, k) in scenarios {
        let ms = match name {
            "serial" => {
                let t = Instant::now();
                rt.block_on(run_serial(&fp, entries));
                t.elapsed().as_secs_f64() * 1000.0
            }
            "unbounded" => {
                let t = Instant::now();
                rt.block_on(run_unbounded(&fp, entries));
                t.elapsed().as_secs_f64() * 1000.0
            }
            _ => {
                let t = Instant::now();
                rt.block_on(run_bounded(&fp, entries, k));
                t.elapsed().as_secs_f64() * 1000.0
            }
        };
        let rate = N as f64 / (ms / 1000.0);
        println!("{name:<12} (k={k:<3}) : {N} files in {ms:8.1} ms -> {rate:8.1} files/s");
        timings.push((name, k, ms));
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{name}\t{k}\t{N}\t{ms:.1}\t{rate:.1}");
        }
    }

    // Relative speedup of parallel over serial tracks executor governance:
    // a value near 1.0x means concurrency buys almost nothing.
    if let (Some(serial), Some(unbounded)) = (
        timings.iter().find(|(n, _, _)| *n == "serial"),
        timings.iter().find(|(n, _, _)| *n == "unbounded"),
    ) {
        let speedup = serial.2 / unbounded.2;
        println!("unbounded/serial speedup: {speedup:.2}x (1.0x = no concurrency benefit)");
    }
    if let (Some(serial), Some(bounded_row)) = (
        timings.iter().find(|(n, _, _)| *n == "serial"),
        timings.iter().find(|(n, _, _)| *n == "bounded"),
    ) {
        let speedup = serial.2 / bounded_row.2;
        println!("bounded(k={bounded})/serial speedup: {speedup:.2}x");
    }

    // Criterion timing for the unbounded scenario (smaller N to bound cost).
    let small: Vec<FileEntry> = entries.iter().take(64).cloned().collect();
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2));
    criterion.bench_function("unbounded_64files", |b| {
        b.iter(|| {
            rt.block_on(run_unbounded(&fp, &small));
        })
    });
    criterion.final_summary();
}
