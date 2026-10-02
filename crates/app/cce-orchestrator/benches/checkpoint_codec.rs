//! Checkpoint codec benchmark.
//!
//! Covers the checkpoint-codec benchmark need: parse-artifact serialize plus
//! compress and deserialize throughput, and the recovery decode prelude. Small
//! large real `ParsedFile` payloads; the summary second-write is contrasted
//! against the parse-blob write. No DB I/O in the hot loop — pure codec cost.
//!
//! Run with: `cargo run --bench checkpoint_codec`
//!
//! Results are printed to stdout and appended to
//! `benches/results/checkpoint_codec.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_orchestrator::hot_update::FileChangeType;
use cce_orchestrator::operation::checkpoint::{
    ParsedCheckpointEnvelope, ParsedCheckpointPayload, decode_parsed_checkpoint,
    encode_parsed_checkpoint,
};
use cce_parser::parser::ParseCoordinator;

fn rust_small() -> (&'static str, String) {
    (
        "src/small.rs",
        "/// Adds two numbers.\npub fn add(a: u64, b: u64) -> u64 { a + b }\n".to_string(),
    )
}

fn rust_large() -> (&'static str, String) {
    let mut s = String::new();
    for i in 0..150 {
        s.push_str(&format!(
            "/// Function {i}.\n\
             pub fn func_{i}(x: u64, name: &str) -> u64 {{\n  let y = x * 2 + {i};\n  y\n}}\n\n"
        ));
    }
    ("src/large.rs", s)
}

fn payload_for(path: &str, content: &str) -> ParsedCheckpointPayload {
    let mut coord = ParseCoordinator::new();
    let parsed = coord.parse(path, content).expect("parse");
    ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
        FileChangeType::Modified,
        parsed,
    )))
}

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("checkpoint_codec benchmark (debug, real ParsedFile payloads)");
    println!(
        "{:<12} {:>12} {:>12} {:>14} {:>12}",
        "sample", "encode", "decode", "bytes", "ratio%"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/checkpoint_codec.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# sample\tencode_ms\tdecode_ms\tbytes\tjson_bytes\tratio_pct"
        );
    }

    for (path, content) in [rust_small(), rust_large()] {
        let payload = payload_for(path, &content);
        let json_len = serde_json::to_vec(&payload).expect("json").len();
        let mut bytes = 0;
        let encode_ms = bench_ms(5, || {
            let b = encode_parsed_checkpoint(&payload).expect("encode");
            bytes = b.len();
        });
        let encoded = encode_parsed_checkpoint(&payload).expect("encode");
        bytes = encoded.len();
        let decode_ms = bench_ms(5, || {
            let _ = decode_parsed_checkpoint(&encoded).expect("decode none");
        });
        let ratio = if json_len > 0 {
            bytes as f64 / json_len as f64 * 100.0
        } else {
            0.0
        };
        let label = PathBuf::from(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        println!("{label:<12} {encode_ms:>12.2} {decode_ms:>12.2} {bytes:>14} {ratio:>12.1}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "{label}\t{encode_ms:.2}\t{decode_ms:.2}\t{bytes}\t{json_len}\t{ratio:.1}"
            );
        }

        // Tombstone contrast: Deleted carries no source, must stay ~free.
        let tombstone = ParsedCheckpointPayload::Deleted;
        let tomb_ms = bench_ms(20, || {
            let b = encode_parsed_checkpoint(&tombstone).expect("encode");
            let _ = decode_parsed_checkpoint(&b);
        });
        println!("deleted      {tomb_ms:>12.2}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "deleted-{label}\t{tomb_ms:.2}\t0\t0\t0\t0");
        }
    }
}
