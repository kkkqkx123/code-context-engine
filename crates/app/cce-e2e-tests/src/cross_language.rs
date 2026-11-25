//! Independent cross-language retrieval evaluation.
//!
//! Cross-language queries are measured against an English query that shares
//! the same judged ranges. The English query represents the retrieval and
//! chunking quality of the corpus; the difference between the two queries is
//! the language-alignment cost. BM25 rows are kept only as a diagnostic: a
//! non-English query has no lexical overlap with the English corpus, so those
//! scores must not drive chunking or fusion conclusions.

use crate::bench_data::{
    BenchmarkData, QueryType, RelevanceJudgment, SourceRange, compute_bm25_scores,
    cosine_similarity, load_benchmark_data,
};
use crate::judgments::evaluate::{
    BASELINES, BenchmarkPaths, RangeMode, group_filled_chunk, group_span_map,
};
use crate::range_evaluator::evaluate_query_range_based_cutoffs;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

const CUTOFFS: &[usize] = &[5, 10];

/// One paired measurement: English query versus its non-English counterpart.
#[derive(Debug, Clone)]
struct CrossLanguageRow {
    baseline: String,
    range_mode: &'static str,
    cross_id: String,
    english_id: String,
    overlap: usize,
    top_k: usize,
    english_recall: f64,
    cross_recall: f64,
    english_f1: f64,
    cross_f1: f64,
    gap_f1: f64,
    bm25_cross_recall: f64,
    bm25_cross_f1: f64,
}

/// Run the independent cross-language benchmark for one project.
pub fn run_cross_language_benchmark(
    project_name: &'static str,
    judgments: &[RelevanceJudgment],
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = BenchmarkPaths::new(project_name);
    let mut rows = Vec::new();
    let mut pairs = 0_usize;

    for baseline in BASELINES {
        let data_path = paths.data_dir(baseline);
        if !data_path.exists() {
            eprintln!(
                "WARNING: {baseline} data not found at {}",
                data_path.display()
            );
            continue;
        }
        let bench = load_benchmark_data(&data_path)?;
        for range_mode in [RangeMode::Precise, RangeMode::GroupFilled] {
            let chunks = prepared_chunks(&bench, range_mode);
            let bm25_scores = bm25_scores_for_cross_queries(&bench);
            for cross in judgments
                .iter()
                .filter(|judgment| judgment.query_type == QueryType::CrossLang)
            {
                let Some(english) = paired_english_judgment(cross, judgments) else {
                    continue;
                };
                let Some(english_scores) = embedding_scores(&bench, &english.id, &chunks) else {
                    continue;
                };
                let Some(cross_scores) = embedding_scores(&bench, &cross.id, &chunks) else {
                    continue;
                };
                let Some(bm25_cross) = bm25_scores.get(&cross.id) else {
                    continue;
                };
                let english_metrics =
                    evaluate_query_range_based_cutoffs(english, &chunks, &english_scores, CUTOFFS);
                let cross_metrics =
                    evaluate_query_range_based_cutoffs(cross, &chunks, &cross_scores, CUTOFFS);
                let bm25_metrics = evaluate_query_range_based_cutoffs(
                    cross,
                    &bench.bm25.chunks,
                    bm25_cross,
                    CUTOFFS,
                );
                let overlap = range_overlap(cross, english);
                for (index, top_k) in CUTOFFS.iter().enumerate() {
                    let en = &english_metrics[index];
                    let zh = &cross_metrics[index];
                    let bm = &bm25_metrics[index];
                    rows.push(CrossLanguageRow {
                        baseline: (*baseline).to_string(),
                        range_mode: match range_mode {
                            RangeMode::Precise => "precise",
                            RangeMode::GroupFilled => "group",
                        },
                        cross_id: cross.id.clone(),
                        english_id: english.id.clone(),
                        overlap,
                        top_k: *top_k,
                        english_recall: en.recall_any,
                        cross_recall: zh.recall_any,
                        english_f1: en.f1_any,
                        cross_f1: zh.f1_any,
                        gap_f1: en.f1_any - zh.f1_any,
                        bm25_cross_recall: bm.recall_any,
                        bm25_cross_f1: bm.f1_any,
                    });
                }
                pairs += 1;
            }
        }
    }

    if rows.is_empty() {
        eprintln!("No cross-language pairs evaluated. Run the bench generators first.");
        return Ok(());
    }

    let out = paths
        .output_dir()
        .parent()
        .expect("benchmark output has a parent")
        .join("cross_language")
        .join(project_name);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("cross_language_metrics.md"),
        render_report(project_name, pairs, &rows),
    )?;
    std::fs::write(out.join("run_manifest.txt"), render_manifest(project_name))?;
    println!("Cross-language report written to {}", out.display());
    Ok(())
}

