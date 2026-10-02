//! Crash-point recovery curve benchmark.
//!
//! Covers the interruption-recovery benchmark needs: how recovery cost
//! varies with the crash point (completed-batch proportion) and what the
//! file-list-hash / batch-boundary mismatch fallback costs. Uses the real
//! production path: `FileIndexer::initialize` (scan, sort, hash, checkpoint),
//! `FileIndexer::recover` (rescan plus hash validation), and
//! `FileIndexer::validate_checkpoint` (hash plus batch-boundary probes with
//! predecessor fallback).
//!
//! Run with: `cargo run -p cce-orchestrator --bench recovery_curve`
//!
//! Results are printed to stdout and appended to
//! `benches/results/recovery_curve.tsv`.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cce_orchestrator::index::FileIndexer;
use cce_scanner::{FSScanner, ScanOptions};
use cce_storage_sqlite::BatchCheckpointRecord;

const FILE_COUNT: usize = 300;
const BATCH_SIZE: usize = 50;

fn write_files(root: &Path, count: usize) {
    for i in 0..count {
        let dir = root.join(format!("src/mod_{:03}", i % 10));
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            dir.join(format!("file_{i:04}.rs")),
            format!("pub fn func_{i}(x: u64) -> u64 {{ x * 2 + {i} }}\n"),
        )
        .expect("write file");
    }
}

fn scan_options(root: &Path) -> ScanOptions {
    ScanOptions {
        root_path: root.to_string_lossy().into_owned(),
        ..Default::default()
    }
}

fn boundary_record(
    indexer: &FileIndexer,
    operation_id: &str,
    batch_index: u32,
) -> BatchCheckpointRecord {
    let batch = indexer
        .get_batch(batch_index as usize)
        .expect("batch must exist");
    BatchCheckpointRecord {
        id: None,
        operation_id: operation_id.to_string(),
        batch_index,
        first_file: batch
            .first()
            .map(|f| f.path.to_string_lossy().to_string())
            .unwrap_or_default(),
        last_file: batch
            .last()
            .map(|f| f.path.to_string_lossy().to_string())
            .unwrap_or_default(),
        file_count: batch.len() as u32,
        processed_files: batch.len() as u32,
        failed_files: 0,
        entities_extracted: 0,
        relations_found: 0,
        chunks_generated: 0,
        vectors_stored: 0,
        start_time: String::new(),
        end_time: None,
        duration_ms: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn main() {
    println!("recovery_curve benchmark (debug, {FILE_COUNT} files, batch {BATCH_SIZE})");
    println!("{:<34} {:>12} {:>10}", "case", "ms", "outcome");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/recovery_curve.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\toutcome");
    }
    let mut row = |label: &str, ms: f64, outcome: &str| {
        println!("{label:<34} {ms:>12.2} {outcome:>10}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{outcome}");
        }
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let _ = FSScanner::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    write_files(tmp.path(), FILE_COUNT);
    let opts = scan_options(tmp.path());

    // Fixed cost of a fresh start: scan, deterministic sort, file-list hash.
    let start = Instant::now();
    let indexer = rt
        .block_on(FileIndexer::initialize(
            tmp.path(),
            BATCH_SIZE,
            &opts,
            None,
            None,
            None,
        ))
        .expect("initialize");
    row(
        "initialize fresh",
        start.elapsed().as_secs_f64() * 1000.0,
        "ok",
    );
    let total_batches = indexer.total_batches();

    // Completed-scale curve: crash at different batch indices. Recovery
    // always rescans everything, so cost must be flat in completed work.
    for crash_at in [1u32, 3, 5] {
        let at = crash_at.min(total_batches as u32);
        let mut checkpoint = indexer.checkpoint().clone();
        checkpoint.current_batch_index = at;
        let start = Instant::now();
        let recovered = FileIndexer::recover(tmp.path(), BATCH_SIZE, &opts, checkpoint, None);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        row(
            &format!("recover clean crash@{at}"),
            ms,
            if recovered.is_ok() { "ok" } else { "err" },
        );
    }

    // Mismatch cliff: one added file, one modified file, one batch-size
    // change. Each must fail fast and force a fresh start (redo from zero).
    let mut tampered = indexer.checkpoint().clone();
    tampered.current_batch_index = 3;
    fs::write(
        tmp.path().join("src/mod_000/file_9999.rs"),
        "pub fn extra() {}\n",
    )
    .expect("add file");
    let start = Instant::now();
    let outcome = FileIndexer::recover(tmp.path(), BATCH_SIZE, &opts, tampered.clone(), None);
    row(
        "recover file-added",
        start.elapsed().as_secs_f64() * 1000.0,
        if outcome.is_err() { "fresh" } else { "ok?" },
    );
    fs::remove_file(tmp.path().join("src/mod_000/file_9999.rs")).expect("remove file");

    fs::write(
        tmp.path().join("src/mod_000/file_0000.rs"),
        "pub fn func_0_changed(x: u64) -> u64 { x + 999 }\n",
    )
    .expect("modify file");
    let start = Instant::now();
    let outcome = FileIndexer::recover(tmp.path(), BATCH_SIZE, &opts, tampered.clone(), None);
    row(
        "recover content-changed",
        start.elapsed().as_secs_f64() * 1000.0,
        if outcome.is_err() { "fresh" } else { "ok?" },
    );
    // Restore original content for the remaining cases.
    fs::write(
        tmp.path().join("src/mod_000/file_0000.rs"),
        "pub fn func_0(x: u64) -> u64 { x * 2 + 0 }\n",
    )
    .expect("restore file");

    let start = Instant::now();
    let outcome = FileIndexer::recover(tmp.path(), 25, &opts, tampered.clone(), None);
    row(
        "recover batchsize-changed",
        start.elapsed().as_secs_f64() * 1000.0,
        if outcome.is_err() { "fresh" } else { "ok?" },
    );

    // Batch-boundary probes: intact boundaries pass; a stale resume-start
    // batch falls back to its predecessor; two stale boundaries fail.
    let operation_id = indexer.operation_id().to_string();
    let bounds: Vec<BatchCheckpointRecord> = (0..=3)
        .map(|b| boundary_record(&indexer, &operation_id, b))
        .collect();
    let start = Instant::now();
    let ok = indexer.validate_checkpoint(&tampered, &bounds);
    row(
        "validate boundaries intact",
        start.elapsed().as_secs_f64() * 1000.0,
        if ok.is_ok() { "ok" } else { "err" },
    );

    let mut stale_start = bounds.clone();
    stale_start[3].first_file = "src/mod_000/file_XXXX.rs".to_string();
    let start = Instant::now();
    let ok = indexer.validate_checkpoint(&tampered, &stale_start);
    row(
        "validate stale start+fallback",
        start.elapsed().as_secs_f64() * 1000.0,
        if ok.is_ok() { "ok" } else { "err" },
    );

    let mut stale_both = stale_start.clone();
    stale_both[2].first_file = "src/mod_000/file_YYYY.rs".to_string();
    let start = Instant::now();
    let ok = indexer.validate_checkpoint(&tampered, &stale_both);
    row(
        "validate stale start+prev",
        start.elapsed().as_secs_f64() * 1000.0,
        if ok.is_err() { "fresh" } else { "ok?" },
    );
}
