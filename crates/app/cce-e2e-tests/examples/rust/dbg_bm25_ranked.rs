//! BM25 term-level ranked diagnostic for ripgrep text-gap queries.
//!
//! For each target query and each chunking baseline, prints the expanded
//! query terms, the top-15 ranked chunks with scores, the truth-covering
//! chunk's rank/score, and per-term tf inside the truth chunk per field.
//! Terminology: same three-field weighted model as the offline benchmark.
//!
//! Usage:
//! ```shell
//! cargo run --example dbg_bm25_ranked -p cce-e2e-tests
//! ```
//! Output: `outputs/debug/bm25_ranked/{baseline}__{query}.txt` (review only,
//! no assertions; needs `gen_bench_ripgrep` data present).

use cce_e2e_tests::bench_data::{Bm25DocRecord, load_benchmark_data, production_bm25_config};
use cce_e2e_tests::infra::{self, Field};
use cce_e2e_tests::judgments::ripgrep_relevance_judgments;
use cce_e2e_tests::range_evaluator::{RankedScan, RetrievedCoverage, coverage_over_retrieved};
use cce_storage_bm25::TermOperator;

struct Target {
    query_id: &'static str,
    query_text: &'static str,
    file: &'static str,
    start: usize,
    end: usize,
}

const TARGETS: &[Target] = &[
    Target {
        query_id: "G1Q1",
        query_text: "find_at in RegexMatcher",
        file: "crates/regex/src/matcher.rs",
        start: 409,
        end: 421,
    },
    Target {
        query_id: "G1Q9",
        query_text: "build method on Pcre2Matcher",
        file: "crates/pcre2/src/matcher.rs",
        start: 37,
        end: 39,
    },
    Target {
        query_id: "G2Q3",
        query_text: "check if file path matches glob pattern",
        file: "crates/globset/src/lib.rs",
        start: 342,
        end: 344,
    },
    Target {
        query_id: "G2Q8",
        query_text: "read file incrementally with bounded memory line buffer",
        file: "crates/searcher/src/line_buffer.rs",
        start: 294,
        end: 323,
    },
    Target {
        query_id: "G2Q20",
        query_text: "search stdin stream without file path",
        file: "crates/searcher/src/searcher/mod.rs",
        start: 727,
        end: 765,
    },
    Target {
        query_id: "G2Q4",
        query_text: "extract matched results with surrounding context lines",
        file: "crates/searcher/src/sink.rs",
        start: 142,
        end: 149,
    },
    Target {
        query_id: "G2Q14",
        query_text: "parse gitignore file format with comments and negation",
        file: "crates/ignore/src/gitignore.rs",
        start: 385,
        end: 421,
    },
    Target {
        query_id: "G2Q11",
        query_text: "compile PCRE2 pattern with advanced flags for lookaround",
        file: "crates/pcre2/src/matcher.rs",
        start: 37,
        end: 85,
    },
    Target {
        query_id: "G2Q16",
        query_text: "extract literal strings from regex for optimization",
        file: "crates/regex/src/literal.rs",
        start: 54,
        end: 70,
    },
    Target {
        query_id: "G2Q4",
        query_text: "extract matched results with surrounding context lines",
        file: "crates/searcher/src/searcher/mod.rs",
        start: 399,
        end: 419,
    },
    Target {
        query_id: "G2Q5",
        query_text: "compile regular expression with case insensitive flag",
        file: "crates/regex/src/matcher.rs",
        start: 103,
        end: 106,
    },
    Target {
        query_id: "G2Q1",
        query_text: "search for text pattern in files using regular expression",
        file: "crates/regex/src/matcher.rs",
        start: 409,
        end: 421,
    },
    Target {
        query_id: "G2Q6",
        query_text: "match patterns that span across multiple lines",
        file: "crates/searcher/src/searcher/mod.rs",
        start: 373,
        end: 391,
    },
    Target {
        query_id: "G2Q7",
        query_text: "use memory mapped file for efficient search performance",
        file: "crates/searcher/src/searcher/mmap.rs",
        start: 13,
        end: 25,
    },
    Target {
        query_id: "G2Q10",
        query_text: "filter files by extension or type like rust or python",
        file: "crates/ignore/src/types.rs",
        start: 248,
        end: 289,
    },
    Target {
        query_id: "G2Q12",
        query_text: "format search output with line numbers and file paths",
        file: "crates/printer/src/standard.rs",
        start: 1172,
        end: 1189,
    },
    Target {
        query_id: "G2Q15",
        query_text: "handle search errors gracefully with unified error type",
        file: "crates/searcher/src/searcher/mod.rs",
        start: 238,
        end: 262,
    },
    Target {
        query_id: "G2Q9",
        query_text: "detect and handle binary files during search",
        file: "crates/searcher/src/line_buffer.rs",
        start: 42,
        end: 64,
    },
    Target {
        query_id: "G2Q18",
        query_text: "override ignore rules with include or exclude globs",
        file: "crates/ignore/src/overrides.rs",
        start: 119,
        end: 134,
    },
    Target {
        query_id: "FZ-G1Q9-synonym",
        query_text: "compile method on Pcre2Matcher",
        file: "crates/pcre2/src/matcher.rs",
        start: 37,
        end: 39,
    },
    Target {
        query_id: "G2Q13",
        query_text: "parallel directory traversal using multiple threads",
        file: "crates/ignore/src/walk.rs",
        start: 1306,
        end: 1325,
    },
    Target {
        query_id: "G2Q13",
        query_text: "parallel directory traversal using multiple threads",
        file: "crates/ignore/src/walk.rs",
        start: 1355,
        end: 1365,
    },
];