/// Pick the English judgment with the largest judged-range overlap.
///
/// Semantic judgments win ties because they are the intended comparison for
/// behavior-level cross-language queries.
fn paired_english_judgment<'a>(
    cross: &RelevanceJudgment,
    judgments: &'a [RelevanceJudgment],
) -> Option<&'a RelevanceJudgment> {
    let cross_ranges = range_keys(cross);
    let mut best: Option<(&'a RelevanceJudgment, usize, u8)> = None;
    for judgment in judgments {
        if judgment.id == cross.id
            || judgment.query_type == QueryType::CrossLang
            || judgment.query_type == QueryType::FileDocumentation
        {
            continue;
        }
        let overlap = range_keys(judgment).intersection(&cross_ranges).count();
        if overlap == 0 {
            continue;
        }
        let priority = match judgment.query_type {
            QueryType::Semantic => 2,
            QueryType::Qualified => 1,
            QueryType::Fuzzy | QueryType::CrossLang | QueryType::FileDocumentation => 0,
        };
        let better = match best {
            None => true,
            Some((current, current_overlap, current_priority)) => {
                (overlap, priority) > (current_overlap, current_priority)
                    || ((overlap, priority) == (current_overlap, current_priority)
                        && judgment.id < current.id)
            }
        };
        if better {
            best = Some((judgment, overlap, priority));
        }
    }
    best.map(|(judgment, _, _)| judgment)
}

fn range_keys(judgment: &RelevanceJudgment) -> BTreeSet<(String, usize, usize)> {
    judgment
        .relevant_ranges
        .iter()
        .map(|(range, _)| range_key(range))
        .collect()
}

fn range_key(range: &SourceRange) -> (String, usize, usize) {
    (range.file.clone(), range.start_line, range.end_line)
}

fn range_overlap(left: &RelevanceJudgment, right: &RelevanceJudgment) -> usize {
    range_keys(left).intersection(&range_keys(right)).count()
}

fn prepared_chunks(
    bench: &BenchmarkData,
    range_mode: RangeMode,
) -> Vec<crate::bench_data::ChunkData> {
    let dim = bench.embedding.dimension as usize;
    let n_chunks = bench.embedding.vectors.len() / dim;
    let chunks = &bench.embedding.chunks[..n_chunks];
    match range_mode {
        RangeMode::Precise => chunks.to_vec(),
        RangeMode::GroupFilled => {
            let spans = group_span_map(chunks);
            chunks
                .iter()
                .map(|chunk| group_filled_chunk(chunk, &spans))
                .collect()
        }
    }
}

fn embedding_scores(
    bench: &BenchmarkData,
    query_id: &str,
    chunks: &[crate::bench_data::ChunkData],
) -> Option<Vec<f64>> {
    let dim = bench.embedding.dimension as usize;
    let index = bench
        .queries
        .iter()
        .position(|query| query.id == query_id)?;
    let query_start = index * dim;
    let query_vector = bench
        .embedding
        .query_vectors
        .get(query_start..query_start + dim)?;
    chunks
        .iter()
        .enumerate()
        .map(|(chunk_index, _)| {
            let start = chunk_index * dim;
            let chunk_vector = bench.embedding.vectors.get(start..start + dim)?;
            Some(cosine_similarity(chunk_vector, query_vector))
        })
        .collect::<Option<Vec<f64>>>()
}

fn bm25_scores_for_cross_queries(bench: &BenchmarkData) -> BTreeMap<String, Vec<f64>> {
    let query_texts: Vec<String> = bench
        .queries
        .iter()
        .map(|query| query.text.clone())
        .collect();
    let scores = compute_bm25_scores(
        &bench.bm25_documents,
        &query_texts,
        cce_storage_bm25::TermOperator::Or,
    );
    bench
        .queries
        .iter()
        .enumerate()
        .filter_map(|(index, query)| {
            scores
                .get(index)
                .cloned()
                .map(|values| (query.id.clone(), values))
        })
        .collect()
}

