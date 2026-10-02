//! Single-file parse / group / NL / chunk benchmark.
//!
//! Covers the parse-side benchmark needs: per-language single-file parse
//! throughput, cold compile versus hot cache, and group plus dual-path NL cost.
//! Small synthetic inputs only; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run --bench parse_nl_chunk`
//!
//! Results are printed to stdout and appended to
//! `benches/results/parse_nl_chunk.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_config::AstToNlConfig;
use cce_parser::ast_to_nl::{AstToNlConverter, ChunkingConfig, ConversionRequest, GroupChunker};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_types::OutputMode;

fn rust_small() -> (&'static str, String) {
    (
        "src/small.rs",
        r#"/// Adds two numbers.
pub fn add(a: u64, b: u64) -> u64 { a + b }

/// Subtracts b from a.
pub fn sub(a: u64, b: u64) -> u64 { a - b }
"#
        .to_string(),
    )
}

fn rust_large() -> (&'static str, String) {
    let mut s = String::new();
    for i in 0..150 {
        s.push_str(&format!(
            "/// Function number {i} with docs.\n\
             pub fn func_{i}(x: u64, name: &str) -> u64 {{\n  let y = x * 2 + {i};\n  y\n}}\n\n"
        ));
    }
    ("src/large.rs", s)
}

fn python_mid() -> (&'static str, String) {
    let mut s = String::new();
    for i in 0..60 {
        s.push_str(&format!(
            "def handler_{i}(request, ctx=None):\n    \"\"\"Handle request {i}.\"\"\"\n    return request.process({i})\n\n"
        ));
    }
    ("src/app.py", s)
}

fn vue_sfc() -> (&'static str, String) {
    (
        "src/comp.vue",
        r#"<template>
  <div class="app"><p v-if="ok">{{ msg }}</p><ul><li v-for="x in items">{{ x }}</li></ul></div>
</template>
<script>
export default { data() { return { msg: "hi", ok: true, items: [1,2,3] }; }, methods: { go() { this.ok = !this.ok; } } };
</script>
<style>.app { color: red; } p { margin: 0; }</style>
"#
        .to_string(),
    )
}

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    // Warm once, then average.
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn main() {
    println!("parse_nl_chunk benchmark (debug, small inputs)");
    println!(
        "{:<14} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "sample", "parse", "cold", "group", "bm25", "embed", "both", "chunk"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/parse_nl_chunk.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(
            f,
            "# sample\tparse_ms\tcold_ms\tgroup_ms\tbm25_ms\tembed_ms\tboth_ms\tchunk_ms"
        );
    }

    let samples: Vec<(&str, String)> = vec![rust_small(), rust_large(), python_mid(), vue_sfc()];
    for (path, content) in &samples {
        // Hot parse: reuse coordinator (query cache warm).
        let mut coord = ParseCoordinator::new();
        let parsed = coord.parse(path, content).expect("parse");
        let parse_ms = bench_ms(3, || {
            let _ = coord.parse(path, content).expect("parse");
        });
        // Cold parse: fresh coordinator each iteration (grammar/query re-init).
        let cold_ms = bench_ms(3, || {
            let mut c = ParseCoordinator::new();
            let _ = c.parse(path, content).expect("parse");
        });

        let pipeline = PreprocessingPipeline::new();
        let processing = pipeline.process(&parsed);
        let groups = &processing.groups;
        let group_ms = bench_ms(5, || {
            let _ = pipeline.process(&parsed);
        });

        let bm25_conv = AstToNlConverter::with_config(&AstToNlConfig {
            default_mode: OutputMode::Bm25,
            ..Default::default()
        });
        let embed_conv = AstToNlConverter::with_config(&AstToNlConfig {
            default_mode: OutputMode::Embedding,
            ..Default::default()
        });
        let both_conv = AstToNlConverter::with_config(&AstToNlConfig {
            default_mode: OutputMode::Both,
            ..Default::default()
        });
        let bm25_req = ConversionRequest {
            force_mode: Some(OutputMode::Bm25),
        };
        let embed_req = ConversionRequest {
            force_mode: Some(OutputMode::Embedding),
        };
        let both_req = ConversionRequest {
            force_mode: Some(OutputMode::Both),
        };
        let bm25_ms = bench_ms(3, || {
            let _ = bm25_conv.convert_entity_groups(groups, path, Some(&bm25_req), None, None);
        });
        let embed_ms = bench_ms(3, || {
            let _ = embed_conv.convert_entity_groups(groups, path, Some(&embed_req), None, None);
        });
        let both_ms = bench_ms(3, || {
            let _ = both_conv.convert_entity_groups(groups, path, Some(&both_req), None, None);
        });

        // Chunk the both-path conversions of the first group (or all groups).
        let conversions =
            both_conv.convert_entity_groups(groups, path, Some(&both_req), None, None);
        let config = ChunkingConfig::default();
        let chunk_ms = bench_ms(3, || {
            let mut chunker = GroupChunker::new(config.clone());
            for (g, gc) in groups.iter().zip(conversions.iter()) {
                if let Some(header) = gc.header_conversion.as_ref() {
                    let _ = chunker.chunk_group(g, header, path);
                }
                for m in &gc.member_conversions {
                    let _ = chunker.chunk_group(g, m, path);
                }
            }
        });

        let label = PathBuf::from(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        println!(
            "{label:<14} {parse_ms:>10.2} {cold_ms:>10.2} {group_ms:>10.2} {bm25_ms:>10.2} {embed_ms:>10.2} {both_ms:>10.2} {chunk_ms:>10.2}"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "{label}\t{parse_ms:.2}\t{cold_ms:.2}\t{group_ms:.2}\t{bm25_ms:.2}\t{embed_ms:.2}\t{both_ms:.2}\t{chunk_ms:.2}"
            );
        }
    }
}