const BASELINES: &[&str] = &[
    "full_pipeline",
    "full_pipeline_raw_source",
    "direct_chunking",
];

/// Dump every BM25 chunk of one file: id, span, text length, text head.
/// Usage: `cargo run --example dbg_bm25_ranked -p cce-e2e-tests -- dump <baseline> <file-suffix>`
/// Output: `outputs/debug/bm25_ranked/DUMP_{baseline}__{stem}.txt`.
fn dump_file_chunks(baseline: &str, file_suffix: &str) {
    let out_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join("debug")
        .join("bm25_ranked");
    std::fs::create_dir_all(&out_root).expect("create debug output dir");
    let bench = load_benchmark_data(&data_path(baseline)).expect("load bench data");
    let mut out = String::new();
    for (i, record) in bench.bm25_documents.iter().enumerate() {
        let chunk = &bench.bm25.chunks[i];
        if !chunk.file_path.ends_with(file_suffix) {
            continue;
        }
        let head: String = record.content.chars().take(300).collect();
        out.push_str(&format!(
            "--- {} {}:{}-{} title={:?} keywords={:?} content_len={}\n{}\n",
            chunk.chunk_id,
            chunk.file_path,
            chunk.start_line,
            chunk.end_line,
            record.title,
            record.keywords,
            record.content.len(),
            head,
        ));
    }
    let stem = file_suffix.replace('/', "_");
    std::fs::write(out_root.join(format!("DUMP_{baseline}__{stem}.txt")), &out)
        .expect("write dump");
    eprintln!("WROTE DUMP {baseline} {file_suffix}");
}

/// Print full content + keywords of every BM25 chunk whose span overlaps
/// [start, end] in the given file.
/// Usage: `cargo run --example dbg_bm25_ranked -p cce-e2e-tests -- show <baseline> <file-suffix> <start> <end>`
fn show_covering_chunks(baseline: &str, file_suffix: &str, start: usize, end: usize) {
    let bench = load_benchmark_data(&data_path(baseline)).expect("load bench data");
    for (i, record) in bench.bm25_documents.iter().enumerate() {
        let chunk = &bench.bm25.chunks[i];
        if !chunk.file_path.ends_with(file_suffix) {
            continue;
        }
        if chunk.start_line > end || chunk.end_line < start {
            continue;
        }
        println!(
            "=== {} {}:{}-{} title={:?} keywords={:?} content_len={} ===\n{}\n",
            chunk.chunk_id,
            chunk.file_path,
            chunk.start_line,
            chunk.end_line,
            record.title,
            record.keywords,
            record.content.len(),
            record.content,
        );
    }
}

