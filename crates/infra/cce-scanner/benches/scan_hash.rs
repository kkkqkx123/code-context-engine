//! Scan + hash throughput benchmark.
//!
//! Covers the scan-and-hash benchmark need: directory scan, full hashing,
//! incremental no-change rescan, and the double-read cost (scan already
//! reads each file once; the parse stage re-reads for encoding/hash check).
//!
//! Run with: `cargo run --bench scan_hash` (debug mode is enough;
//! inputs are small and comparison-oriented).
//!
//! Results table is printed to stdout and appended to
//! `benches/results/scan_hash.tsv` for trend tracking.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cce_scanner::{FSScanner, FileProcessor, ScanOptions, compute_content_hash};

fn write_files(root: &Path, count: usize, body_lines: usize) {
    for i in 0..count {
        let dir = root.join(format!("src/mod_{:03}", i % 10));
        fs::create_dir_all(&dir).expect("create dir");
        let mut body = String::from("use std::collections::HashMap;\n");
        for l in 0..body_lines {
            body.push_str(&format!(
                "pub fn func_{i}_{l}() -> u64 {{ let x = {l} * {i}; x + 1 }}\n"
            ));
        }
        fs::write(dir.join(format!("file_{i:04}.rs")), body).expect("write file");
    }
    // One binary file and one large file to show sniff/stream branches.
    fs::write(root.join("asset.bin"), vec![0u8, 1, 2, 3, 255, 254]).expect("bin");
    let large = "x".repeat(256 * 1024);
    fs::write(root.join("large.txt"), large).expect("large");
}

fn scan_ms(root: &Path) -> (usize, f64) {
    let mut scanner = FSScanner::new();
    let opts = ScanOptions {
        root_path: root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let start = Instant::now();
    let entries = scanner.scan(&opts).expect("scan");
    (entries.len(), start.elapsed().as_secs_f64() * 1000.0)
}

fn hash_all_ms(root: &Path) -> f64 {
    let mut total = 0u64;
    let start = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("readdir") {
            let entry = entry.expect("entry");
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                let bytes = fs::read(&p).expect("read");
                let _ = compute_content_hash(&bytes);
                total += bytes.len() as u64;
            }
        }
    }
    let _ = total;
    start.elapsed().as_secs_f64() * 1000.0
}

fn double_read_ms(root: &Path) -> f64 {
    // Simulates the batch path re-reading each file for encoding/hash check
    // after the scan already touched it once.
    let processor = FileProcessor::new();
    let start = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("readdir") {
            let entry = entry.expect("entry");
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() && p.extension().map(|e| e == "rs").unwrap_or(false) {
                let _ = processor.process_file(&p, root);
                // Second touch: raw re-read as the parse stage does.
                let _ = fs::read(&p).expect("reread");
            }
        }
    }
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    println!("scan_hash benchmark (debug, small inputs)");
    println!(
        "{:<10} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "files", "scan(ms)", "rescan(ms)", "hash(ms)", "2xread(ms)", "overhead%"
    );

    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/scan_hash.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# files\tscan_ms\trescan_ms\thash_ms\tdouble_read_ms\toverhead_pct"
        );
    }

    for count in [100usize, 300] {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_files(tmp.path(), count, 20);

        let (n, scan) = scan_ms(tmp.path());
        let (_, rescan) = scan_ms(tmp.path());
        let hash = hash_all_ms(tmp.path());
        let reread = double_read_ms(tmp.path());
        let overhead = if scan > 0.0 {
            (reread - scan).max(0.0) / scan * 100.0
        } else {
            0.0
        };
        println!(
            "{count:<10} {scan:>12.2} {rescan:>12.2} {hash:>12.2} {reread:>12.2} {overhead:>12.1}"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "{count}\t{scan:.2}\t{rescan:.2}\t{hash:.2}\t{reread:.2}\t{overhead:.1}"
            );
        }
        let _ = n;
    }
}
