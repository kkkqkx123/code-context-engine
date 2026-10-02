//! Change-detection polling benchmark.
//!
//! Covers the change-detection benchmark need: polling cost at different
//! change rates. A fixed tree is scanned once for the baseline map, then
//! zero, few, or many files are modified (content appended, so the size
//! half of the size-plus-mtime fingerprint always trips) and the
//! incremental rescan is timed. The reused-hash count separates directory
//! traversal cost from re-hash cost.
//!
//! Run with: `cargo run -p cce-scanner --bench poll_rates`
//!
//! Results are printed to stdout and appended to
//! `benches/results/poll_rates.tsv`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cce_scanner::{FSScanner, FileEntry, ScanOptions};

const FILE_COUNT: usize = 300;

fn write_files(root: &Path, count: usize) {
    for i in 0..count {
        let dir = root.join(format!("src/mod_{:03}", i % 10));
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            dir.join(format!("file_{i:04}.rs")),
            format!("pub fn func_{i}(x: u64) -> u64 {{ x * 2 + {i} }}\n"),
        )
        .expect("write file");
    }
}

fn opts(root: &Path) -> ScanOptions {
    ScanOptions {
        root_path: root.to_string_lossy().into_owned(),
        ..Default::default()
    }
}

/// Append a line to the first `changed` files so their size changes and
/// the incremental scan must re-hash exactly those files.
fn touch_files(root: &Path, changed: usize) {
    for i in 0..changed {
        let path = root.join(format!("src/mod_{:03}/file_{i:04}.rs", i % 10));
        let mut content = std::fs::read_to_string(&path).expect("read for touch");
        content.push_str(&format!("// touched {i}\n"));
        std::fs::write(&path, content).expect("touch file");
    }
}

fn main() {
    println!("poll_rates benchmark (debug, {FILE_COUNT} files)");
    println!(
        "{:<16} {:>12} {:>12} {:>12}",
        "changed", "ms", "files", "reused%"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/poll_rates.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# changed\tms\tfiles\treused_pct");
    }

    for changed in [0usize, 5, 50, 150] {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_files(tmp.path(), FILE_COUNT);
        let options = opts(tmp.path());

        let mut scanner = FSScanner::new();
        let start = Instant::now();
        let first = scanner.scan(&options).expect("full scan");
        let full_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(first.len(), FILE_COUNT);
        let previous: HashMap<PathBuf, FileEntry> = first
            .iter()
            .map(|entry| (entry.relative_path.clone(), entry.clone()))
            .collect();

        touch_files(tmp.path(), changed);

        let start = Instant::now();
        let second = scanner
            .scan_incremental(&options, &previous)
            .expect("incremental scan");
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(second.len(), FILE_COUNT);

        // Reuse rate: entries whose content hash survived untouched.
        let reused = second
            .iter()
            .filter(|entry| {
                previous
                    .get(&entry.relative_path)
                    .and_then(|old| old.content_hash.clone())
                    .is_some_and(|old| Some(old) == entry.content_hash)
            })
            .count();
        let reused_pct = reused as f64 / FILE_COUNT as f64 * 100.0;
        println!("{changed:<16} {ms:>12.2} {FILE_COUNT:>12} {reused_pct:>12.1}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{changed}\t{ms:.2}\t{FILE_COUNT}\t{reused_pct:.1}");
        }
        if changed == 0 {
            println!("  full-scan baseline for comparison: {full_ms:.2} ms");
            if let Some(f) = out.as_mut() {
                let _ = writeln!(f, "# full_scan_baseline_ms={full_ms:.2}");
            }
        }
    }
}