fn data_path(baseline: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("benchmark")
        .join(baseline)
        .join("ripgrep")
        .join("bge-m3")
        .join("bench_data.rkyv")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 && args[1] == "dump" {
        dump_file_chunks(&args[2], &args[3]);
        return;
    }
    if args.len() == 6 && args[1] == "show" {
        let start: usize = args[4].parse().expect("start line");
        let end: usize = args[5].parse().expect("end line");
        show_covering_chunks(&args[2], &args[3], start, end);
        return;
    }
    let out_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join("debug")
        .join("bm25_ranked");
    std::fs::create_dir_all(&out_root).expect("create debug output dir");

    for baseline in BASELINES {
        let path = data_path(baseline);
        if !path.exists() {
            eprintln!("SKIP {baseline}: no bench data");
            continue;
        }
        let bench = load_benchmark_data(&path).expect("load bench data");
        let docs: &[Bm25DocRecord] = &bench.bm25_documents;
        let bm25_docs: Vec<cce_storage_common::FulltextDocument> = docs
            .iter()
            .enumerate()
            .map(|(i, record)| {
                cce_storage_common::FulltextDocument::new(format!("doc:{i}"))
                    .with_field("title", &record.title)
                    .with_field("keywords", &record.keywords)
                    .with_field("content", &record.content)
            })
            .collect();
        let term_index = infra::build_term_index(&bm25_docs);
        let config = production_bm25_config();

        for target in TARGETS {
            let judgments = ripgrep_relevance_judgments();
            let judgment = judgments
                .iter()
                .find(|j| j.id == target.query_id)
                .expect("judgment present");
            let q_idx = bench
                .queries
                .iter()
                .position(|q| q.id == target.query_id)
                .expect("query present");
            // Score exactly what the benchmark scores: the persisted bench query text.
            let query = infra::expand_query(&bench.query_texts[q_idx], &term_index);
            let ranked_all = infra::score_all(
                &term_index,
                std::slice::from_ref(&query),
                &config,
                TermOperator::Or,
                docs.len(),
            );
            let ranked = ranked_all.into_iter().next().unwrap_or_default();

            let mut out = String::new();
            out.push_str(&format!(
                "baseline={baseline} query={} \"{}\"\ntruth={}:{}-{}\n",
                target.query_id, target.query_text, target.file, target.start, target.end
            ));
            out.push_str("expanded terms (text x scale):\n");
            for term in &query.terms {
                let df = term_index.term_doc_freq(&term.text);
                out.push_str(&format!(
                    "  {:28} scale={:.3} df={}\n",
                    term.text, term.scale, df
                ));
            }
            out.push_str("top-15 (rank score chunk_id file:lines):\n");
            for (rank, (doc_idx, score)) in ranked.iter().take(15).enumerate() {
                let chunk = &bench.bm25.chunks[*doc_idx];
                let covers = chunk.file_path.ends_with(target.file)
                    && chunk.start_line <= target.start
                    && chunk.end_line >= target.end;
                out.push_str(&format!(
                    "  {:>2} {:>12.6} {:<28} {}:{}-{}{}\n",
                    rank + 1,
                    score,
                    chunk.chunk_id,
                    chunk.file_path,
                    chunk.start_line,
                    chunk.end_line,
                    if covers { "  <-- COVERS TRUTH" } else { "" }
                ));
            }
            // Truth-chunk rank/score plus per-term tf per field.
            // Coverage semantics mirror the evaluator: the union of the
            // retrieved chunks' precise source_ranges must cover every
            // truth line (touching intervals merge). Report the rank at
            // which the cumulative union first covers the truth, plus the
            // best single-chunk contributor.
            let mut covered: Vec<(usize, usize)> = Vec::new();
            let mut cover_rank: Option<usize> = None;
            let mut best_single: Option<(usize, f64, usize, usize)> = None;
            for (pos, (doc_idx, score)) in ranked.iter().enumerate() {
                let chunk = &bench.bm25.chunks[*doc_idx];
                if !chunk.file_path.ends_with(target.file) {
                    continue;
                }
                let mut single_cover = 0usize;
                for range in &chunk.source_ranges {
                    if range.end_line < range.start_line || range.start_line == 0 {
                        continue;
                    }
                    covered.push((range.start_line, range.end_line));
                    let lo = range.start_line.max(target.start);
                    let hi = range.end_line.min(target.end);
                    if hi >= lo {
                        single_cover += hi - lo + 1;
                    }
                }
                let need = target.end - target.start + 1;
                if best_single.map(|(_, _, _, c)| c).unwrap_or(0) < single_cover {
                    best_single = Some((pos + 1, *score, *doc_idx, single_cover));
                }
                covered.sort_unstable();
                let mut merged: Vec<(usize, usize)> = Vec::new();
                for (s, e) in covered.iter().copied() {
                    match merged.last_mut() {
                        Some(last) if s <= last.1 + 1 => last.1 = last.1.max(e),
                        _ => merged.push((s, e)),
                    }
                }
                let full = merged
                    .iter()
                    .any(|(s, e)| *s <= target.start && *e >= target.end);
                let _ = need;
                if full && cover_rank.is_none() {
                    cover_rank = Some(pos + 1);
                    break;
                }
            }
            out.push_str(&format!("union-cover-rank={cover_rank:?}\n"));
            // Cross-check with the real evaluator primitives on the top-10.
            {
                let mut scan = RankedScan::new(judgment, &[5, 10, 30, 50], bench.bm25.chunks.len());
                for (doc_idx, _) in ranked.iter().take(50) {
                    scan.advance(&bench.bm25.chunks[*doc_idx]);
                }
                let snaps = scan.finish();
                out.push_str("evaluator RankedScan snapshots (k s_m r_m a_m R_s R_r R_a):\n");
                for (k, (score, _)) in [5usize, 10, 30, 50].iter().zip(snaps.iter()) {
                    out.push_str(&format!(
                        "  k={k} recall_strong={:.4} precision_strong={:.4}\n",
                        score.recall_strong, score.precision_strong
                    ));
                }
                let retrieved: Vec<RetrievedCoverage> = ranked
                    .iter()
                    .take(10)
                    .map(|(d, _)| {
                        let c = &bench.bm25.chunks[*d];
                        RetrievedCoverage {
                            file: c.file_path.clone(),
                            ranges: c.source_ranges.clone(),
                        }
                    })
                    .collect();
                let cov = coverage_over_retrieved(judgment, &retrieved);
                out.push_str(&format!("evaluator top-10 recovered={:?}\n", cov.recovered));
            }
            if let Some((rank, score, doc_idx, lines)) = best_single {
                let chunk = &bench.bm25.chunks[doc_idx];
                out.push_str(&format!(
                    "best-single rank={rank} score={score:.6} {} covers {lines} truth lines\n",
                    chunk.chunk_id
                ));
                out.push_str("best-single term tf per field T/K/C:\n");
                out.push_str(&format!("  {}\n", chunk.chunk_id));
                for term in &query.terms {
                    let mut tf = [0u32; 3];
                    if let Some(postings) = term_index.postings.get(&term.text) {
                        for (d, f, t) in postings {
                            if *d as usize == doc_idx {
                                tf[*f as usize] = *t;
                            }
                        }
                    }
                    if tf.iter().any(|t| *t > 0) {
                        out.push_str(&format!(
                            "    {:28} T={} K={} C={}\n",
                            term.text,
                            tf[Field::Title as usize],
                            tf[Field::Keywords as usize],
                            tf[Field::Content as usize]
                        ));
                    }
                }
            } else {
                out.push_str("best-single: NONE (no same-file chunk retrieved)\n");
            }
            std::fs::write(
                out_root.join(format!(
                    "{baseline}__{}_{}-{}.txt",
                    target.query_id, target.start, target.end
                )),
                &out,
            )
            .expect("write debug output");
        }
        eprintln!("WROTE {baseline}");
    }
    eprintln!("=== ranked diagnostics in outputs/debug/bm25_ranked/ ===");
}
