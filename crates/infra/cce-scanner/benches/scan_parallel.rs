//! T-9 — scanner parallel hashing & plugin-filter overhead benchmark (covers P-8).
//!
//! Extends the existing `scan_hash` coverage with two controlled comparisons:
//!
//! 1. **Single-thread vs bounded-concurrency full-content hashing.** The current
//!    `FSScanner::scan` reads and hashes every file on one thread. This bench
//!    replays the same file set through `FileProcessor::process_file_with_metadata`
//!    sequentially vs through a bounded `rayon` pool, isolating the hashing
//!    parallelism win the doc calls for.
//!
//! 2. **Per-file filter-decision overhead.** Compares a scan with an empty
//!    filter config against one carrying a heavy gitignore + exclude pattern
//!    set, approximating the per-file decision cost a `FileFilter` plugin would
//!    add. The production-accurate measurement registers a real Lua/native
//!    `FileFilter` plugin via `cce_plugin_runtime` and re-runs the same delta;
//!    the hook is left explicit below.
//!
//! Run: `cargo run -p cce-scanner --bench scan_parallel`

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use cce_bench_kit::ProjectSize;
use cce_scanner::FileProcessor;
use cce_scanner::{FSScanner, ScanOptions};

/// Open the results TSV for appending, creating `benches/results` on demand.
///
/// The directory is not part of the worktree, so a first run used to lose every
/// measurement: `open(...).ok()` failed silently and each row was guarded by
/// `if let Some(f)`. Failing loudly is the point — a bench that cannot record
/// its own output is broken.
fn results_tsv(name: &str) -> std::fs::File {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results");
    fs::create_dir_all(&dir).expect("create benches/results");
    let path = dir.join(name);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
}

struct Harness {
    root: PathBuf,
    paths: Vec<(PathBuf, std::fs::Metadata)>,
}

fn harness() -> &'static Harness {
    static H: OnceLock<Harness> = OnceLock::new();
    H.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("cce_scan_bench_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mk temp");
        let proj = cce_bench_kit::SyntheticProject::generate(
            ProjectSize::Medium,
            cce_bench_kit::DEFAULT_LANG_DIST,
        );
        proj.materialize(&dir).expect("materialize");

        // Collect (path, metadata) once; both strategies hash this exact set.
        let mut paths = Vec::new();
        collect(&dir, &mut paths);
        Harness { root: dir, paths }
    })
}

fn collect(dir: &Path, out: &mut Vec<(PathBuf, std::fs::Metadata)>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        let md = fs::metadata(&path).expect("metadata");
        if md.is_dir() {
            collect(&path, out);
        } else if md.is_file() {
            out.push((path, md));
        }
    }
}

fn scan_opts(root: &Path, heavy_filter: bool) -> ScanOptions {
    let mut opts = ScanOptions {
        root_path: root.to_string_lossy().into_owned(),
        include_patterns: vec!["**/*".to_string()],
        exclude_patterns: vec![],
        follow_symlinks: false,
        respect_gitignore: false,
        gitignore_patterns: vec![],
        gitignore_path: None,
        max_content_size: Some(1 << 20),
        max_file_size: Some(1 << 22),
        parallelism: None,
    };
    if heavy_filter {
        // Standing in for a plugin FileFilter decision: a large pattern set
        // forces the per-file matcher to do real work on every entry.
        opts.respect_gitignore = true;
        opts.exclude_patterns = (0..200).map(|i| format!("**/scratch_{i}/**")).collect();
        opts.gitignore_patterns = (0..200).map(|i| format!("build_{i}/")).collect();
    }
    opts
}

