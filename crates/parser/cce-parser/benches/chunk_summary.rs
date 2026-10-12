//! Chunking and summary benchmark (§3.5).
//!
//! Quantifies recursive splitting, cost re-estimation and rule-summary
//! grouping on long, over-limit and nested inputs, plus the payoff of the
//! reusable summary entry (`generate_sync_with_result` with a precomputed
//! `ProcessingResult`) over the default path that rebuilds preprocessing.
//! Local data only; comparisons matter, not absolute numbers.
//!
//! Run with: `cargo run -p cce-parser --bench chunk_summary`
//!
//! Results are printed to stdout and appended to
//! `benches/results/chunk_summary.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_config::AstToNlConfig;
use cce_parser::ast_to_nl::{AstToNlConverter, ChunkingConfig, ConversionRequest, GroupChunker};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_parser::summary::RuleBasedGenerator;
use cce_types::OutputMode;

fn long_group() -> (&'static str, String) {
    let mut s = String::new();
    for i in 0..200 {
        s.push_str(&format!(
            "/// Handler number {i}.\npub fn handler_{i}(x: u64) -> u64 {{\n  let y = x + {i};\n  y * 2\n}}\n\n"
        ));
    }
    ("src/long_group.rs", s)
}

fn nested_group() -> (&'static str, String) {
    let mut s = String::from("pub mod outer {\n    pub mod inner {\n");
    for i in 0..60 {
        s.push_str(&format!(
            "        pub struct Item{i} {{ pub field: u64 }}\n        impl Item{i} {{\n            pub fn get(&self) -> u64 {{ self.field + {i} }}\n            pub fn set(&mut self, v: u64) {{ self.field = v; }}\n        }}\n"
        ));
    }
    s.push_str("    }\n}\n");
    ("src/nested.rs", s)
}

fn over_limit_body() -> (&'static str, String) {
    let mut s = String::from("pub fn giant() -> u64 {\n    let mut acc = 0u64;\n");
    for i in 0..800 {
        s.push_str(&format!("    acc += {i} * 3;\n"));
    }
    s.push_str("    acc\n}\n");
    ("src/giant.rs", s)
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
    println!("chunk_summary benchmark (debug, synthetic inputs)");
    println!(
        "{:<14} {:>12} {:>12} {:>12} {:>10}",
        "sample", "summary", "reuse", "chunk", "groups"
    );

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/chunk_summary.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# sample\tsummary_ms\treuse_ms\tchunk_ms\tgroups");
    }

    let samples = vec![long_group(), nested_group(), over_limit_body()];
    for (path, content) in &samples {
        let mut coord = ParseCoordinator::new();
        let parsed = coord.parse(path, content).expect("parse");
        let pipeline = PreprocessingPipeline::new();
        let processing = pipeline.process(&parsed);
        let group_count = processing.groups.len();

        let generator = RuleBasedGenerator::new();
        let summary_ms = bench_ms(3, || {
            let _ = generator.generate_sync(&parsed);
        });
        let reuse_ms = bench_ms(5, || {
            let _ = generator.generate_sync_with_result(&parsed, Some(&processing));
        });

        let converter = AstToNlConverter::with_config(&AstToNlConfig {
            default_mode: OutputMode::Both,
            ..Default::default()
        });
        let request = ConversionRequest {
            force_mode: Some(OutputMode::Both),
        };
        let conversions =
            converter.convert_entity_groups(&processing.groups, path, Some(&request), None, None);
        let config = ChunkingConfig::default();
        let chunk_ms = bench_ms(3, || {
            let mut chunker = GroupChunker::new(config.clone());
            for (g, gc) in processing.groups.iter().zip(conversions.iter()) {
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
            "{label:<14} {summary_ms:>12.2} {reuse_ms:>12.2} {chunk_ms:>12.2} {group_count:>10}"
        );
        if let Some(f) = out.as_mut() {
            let _ = writeln!(
                f,
                "{label}\t{summary_ms:.2}\t{reuse_ms:.2}\t{chunk_ms:.2}\t{group_count}"
            );
        }
    }
}
