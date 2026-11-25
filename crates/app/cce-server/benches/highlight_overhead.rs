//! Standalone highlight-build harness.
//!
//! The service response path currently has no observable highlight stage, so
//! this benchmark isolates the three costs a real highlight implementation
//! would pay: reading the source file, cutting the displayed excerpt, and
//! building match markers. It stays out of the BM25 retrieval timing.
//!
//! Run: `cargo bench -p cce-server --bench highlight_overhead`
//! Report: `benches/results/highlight_overhead.tsv`.

use std::io::Write as _;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cce_bench_kit::{DEFAULT_LANG_DIST, ProjectSize, SyntheticProject};

#[derive(Debug, Clone, Copy)]
struct Marker {
    start: usize,
    end: usize,
    line: u32,
}

fn build_markers(text: &str, token: &str) -> Vec<Marker> {
    let mut markers = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut offset = 0;
        while let Some(found) = line[offset..].find(token) {
            let start = offset + found;
            markers.push(Marker {
                start,
                end: start + token.len(),
                line: index as u32 + 1,
            });
            offset = start + token.len();
        }
    }
    markers
}

fn main() {
    let root = tempfile::TempDir::new().expect("highlight fixture root");
    let project = SyntheticProject::generate(ProjectSize::Small, DEFAULT_LANG_DIST);
    project
        .materialize(root.path())
        .expect("materialize fixture");

    let files: Vec<_> = project.files.iter().take(200).collect();
    let rounds = 5usize;
    let mut read_ns = 0u128;
    let mut slice_ns = 0u128;
    let mut marker_ns = 0u128;
    let mut markers_total = 0usize;
    for _ in 0..rounds {
        for file in &files {
            let path = root.path().join(&file.path);
            let started = Instant::now();
            let text = std::fs::read_to_string(&path).expect("read source file");
            read_ns += started.elapsed().as_nanos();

            let started = Instant::now();
            let excerpt: String = text.lines().take(40).collect::<Vec<_>>().join("\n");
            slice_ns += started.elapsed().as_nanos();

            let started = Instant::now();
            let markers = build_markers(&excerpt, "fn_");
            marker_ns += started.elapsed().as_nanos();
            markers_total += markers.len();
            let span = markers
                .first()
                .map_or(0, |m| (m.end - m.start) + m.line as usize);
            std::hint::black_box((markers, span));
        }
    }

    let requests = files.len() * rounds;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after unix epoch")
        .as_nanos();
    let run_id = format!("{:x}", nanos & 0xffff_ffff_ffff);
    let mut out = cce_bench_kit::open_report_with_header(
        env!("CARGO_MANIFEST_DIR"),
        "highlight_overhead.tsv",
        "# run_id\trequests\tmarkers\tread_us_per_request\tslice_us_per_request\tmarker_us_per_request\ttotal_us_per_request",
    );
    let per = |total: u128| total as f64 / requests as f64 / 1000.0;
    let _ = writeln!(
        out,
        "{run_id}\t{requests}\t{markers_total}\t{:.3}\t{:.3}\t{:.3}\t{:.3}",
        per(read_ns),
        per(slice_ns),
        per(marker_ns),
        per(read_ns + slice_ns + marker_ns),
    );
    println!(
        "highlight requests={requests} read={:.3}us slice={:.3}us marker={:.3}us",
        per(read_ns),
        per(slice_ns),
        per(marker_ns)
    );
}
