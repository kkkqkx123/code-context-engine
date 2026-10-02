//! Inline-query compile and embedded re-parse amplification benchmark.
//!
//! Covers two unreproduced risks with production code: cold versus warm
//! query-compile cost per language (including the single-file style inline
//! query branch, whose first call compiles and later calls hit the cache),
//! and embedded secondary-parse amplification as the block count grows
//! (styled-components blocks per file, plus group/chunk scaling that shows
//! the main-parse versus chunking share).
//!
//! Run with: `cargo run -p cce-parser --bench inline_embedded`
//!
//! Results are printed to stdout and appended to
//! `benches/results/inline_embedded.tsv`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use cce_parser::ast_to_nl::{AstToNlConverter, ChunkingConfig, ConversionRequest, GroupChunker};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_config::AstToNlConfig;
use cce_types::OutputMode;

fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

fn rust_src(fns: usize) -> String {
    let mut s = String::new();
    for i in 0..fns {
        s.push_str(&format!(
            "/// Function {i}.\npub fn func_{i}(x: u64) -> u64 {{ x * 2 + {i} }}\n\n"
        ));
    }
    s
}

fn python_src(fns: usize) -> String {
    let mut s = String::new();
    for i in 0..fns {
        s.push_str(&format!(
            "def handler_{i}(request, ctx=None):\n    \"\"\"Handle {i}.\"\"\"\n    return request.process({i})\n\n"
        ));
    }
    s
}

fn js_src(fns: usize) -> String {
    let mut s = String::new();
    for i in 0..fns {
        s.push_str(&format!(
            "function handler_{i}(req) {{ return req * {i}; }}\nmodule.exports.h{i} = handler_{i};\n"
        ));
    }
    s
}

fn styled_src(blocks: usize) -> String {
    let mut s = String::from("import styled from \"styled-components\";\n");
    for i in 0..blocks {
        s.push_str(&format!(
            "const C{i} = styled.button`\n  color: red;\n  margin: {i}px;\n`;\n"
        ));
    }
    s
}

fn main() {
    println!("inline_embedded benchmark (debug, synthetic inputs)");
    println!("{:<30} {:>12} {:>12}", "case", "ms", "extra");

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/results/inline_embedded.tsv"))
        .ok();
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "# case\tms\textra");
    }
    let mut row = |label: &str, ms: f64, extra: &str| {
        println!("{label:<30} {ms:>12.2} {extra:>12}");
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{label}\t{ms:.2}\t{extra}");
        }
    };

    // Cold (fresh coordinator: grammar plus query init) versus warm
    // (reused coordinator: cache hit) per language.
    let samples = [
        ("rust", "src/a.rs", rust_src(20)),
        ("python", "src/a.py", python_src(20)),
        ("javascript", "src/a.js", js_src(20)),
    ];
    for (lang, path, content) in &samples {
        let cold_ms = bench_ms(3, || {
            let mut coord = ParseCoordinator::new();
            let _ = coord.parse(path, content).expect("parse");
        });
        let mut coord = ParseCoordinator::new();
        let warm_ms = bench_ms(5, || {
            let _ = coord.parse(path, content).expect("parse");
        });
        row(
            &format!("{lang} cold parse"),
            cold_ms,
            &format!("x{warm_ms:.2}"),
        );
        row(&format!("{lang} warm parse"), warm_ms, "cached");
    }

    // Single-file style inline query branch: first extraction compiles the
    // query, later ones reuse it; files without markers skip entirely.
    {
        use cce_parser::parser::AstParser;
        use cce_parser::parser::extractor::EmbeddedParser;
        use cce_types::language::Language;

        for blocks in [1usize, 5, 20] {
            let src = styled_src(blocks);
            let mut ast = AstParser::new();
            let embedded = EmbeddedParser::new();
            let (tree, _) = ast
                .parse_with_tree(&src, &Language::JavaScript)
                .expect("parse styled js");
            let cold_start = Instant::now();
            let found = embedded
                .extract_css_in_js(&tree, &src, &Language::JavaScript)
                .expect("cold extract")
                .len();
            let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
            let warm_ms = bench_ms(5, || {
                let _ = embedded
                    .extract_css_in_js(&tree, &src, &Language::JavaScript)
                    .expect("warm extract");
            });
            row(
                &format!("styled extract blocks={blocks} cold"),
                cold_ms,
                &format!("found={found}"),
            );
            row(
                &format!("styled extract blocks={blocks} warm"),
                warm_ms,
                &format!("per-block={:.3}", warm_ms / blocks as f64),
            );
        }
        let plain = "function add(a, b) { return a + b; }\nmodule.exports = { add };\n";
        let mut ast = AstParser::new();
        let embedded = EmbeddedParser::new();
        let (plain_tree, _) = ast
            .parse_with_tree(plain, &Language::JavaScript)
            .expect("parse plain js");
        let skip_ms = bench_ms(5, || {
            let blocks = embedded
                .extract_css_in_js(&plain_tree, plain, &Language::JavaScript)
                .expect("skip");
            assert!(blocks.is_empty());
        });
        row("styled extract skip-plain", skip_ms, "no markers");
    }

    // Main-parse versus chunking share as the group count grows.
    let converter = AstToNlConverter::with_config(&AstToNlConfig {
        default_mode: OutputMode::Both,
        ..Default::default()
    });
    let pipeline = PreprocessingPipeline::new();
    let chunk_config = ChunkingConfig::default();
    for fns in [10usize, 50, 150] {
        let content = rust_src(fns);
        let mut coord = ParseCoordinator::new();
        let parse_ms = bench_ms(3, || {
            let _ = coord.parse("src/grow.rs", &content).expect("parse");
        });
        let parsed = coord.parse("src/grow.rs", &content).expect("parse");
        let processing = pipeline.process(&parsed);
        let req = ConversionRequest {
            force_mode: Some(OutputMode::Both),
        };
        let conversions =
            converter.convert_entity_groups(&processing.groups, "src/grow.rs", Some(&req), None, None);
        let chunk_ms = bench_ms(3, || {
            let mut chunker = GroupChunker::new(chunk_config.clone());
            for (group, conversion) in processing.groups.iter().zip(conversions.iter()) {
                if let Some(header) = conversion.header_conversion.as_ref() {
                    let _ = chunker.chunk_group(group, header, "src/grow.rs");
                }
                for member in &conversion.member_conversions {
                    let _ = chunker.chunk_group(group, member, "src/grow.rs");
                }
            }
        });
        let share = if parse_ms > 0.0 {
            chunk_ms / parse_ms * 100.0
        } else {
            0.0
        };
        row(
            &format!("rust fns={fns} parse"),
            parse_ms,
            &format!("groups={}", processing.groups.len()),
        );
        row(
            &format!("rust fns={fns} chunk"),
            chunk_ms,
            &format!("{share:.0}% of parse"),
        );
    }

    // Rule summary scaling: default summary path cost versus entity count.
    {
        use cce_parser::summary::RuleBasedGenerator;
        let generator = RuleBasedGenerator::default();
        for fns in [10usize, 50, 150] {
            let content = rust_src(fns);
            let mut coord = ParseCoordinator::new();
            let parsed = coord.parse("src/sum.rs", &content).expect("parse");
            let summary_ms = bench_ms(5, || {
                let _ = generator.generate_sync(&parsed);
            });
            row(
                &format!("rule summary fns={fns}"),
                summary_ms,
                &format!("ents={}", parsed.entities.len()),
            );
        }
    }
}