fn main() {
    let h = harness();
    println!(
        "scan_parallel benchmark: {} files under {}",
        h.paths.len(),
        h.root.display()
    );

    // --- 1. single-thread vs bounded concurrency hashing ---
    let mut out = results_tsv("scan_parallel.tsv");
    let _ = writeln!(
        out,
        "# scenario\tthreads\tfiles\tms\tfiles_per_sec\tspeedup"
    );

    let fp = Arc::new(FileProcessor::new());
    let t = Instant::now();
    let mut hashed = 0usize;
    for (p, m) in &h.paths {
        let _ = fp.process_file_with_metadata(p, &h.root, m);
        hashed += 1;
    }
    let single_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("single_thread hash : {hashed} files in {single_ms:8.1} ms");
    let rate = hashed as f64 / (single_ms / 1000.0);
    let _ = writeln!(
        out,
        "single_thread\t1\t{hashed}\t{single_ms:.1}\t{rate:.1}\t1.00"
    );

    for threads in [2usize, 4, 8, num_cpus::get()] {
        let fp = Arc::new(FileProcessor::new());
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("rayon pool");
        let t = Instant::now();
        pool.install(|| {
            use rayon::prelude::*;
            h.paths.par_iter().for_each(|(p, m)| {
                let fp = fp.clone();
                let _ = fp.process_file_with_metadata(p, &h.root, m);
            });
        });
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let speedup = single_ms / ms;
        println!(
            "bounded({threads:>2}t) hash : {hashed} files in {ms:8.1} ms  (speedup {speedup:5.2}x)"
        );
        let rate = hashed as f64 / (ms / 1000.0);
        let _ = writeln!(
            out,
            "bounded\t{threads}\t{hashed}\t{ms:.1}\t{rate:.1}\t{speedup:.2}"
        );
    }

    // --- 1b. walker-level scan: serial vs global pool (product path) ---
    // The hashing comparison above drives the processor directly; this drives
    // the real `FSScanner` walk so the integrated parallel file pass (which
    // M4 added) is measured, including the serial fallback the parallelism
    // config selects.
    let serial_opts = ScanOptions {
        parallelism: Some(1),
        ..scan_opts(&h.root, false)
    };
    let t = Instant::now();
    let serial_entries = FSScanner::new().scan(&serial_opts).expect("serial scan");
    let serial_scan_ms = t.elapsed().as_secs_f64() * 1000.0;

    let default_opts = scan_opts(&h.root, false);
    let t = Instant::now();
    let parallel_entries = FSScanner::new().scan(&default_opts).expect("parallel scan");
    let parallel_scan_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(serial_entries.len(), parallel_entries.len());
    let pool_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    println!(
        "walker scan         : serial {serial_scan_ms:8.1} ms vs pool {parallel_scan_ms:8.1} ms \
         ({} files, speedup {:.2}x)",
        serial_entries.len(),
        serial_scan_ms / parallel_scan_ms
    );
    let _ = writeln!(
        out,
        "walker_scan\t1\t{}\t{serial_scan_ms:.1}\t{:.1}\t1.00",
        serial_entries.len(),
        serial_entries.len() as f64 / (serial_scan_ms / 1000.0)
    );
    let _ = writeln!(
        out,
        "walker_scan\t{pool_threads}\t{}\t{parallel_scan_ms:.1}\t{:.1}\t{:.2}",
        parallel_entries.len(),
        parallel_entries.len() as f64 / (parallel_scan_ms / 1000.0),
        serial_scan_ms / parallel_scan_ms
    );

    // --- 2. per-file filter-decision overhead (plugin FFI proxy) ---
    let t = Instant::now();
    let _ = FSScanner::new().scan(&scan_opts(&h.root, false));
    let no_filter_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let _ = FSScanner::new().scan(&scan_opts(&h.root, true));
    let heavy_filter_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "filter overhead     : empty {no_filter_ms:8.1} ms vs heavy-pattern {heavy_filter_ms:8.1} ms \
         (delta {:.1} ms over {} files)",
        heavy_filter_ms - no_filter_ms,
        h.paths.len()
    );
    let n = h.paths.len();
    let delta = heavy_filter_ms - no_filter_ms;
    let _ = writeln!(
        out,
        "filter_empty\t0\t{n}\t{no_filter_ms:.1}\t{:.1}\t-",
        n as f64 / (no_filter_ms / 1000.0)
    );
    let _ = writeln!(
        out,
        "filter_heavy\t0\t{n}\t{heavy_filter_ms:.1}\t{:.1}\t-",
        n as f64 / (heavy_filter_ms / 1000.0)
    );
    let _ = writeln!(out, "# filter delta_ms {delta:.1} over {n} files");
    println!(
        "NOTE: replace the heavy-pattern branch with a registered Lua/native \
         FileFilter plugin (cce_plugin_runtime) to measure true FFI overhead."
    );

    // --- criterion timing for the two hashing strategies ---
    let mut criterion = criterion::Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(15))
        .warm_up_time(Duration::from_secs(3));
    let h = harness();

    criterion.bench_function("single_thread_hash", |b| {
        b.iter(|| {
            let fp = Arc::new(FileProcessor::new());
            for (p, m) in &h.paths {
                let _ = fp.process_file_with_metadata(p, &h.root, m);
            }
        })
    });
    criterion.bench_function("bounded_concurrency_hash", |b| {
        b.iter(|| {
            let fp = Arc::new(FileProcessor::new());
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_cpus::get())
                .build()
                .expect("rayon pool");
            pool.install(|| {
                use rayon::prelude::*;
                h.paths.par_iter().for_each(|(p, m)| {
                    let fp = fp.clone();
                    let _ = fp.process_file_with_metadata(p, &h.root, m);
                });
            });
        })
    });
    criterion.final_summary();

    let _ = fs::remove_dir_all(&h.root);
}
