//! Batch concurrency / batch-size combination benchmark.
//!
//! Covers the batch-concurrency benchmark need: how parse-group-convert-chunk
//! throughput responds to the joint choice of batch size and per-file
//! concurrency, so a recommended default interval can be read off a table
//! instead of guessed.
//!
//! Production runs sequential batches with bounded per-file concurrency
//! (tokio semaphore, one task per file). This bench isolates the pipeline
//! scaling itself from executor overhead by splitting each batch across a
//! fixed number of worker threads (each with its own warm parser
//! coordinator); the sweep dimensions and the measured quantities are the
//! same ones the production knobs control.
//!
//! Run with: `cargo run -p cce-orchestrator --bench batch_concurrency`
//!
//! Results are printed to stdout and appended to
//! `benches/results/batch_concurrency.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_config::AstToNlConfig;
use cce_parser::ast_to_nl::{AstToNlConverter, ChunkingConfig, ConversionRequest, GroupChunker};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_types::OutputMode;

const FILE_COUNT: usize = 120;
const FUNCTIONS_PER_FILE: usize = 20;

fn make_file(i: usize) -> (String, String) {
    let mut body = String::new();
    for f in 0..FUNCTIONS_PER_FILE {
        body.push_str(&format!(
            "/// Function {f} of file {i}.\n\
             pub fn func_{i}_{f}(x: u64, name: &str) -> u64 {{\n  let y = x * 2 + {f};\n  y\n}}\n\n"
        ));
    }
    (format!("src/mod_{i:03}.rs"), body)
}

/// Full per-file pipeline: parse, group, dual-path NL conversion, chunking.
/// Returns the number of chunks produced. One warm coordinator per worker.
fn process_file(
    coord: &mut ParseCoordinator,
    converter: &AstToNlConverter,
    pipeline: &PreprocessingPipeline,
    chunk_config: &ChunkingConfig,
    path: &str,
    content: &str,
) -> usize {
    let parsed = coord.parse(path, content).expect("parse");
    let processing = pipeline.process(&parsed);
    let req = ConversionRequest {
        force_mode: Some(OutputMode::Both),
    };
    let conversions =
        converter.convert_entity_groups(&processing.groups, path, Some(&req), None, None);
    let mut chunker = GroupChunker::new(chunk_config.clone());
    let mut chunks = 0;
    for (group, conversion) in processing.groups.iter().zip(conversions.iter()) {
        if let Some(header) = conversion.header_conversion.as_ref() {
            if let Ok(out) = chunker.chunk_group(group, header, path) {
                chunks += out.chunks.len();
            }
        }
        for member in &conversion.member_conversions {
            if let Ok(out) = chunker.chunk_group(group, member, path) {
                chunks += out.chunks.len();
            }
        }
    }
    chunks
}

/// Run all files in batches of `batch_size`, each batch spread over
/// `concurrency` worker threads. Returns wall milliseconds and total chunks.
fn run_sweep(files: &[(String, String)], batch_size: usize, concurrency: usize) -> (f64, usize) {
    let converter = AstToNlConverter::with_config(&AstToNlConfig {
        default_mode: OutputMode::Both,
        ..Default::default()
    });
    let chunk_config = ChunkingConfig::default();
    let start = Instant::now();
    let mut total_chunks = 0;
    for batch in files.chunks(batch_size) {
        let workers = concurrency.min(batch.len()).max(1);
        let mut per_worker: Vec<usize> = vec![0; workers];
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for (slot, group) in batch.chunks(group_size(batch.len(), workers)).enumerate() {
                let converter = &converter;
                let chunk_config = &chunk_config;
                handles.push(scope.spawn(move || {
                    let pipeline = PreprocessingPipeline::new();
                    let mut coord = ParseCoordinator::new();
                    let mut chunks = 0;
                    for (path, content) in group {
                        chunks += process_file(
                            &mut coord,
                            converter,
                            &pipeline,
                            chunk_config,
                            path,
                            content,
                        );
                    }
                    (slot, chunks)
                }));
            }
            for handle in handles {
                let (slot, chunks) = handle.join().expect("worker");
                per_worker[slot] = chunks;
            }
        });
        total_chunks += per_worker.iter().sum::<usize>();
    }
    (start.elapsed().as_secs_f64() * 1000.0, total_chunks)
}

fn group_size(batch_len: usize, workers: usize) -> usize {
    batch_len.div_ceil(workers).max(1)
}

fn main() {
    println!("batch_concurrency benchmark (debug, {FILE_COUNT} files x {FUNCTIONS_PER_FILE} fns)");
    println!(
        "{:<10} {:<12} {:>12} {:>12} {:>12}",
        "batch", "workers", "ms", "files/sec", "chunks"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/batch_concurrency.tsv"),
        )
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# batch\tworkers\tms\tfiles_per_sec\tchunks");
    }

    let files: Vec<(String, String)> = (0..FILE_COUNT).map(make_file).collect();

    // Warm-up so tree-sitter grammars and query caches are steady-state.
    let _ = run_sweep(&files[..8], 8, 1);

    // Baseline: fully sequential, one batch.
    let (base_ms, base_chunks) = run_sweep(&files, FILE_COUNT, 1);

    for batch_size in [25usize, 50, 100] {
        for workers in [1usize, 5, 10, 20] {
            let (ms, chunks) = run_sweep(&files, batch_size, workers);
            let rate = FILE_COUNT as f64 / (ms / 1000.0);
            println!("{batch_size:<10} {workers:<12} {ms:>12.2} {rate:>12.1} {chunks:>12}");
            if let Some(f) = out.as_mut() {
                let _ = writeln!(f, "{batch_size}\t{workers}\t{ms:.2}\t{rate:.1}\t{chunks}");
            }
        }
    }
    let base_rate = FILE_COUNT as f64 / (base_ms / 1000.0);
    println!(
        "baseline sequential: {base_ms:.2} ms ({base_rate:.1} files/sec, {base_chunks} chunks)"
    );
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# baseline\t1\t{base_ms:.2}\t{base_rate:.1}\t{base_chunks}"
        );
    }
}