fn render_report(project: &str, pairs: usize, rows: &[CrossLanguageRow]) -> String {
    let mut page = String::new();
    writeln!(page, "# Cross-Language Benchmark: {project}").expect("write report title");
    writeln!(page).expect("write blank line");
    writeln!(
        page,
        "- Pair measurements: {pairs}, each matching a non-English judgment to the English judgment with the largest judged-range overlap."
    )
    .expect("write pair count");
    writeln!(
        page,
        "- English rows measure corpus retrieval; language_gap = English F1 - non-English F1."
    )
    .expect("write gap definition");
    writeln!(
        page,
        "- BM25 rows are diagnostic only: any score comes from accidental lexical overlap, not cross-lingual capability."
    )
    .expect("write bm25 note");
    writeln!(
        page,
        "- This report is separate from the main benchmark and does not feed chunking, BM25-parameter or default aggregation settings."
    )
    .expect("write scope note");
    writeln!(page).expect("write blank line");

    writeln!(page, "## Paired measurements").expect("write section title");
    writeln!(page).expect("write blank line");
    let headers = [
        "baseline",
        "range",
        "cross_id",
        "english_id",
        "overlap",
        "k",
        "en_recall",
        "cross_recall",
        "en_f1",
        "cross_f1",
        "language_gap_f1",
        "bm25_cross_recall",
        "bm25_cross_f1",
    ];
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            vec![
                row.baseline.clone(),
                row.range_mode.to_string(),
                row.cross_id.clone(),
                row.english_id.clone(),
                row.overlap.to_string(),
                row.top_k.to_string(),
                format!("{:.4}", row.english_recall),
                format!("{:.4}", row.cross_recall),
                format!("{:.4}", row.english_f1),
                format!("{:.4}", row.cross_f1),
                format!("{:+.4}", row.gap_f1),
                format!("{:.4}", row.bm25_cross_recall),
                format!("{:.4}", row.bm25_cross_f1),
            ]
        })
        .collect();
    write_table(&mut page, &headers, &table);

    writeln!(page).expect("write blank line");
    writeln!(page, "## Summary at top-10").expect("write section title");
    writeln!(page).expect("write blank line");
    let mut grouped: BTreeMap<(String, String), Vec<&CrossLanguageRow>> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.top_k == 10) {
        grouped
            .entry((row.baseline.clone(), row.range_mode.to_string()))
            .or_default()
            .push(row);
    }
    let headers = [
        "baseline",
        "range",
        "n",
        "en_f1",
        "cross_f1",
        "language_gap_f1",
        "bm25_cross_f1",
    ];
    let table: Vec<Vec<String>> = grouped
        .iter()
        .map(|((baseline, range), group)| {
            vec![
                baseline.clone(),
                range.clone(),
                group.len().to_string(),
                format!("{:.4}", mean(group.iter().map(|row| row.english_f1))),
                format!("{:.4}", mean(group.iter().map(|row| row.cross_f1))),
                format!("{:+.4}", mean(group.iter().map(|row| row.gap_f1))),
                format!("{:.4}", mean(group.iter().map(|row| row.bm25_cross_f1))),
            ]
        })
        .collect();
    write_table(&mut page, &headers, &table);
    page
}

fn render_manifest(project: &str) -> String {
    let mut page = format!("project: {project}\n");
    page.push_str("scope: cross_language_only\n");
    page.push_str("data: data/benchmark/{baseline}/{project}/bge-m3/bench_data.rkyv\n");
    page.push_str("embedding: precomputed bge-m3 vectors from the bench dataset\n");
    page.push_str("pairing: largest judged-range overlap with a non-cross-language judgment\n");
    page.push_str("metrics: range-based R/F1 plus language_gap; BM25 is diagnostic only (accidental overlap)\n");
    page.push_str("excluded_from: main aggregate reports, BM25 sweep, chunking verdicts, aggregation defaults\n");
    page
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0_usize), |(sum, count), value| {
        (sum + value, count + 1)
    });
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn write_table(page: &mut String, headers: &[&str], rows: &[Vec<String>]) {
    writeln!(page, "| {} |", headers.join(" | ")).expect("write table header");
    writeln!(
        page,
        "|{}|",
        headers.iter().map(|_| "---").collect::<Vec<_>>().join("|")
    )
    .expect("write table separator");
    for row in rows {
        writeln!(page, "| {} |", row.join(" | ")).expect("write table row");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::{QueryType, RelevanceLevel};

    fn judgment(id: &str, query_type: QueryType, ranges: &[(usize, usize)]) -> RelevanceJudgment {
        RelevanceJudgment {
            id: id.to_string(),
            query_text: id.to_string(),
            query_type,
            fuzzy_subtype: None,
            relevant_ranges: ranges
                .iter()
                .map(|&(start, end)| {
                    (
                        SourceRange {
                            file: "src/lib.rs".to_string(),
                            start_line: start,
                            end_line: end,
                        },
                        RelevanceLevel::Strong,
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn pairs_cross_language_queries_by_range_overlap() {
        let cross = judgment("G4Q1", QueryType::CrossLang, &[(10, 20), (30, 40)]);
        let semantic = judgment("G2Q3", QueryType::Semantic, &[(10, 20)]);
        let qualified = judgment("G1Q7", QueryType::Qualified, &[(30, 40)]);
        let judgments = [semantic, qualified];
        let paired = paired_english_judgment(&cross, &judgments);
        assert_eq!(
            paired.map(|judgment| judgment.id.as_str()),
            Some("G2Q3"),
            "semantic judgment wins the one-range tie"
        );
    }

    #[test]
    fn skips_cross_language_queries_without_shared_ranges() {
        let cross = judgment("G4Q1", QueryType::CrossLang, &[(10, 20)]);
        let english = judgment("G2Q3", QueryType::Semantic, &[(50, 60)]);
        assert!(paired_english_judgment(&cross, &[english]).is_none());
    }
}
