//! Shared benchmark evaluation logic
//!
//! Provides the scoring functions used by all benchmark runners:
//! embedding cosine-similarity, BM25, file-documentation ranking,
//! and per-query relevance-info collection.

use std::collections::HashMap;

use crate::bench_data::{
    BenchmarkData, ChunkData, ChunkSourceRange, QueryType, RelevanceJudgment, RelevanceLevel,
    compute_bm25_scores, cosine_similarity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeMode {
    Precise,
    GroupFilled,
}

impl RangeMode {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Precise => "",
            Self::GroupFilled => "_group",
        }
    }
}

use crate::range_evaluator::{
    RangeBasedPerQueryScore, RankedScan, RetrievedCoverage, coverage_over_retrieved,
};
use cce_storage_bm25::TermOperator;
use cce_types::{TestStatus, segments};

pub const BASELINES: &[&str] = &[
    "full_pipeline",
    "full_pipeline_raw_source",
    "direct_chunking",
];
pub const TOP_K_VALUES: &[usize] = &[5, 10, 20, 30, 50];
pub const DEFAULT_TOP_K: usize = 5;

/// Evaluation variant: which non-implementation chunks participate in retrieval.
///
/// `All` keeps every chunk. `NoTest` drops test-code chunks. `ImplOnly` also
/// drops demo/sample trees (`examples`, `example`, `samples`, `sample`) so
/// ranking can be compared against the query-time path filters that the
/// search API already exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalVariant {
    All,
    NoTest,
    ImplOnly,
}

impl EvalVariant {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::NoTest => "no_test",
            Self::ImplOnly => "impl_only",
        }
    }
}

pub struct BaselineResult {
    pub baseline: String,
    pub variant: String,
    pub query_id: String,
    pub query_text: String,
    pub query_type: QueryType,
    pub retriever_type: String,
    pub top_k: usize,
    pub score: RangeBasedPerQueryScore,
}

pub struct RelevanceInfo {
    pub baseline: String,
    pub query_id: String,
    pub query_text: String,
    pub retriever_type: String,
    pub strong_chunks: Vec<(usize, String, RelevanceLevel)>,
    pub related_chunks: Vec<(usize, String, RelevanceLevel)>,
}

pub struct FileDocumentationScore {
    pub baseline: String,
    pub query_id: String,
    pub retriever: String,
    pub rank: usize,
    pub document_id: String,
    pub chunk_id: String,
    pub score: f64,
    pub is_expected: bool,
}

pub struct FileDocumentationMetric {
    pub baseline: String,
    pub query_id: String,
    pub retriever: String,
    pub rr: f64,
}

/// Determine whether a chunk originates from test code.
///
/// Consumes the end-to-end `test_info` marker carried by `ChunkData` (AST
/// detection in the grouper + per-language file-path rules). When the marker
/// is `Unknown`, fall back to the per-language path rules derived from the
/// chunk's language — never default to `Test`.
pub fn is_test_chunk(chunk: &ChunkData) -> bool {
    // Only chunks explicitly marked as test code (AST or path level) count
    // as test. An `Unknown` marker means no test signal was found, so the
    // chunk is treated as non-test rather than re-derived from the path.
    chunk.test_info.status == TestStatus::Test
}

const EXAMPLE_DIR_SEGMENTS: [&str; 4] = ["examples", "example", "samples", "sample"];

/// Whether a chunk lives under a conventional demo/sample directory.
///
/// Segment names are matched exactly so `my_examples.rs` stays in ranking
/// while `examples/hello-world/index.js` is excluded. Only segments before
/// the first `src` source-root count: package paths such as the Java
/// `src/main/java/com/example/demo` embed `example` as a package name, not
/// as a demo directory.
pub fn is_example_chunk(chunk: &ChunkData) -> bool {
    let mut outside_source_root = true;
    segments(&chunk.file_path).into_iter().any(|segment| {
        if segment == "src" {
            outside_source_root = false;
            return false;
        }
        outside_source_root && EXAMPLE_DIR_SEGMENTS.contains(&segment)
    })
}

fn keep_chunk(chunk: &ChunkData, variant: EvalVariant) -> bool {
    match variant {
        EvalVariant::All => true,
        EvalVariant::NoTest => !is_test_chunk(chunk),
        EvalVariant::ImplOnly => !is_test_chunk(chunk) && !is_example_chunk(chunk),
    }
}

/// Per-retriever test-code diagnostics for one baseline.
pub struct TestDiagnostics {
    pub baseline: String,
    pub retriever: String,
    pub total_chunks: usize,
    pub test_chunks: usize,
    pub unknown_chunks: usize,
    /// Chunks removed by the `NoTest` variant (equals `test_chunks`).
    pub filtered_chunks: usize,
    /// Number of test chunks appearing in the top-5 ranking across all
    /// queries of the `All` variant (occurrences, not unique chunks).
    pub top_k_test_occurrences: usize,
    /// Number of queries whose top-5 contains at least one test chunk.
    pub top_k_test_queries_affected: usize,
}

/// Collect test-code diagnostics for a baseline across both retrievers.
///
/// Ranking is recomputed here (cosine / BM25) so the `All`-variant top-k
/// occurrence statistics are independent of the score bookkeeping inside
/// `evaluate_embedding`/`evaluate_bm25`.
pub fn collect_test_diagnostics(bench: &BenchmarkData) -> Vec<TestDiagnostics> {
    vec![
        collect_embedding_diagnostics(bench),
        collect_bm25_diagnostics(bench),
    ]
}

fn collect_embedding_diagnostics(bench: &BenchmarkData) -> TestDiagnostics {
    let dim = bench.embedding.dimension as usize;
    let mut diagnostics = TestDiagnostics {
        baseline: String::new(),
        retriever: "emb".to_string(),
        total_chunks: 0,
        test_chunks: 0,
        unknown_chunks: 0,
        filtered_chunks: 0,
        top_k_test_occurrences: 0,
        top_k_test_queries_affected: 0,
    };
    if dim == 0 || bench.embedding.vectors.is_empty() {
        return diagnostics;
    }
    let n_chunks = bench.embedding.vectors.len() / dim;
    diagnostics.total_chunks = n_chunks;
    for chunk in &bench.embedding.chunks[..n_chunks] {
        if is_test_chunk(chunk) {
            diagnostics.test_chunks += 1;
        }
        if chunk.test_info.is_unknown() {
            diagnostics.unknown_chunks += 1;
        }
    }
    diagnostics.filtered_chunks = diagnostics.test_chunks;

    let n_queries = bench.embedding.query_vectors.len() / dim;
    for q_idx in 0..n_queries.min(bench.queries.len()) {
        let q_start = q_idx * dim;
        let q_vec = &bench.embedding.query_vectors[q_start..q_start + dim];
        let mut ranked: Vec<(usize, f32)> = (0..n_chunks)
            .map(|c| {
                let c_start = c * dim;
                (
                    c,
                    cosine_similarity(&bench.embedding.vectors[c_start..c_start + dim], q_vec)
                        as f32,
                )
            })
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let top5: Vec<(usize, f32)> = ranked.iter().take(DEFAULT_TOP_K).cloned().collect();
        let affected = top5
            .iter()
            .any(|(c, _)| is_test_chunk(&bench.embedding.chunks[*c]));
        diagnostics.top_k_test_occurrences += top5
            .iter()
            .filter(|(c, _)| is_test_chunk(&bench.embedding.chunks[*c]))
            .count();
        if affected {
            diagnostics.top_k_test_queries_affected += 1;
        }
    }
    diagnostics
}

/// Score all BM25 queries against the benchmark corpus with production
/// semantics: `MixedTokenizer` query expansion, three-field weighted scoring,
/// `Or` operator.
pub fn bm25_scores_for_bench(bench: &BenchmarkData) -> Vec<Vec<f64>> {
    if bench.bm25_documents.is_empty() || bench.query_texts.is_empty() {
        return Vec::new();
    }
    compute_bm25_scores(&bench.bm25_documents, &bench.query_texts, TermOperator::Or)
}

fn collect_bm25_diagnostics(bench: &BenchmarkData) -> TestDiagnostics {
    let mut diagnostics = TestDiagnostics {
        baseline: String::new(),
        retriever: "BM25".to_string(),
        total_chunks: bench.bm25.chunks.len(),
        test_chunks: 0,
        unknown_chunks: 0,
        filtered_chunks: 0,
        top_k_test_occurrences: 0,
        top_k_test_queries_affected: 0,
    };
    for chunk in &bench.bm25.chunks {
        if is_test_chunk(chunk) {
            diagnostics.test_chunks += 1;
        }
        if chunk.test_info.is_unknown() {
            diagnostics.unknown_chunks += 1;
        }
    }
    diagnostics.filtered_chunks = diagnostics.test_chunks;

    let bm25_scores = bm25_scores_for_bench(bench);
    for scores in bm25_scores.iter().take(bench.queries.len()) {
        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let top5: Vec<(usize, f64)> = ranked.iter().take(DEFAULT_TOP_K).cloned().collect();
        diagnostics.top_k_test_occurrences += top5
            .iter()
            .filter(|(c, _)| is_test_chunk(&bench.bm25.chunks[*c]))
            .count();
        if top5
            .iter()
            .any(|(c, _)| is_test_chunk(&bench.bm25.chunks[*c]))
        {
            diagnostics.top_k_test_queries_affected += 1;
        }
    }
    diagnostics
}

/// Evaluation outputs of one retriever pass for a single test-code variant.
#[derive(Default)]
pub struct VariantEvaluation {
    pub results: Vec<BaselineResult>,
    pub relevance: Vec<RelevanceInfo>,
}

/// Evaluation outputs for the three path-filter variants of one retriever.
#[derive(Default)]
pub struct PathFilterEvaluations {
    pub all: VariantEvaluation,
    pub no_test: VariantEvaluation,
    pub impl_only: VariantEvaluation,
}

/// Select the chunks passing a variant's path filter.
///
/// Returns (original indices, cloned chunks); the two are index-aligned.
fn select_chunks(chunks: &[ChunkData], variant: EvalVariant) -> (Vec<usize>, Vec<ChunkData>) {
    let indices: Vec<usize> = (0..chunks.len())
        .filter(|&i| keep_chunk(&chunks[i], variant))
        .collect();
    let selected: Vec<ChunkData> = indices.iter().map(|&i| chunks[i].clone()).collect();
    (indices, selected)
}

/// Evaluate one variant of a query against an already-scored chunk set.
///
/// Scores and chunks are index-aligned and pre-filtered for the variant; a
/// single sort and a single scan produce every top-k cutoff row plus the
/// top-5 relevance detail.
#[allow(clippy::too_many_arguments)]
fn evaluate_variant_query(
    baseline: &str,
    query_data: &crate::bench_data::QueryData,
    retriever_type: &str,
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    scores: &[f64],
    variant: EvalVariant,
    group_level: bool,
    out: &mut VariantEvaluation,
) {
    let mut ranked: Vec<usize> = (0..chunks.len()).collect();
    ranked.sort_by(|left, right| scores[*right].total_cmp(&scores[*left]));

    let max_cutoff = TOP_K_VALUES.iter().copied().max().unwrap_or(0);
    let mut scan = RankedScan::new(judgment, TOP_K_VALUES, chunks.len());
    let mut seen_segments = std::collections::HashSet::new();
    for &index in ranked.iter().take(max_cutoff) {
        if let Some(chunk) = chunks.get(index) {
            if group_level
                && !chunk.segment_id.is_empty()
                && !seen_segments.insert(chunk.segment_id.clone())
            {
                continue;
            }
            scan.advance(chunk);
        }
    }
    let range_scores = scan.finish();

    for (&top_k, (range, _)) in TOP_K_VALUES.iter().zip(range_scores) {
        out.results.push(BaselineResult {
            baseline: baseline.to_string(),
            variant: variant.label().to_string(),
            query_id: query_data.id.clone(),
            query_text: query_data.text.clone(),
            query_type: judgment.query_type,
            retriever_type: retriever_type.to_string(),
            top_k,
            score: range,
        });
    }

    out.relevance.push(build_relevance_info_ranked(
        baseline,
        query_data,
        retriever_type,
        judgment,
        chunks,
        &ranked,
        DEFAULT_TOP_K,
    ));
}

pub fn group_span_map(chunks: &[ChunkData]) -> std::collections::HashMap<String, (usize, usize)> {
    use std::collections::HashMap;
    let mut min_map: HashMap<String, usize> = HashMap::new();
    let mut max_map: HashMap<String, usize> = HashMap::new();
    for chunk in chunks {
        if chunk.start_line == 0 {
            continue;
        }
        let min_entry = min_map
            .entry(chunk.segment_id.clone())
            .or_insert(chunk.start_line);
        *min_entry = (*min_entry).min(chunk.start_line);
        let max_entry = max_map
            .entry(chunk.segment_id.clone())
            .or_insert(chunk.end_line);
        *max_entry = (*max_entry).max(chunk.end_line);
    }
    let mut out = HashMap::new();
    for (seg, &mn) in &min_map {
        if let Some(&mx) = max_map.get(seg) {
            if mx >= mn {
                out.insert(seg.clone(), (mn, mx));
            }
        }
    }
    out
}

pub fn group_filled_chunk(
    chunk: &ChunkData,
    spans: &std::collections::HashMap<String, (usize, usize)>,
) -> ChunkData {
    let mut filled = chunk.clone();
    if let Some(&(mn, mx)) = spans.get(&chunk.segment_id) {
        filled.source_ranges = vec![ChunkSourceRange {
            start_line: mn,
            end_line: mx,
        }];
    }
    filled
}

pub fn evaluate_embedding(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
    results: &mut Vec<BaselineResult>,
    relevance: &mut Vec<RelevanceInfo>,
    variant: EvalVariant,
    range_mode: RangeMode,
) {
    let dim = bench.embedding.dimension as usize;
    if dim == 0 || bench.embedding.vectors.is_empty() {
        return;
    }
    let n_chunks = bench.embedding.vectors.len() / dim;
    let n_queries = bench.embedding.query_vectors.len() / dim;

    let group_level = matches!(range_mode, RangeMode::GroupFilled);
    let spans = if group_level {
        group_span_map(&bench.embedding.chunks[..n_chunks])
    } else {
        std::collections::HashMap::new()
    };
    let source_chunks: Vec<ChunkData> = match range_mode {
        RangeMode::Precise => bench.embedding.chunks[..n_chunks].to_vec(),
        RangeMode::GroupFilled => bench.embedding.chunks[..n_chunks]
            .iter()
            .map(|c| group_filled_chunk(c, &spans))
            .collect(),
    };

    let (chunk_indices, filtered_chunks) = select_chunks(&source_chunks, variant);
    let mut out = VariantEvaluation::default();

    for q_idx in 0..bench.queries.len().min(n_queries) {
        let query_data = &bench.queries[q_idx];
        let Some(judgment) = judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };

        let q_start = q_idx * dim;
        let q_vec = &bench.embedding.query_vectors[q_start..q_start + dim];
        let scores: Vec<f64> = chunk_indices
            .iter()
            .map(|&c_idx| {
                let c_start = c_idx * dim;
                cosine_similarity(&bench.embedding.vectors[c_start..c_start + dim], q_vec)
            })
            .collect();

        let retriever = format!("emb{}", range_mode.suffix());
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &filtered_chunks,
            &scores,
            variant,
            group_level,
            &mut out,
        );
    }

    results.extend(out.results);
    relevance.extend(out.relevance);
}

/// Evaluate path-filter variants with shared score computation.
///
/// Embedding scores do not depend on the path filter, so one pass serves the
/// `All`, `NoTest` and `ImplOnly` variants.
pub fn evaluate_embedding_variants(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
    range_mode: RangeMode,
) -> PathFilterEvaluations {
    let dim = bench.embedding.dimension as usize;
    if dim == 0 || bench.embedding.vectors.is_empty() {
        return PathFilterEvaluations::default();
    }
    let n_chunks = bench.embedding.vectors.len() / dim;
    let n_queries = bench.embedding.query_vectors.len() / dim;

    let group_level = matches!(range_mode, RangeMode::GroupFilled);
    let spans = if group_level {
        group_span_map(&bench.embedding.chunks[..n_chunks])
    } else {
        std::collections::HashMap::new()
    };
    let all_chunks: Vec<ChunkData> = match range_mode {
        RangeMode::Precise => bench.embedding.chunks[..n_chunks].to_vec(),
        RangeMode::GroupFilled => bench.embedding.chunks[..n_chunks]
            .iter()
            .map(|c| group_filled_chunk(c, &spans))
            .collect(),
    };
    let (no_test_indices, no_test_chunks) = select_chunks(&all_chunks, EvalVariant::NoTest);
    let (impl_only_indices, impl_only_chunks) = select_chunks(&all_chunks, EvalVariant::ImplOnly);
    let mut evaluations = PathFilterEvaluations::default();

    let retriever = format!("emb{}", range_mode.suffix());

    for q_idx in 0..bench.queries.len().min(n_queries) {
        let query_data = &bench.queries[q_idx];
        let Some(judgment) = judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };

        let q_start = q_idx * dim;
        let q_vec = &bench.embedding.query_vectors[q_start..q_start + dim];
        let scores: Vec<f64> = (0..n_chunks)
            .map(|c_idx| {
                let c_start = c_idx * dim;
                cosine_similarity(&bench.embedding.vectors[c_start..c_start + dim], q_vec)
            })
            .collect();

        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &all_chunks,
            &scores,
            EvalVariant::All,
            group_level,
            &mut evaluations.all,
        );
        let no_test_scores: Vec<f64> = no_test_indices.iter().map(|&i| scores[i]).collect();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &no_test_chunks,
            &no_test_scores,
            EvalVariant::NoTest,
            group_level,
            &mut evaluations.no_test,
        );
        let impl_only_scores: Vec<f64> = impl_only_indices.iter().map(|&i| scores[i]).collect();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &impl_only_chunks,
            &impl_only_scores,
            EvalVariant::ImplOnly,
            group_level,
            &mut evaluations.impl_only,
        );
    }

    evaluations
}

pub fn evaluate_bm25(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
    results: &mut Vec<BaselineResult>,
    relevance: &mut Vec<RelevanceInfo>,
    variant: EvalVariant,
    range_mode: RangeMode,
) {
    if bench.bm25_documents.is_empty() {
        return;
    }
    let bm25_scores = bm25_scores_for_bench(bench);

    let group_level = matches!(range_mode, RangeMode::GroupFilled);
    let spans = if group_level {
        group_span_map(&bench.bm25.chunks)
    } else {
        std::collections::HashMap::new()
    };
    let source_chunks: Vec<ChunkData> = match range_mode {
        RangeMode::Precise => bench.bm25.chunks.clone(),
        RangeMode::GroupFilled => bench
            .bm25
            .chunks
            .iter()
            .map(|c| group_filled_chunk(c, &spans))
            .collect(),
    };

    let (chunk_indices, filtered_chunks) = select_chunks(&source_chunks, variant);
    let mut out = VariantEvaluation::default();

    let retriever = format!("BM25{}", range_mode.suffix());

    for (q_idx, scores) in bm25_scores
        .iter()
        .enumerate()
        .take(bench.queries.len().min(bm25_scores.len()))
    {
        let query_data = &bench.queries[q_idx];
        let Some(judgment) = judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };

        let filtered_scores: Vec<f64> = chunk_indices.iter().map(|&i| scores[i]).collect();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &filtered_chunks,
            &filtered_scores,
            variant,
            group_level,
            &mut out,
        );
    }

    results.extend(out.results);
    relevance.extend(out.relevance);
}

/// Evaluate path-filter variants with shared score computation.
///
/// BM25 scores do not depend on the path filter, so one scoring pass serves
/// the `All`, `NoTest` and `ImplOnly` variants.
pub fn evaluate_bm25_variants(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
    range_mode: RangeMode,
) -> PathFilterEvaluations {
    if bench.bm25_documents.is_empty() {
        return PathFilterEvaluations::default();
    }
    let bm25_scores = bm25_scores_for_bench(bench);

    let group_level = matches!(range_mode, RangeMode::GroupFilled);
    let spans = if group_level {
        group_span_map(&bench.bm25.chunks)
    } else {
        std::collections::HashMap::new()
    };
    let source_chunks: Vec<ChunkData> = match range_mode {
        RangeMode::Precise => bench.bm25.chunks.clone(),
        RangeMode::GroupFilled => bench
            .bm25
            .chunks
            .iter()
            .map(|c| group_filled_chunk(c, &spans))
            .collect(),
    };

    let (_, all_chunks) = select_chunks(&source_chunks, EvalVariant::All);
    let (no_test_indices, no_test_chunks) = select_chunks(&source_chunks, EvalVariant::NoTest);
    let (impl_only_indices, impl_only_chunks) =
        select_chunks(&source_chunks, EvalVariant::ImplOnly);
    let mut evaluations = PathFilterEvaluations::default();

    let retriever = format!("BM25{}", range_mode.suffix());

    for (q_idx, scores) in bm25_scores
        .iter()
        .enumerate()
        .take(bench.queries.len().min(bm25_scores.len()))
    {
        let query_data = &bench.queries[q_idx];
        let Some(judgment) = judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };

        let all_scores: Vec<f64> = scores.to_vec();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &all_chunks,
            &all_scores,
            EvalVariant::All,
            group_level,
            &mut evaluations.all,
        );
        let no_test_scores: Vec<f64> = no_test_indices.iter().map(|&i| scores[i]).collect();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &no_test_chunks,
            &no_test_scores,
            EvalVariant::NoTest,
            group_level,
            &mut evaluations.no_test,
        );
        let impl_only_scores: Vec<f64> = impl_only_indices.iter().map(|&i| scores[i]).collect();
        evaluate_variant_query(
            baseline,
            query_data,
            &retriever,
            judgment,
            &impl_only_chunks,
            &impl_only_scores,
            EvalVariant::ImplOnly,
            group_level,
            &mut evaluations.impl_only,
        );
    }

    evaluations
}

pub fn build_relevance_info(
    baseline: &str,
    query_data: &crate::bench_data::QueryData,
    retriever_type: &str,
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    scores: &[f64],
    top_k: usize,
) -> RelevanceInfo {
    let mut indices: Vec<usize> = (0..chunks.len()).collect();
    indices.sort_by(|a, b| {
        scores[*b]
            .partial_cmp(&scores[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    build_relevance_info_ranked(
        baseline,
        query_data,
        retriever_type,
        judgment,
        chunks,
        &indices,
        top_k,
    )
}

/// Build top-k relevance detail rows from an already-ranked chunk list.
///
/// `ranked_indices` must be sorted by score descending.
///
/// The rows report the same relation the metrics score: a row counts as a
/// hit when the chunk contributes to a labeled target the top-k set
/// recovers. A target may be split across several chunks, so the chunks
/// carrying the missing parts are the ones that count.
fn build_relevance_info_ranked(
    baseline: &str,
    query_data: &crate::bench_data::QueryData,
    retriever_type: &str,
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    ranked_indices: &[usize],
    top_k: usize,
) -> RelevanceInfo {
    let mut strong = Vec::new();
    let mut related = Vec::new();

    let selected: Vec<(usize, &ChunkData)> = ranked_indices
        .iter()
        .enumerate()
        .take(top_k)
        .filter_map(|(rank, &idx)| chunks.get(idx).map(|chunk| (rank, chunk)))
        .collect();
    let retrieved: Vec<RetrievedCoverage> = selected
        .iter()
        .map(|(_, chunk)| RetrievedCoverage {
            file: crate::bench_data::normalize_path(&chunk.file_path),
            ranges: chunk.source_ranges.clone(),
        })
        .collect();
    let coverage = coverage_over_retrieved(judgment, &retrieved);

    for (index, (rank, chunk)) in selected.iter().enumerate() {
        let Some(level) = coverage.level_for(index) else {
            continue;
        };
        let loc = format!(
            "{}:{}-{}",
            chunk.file_path, chunk.start_line, chunk.end_line
        );
        match level {
            RelevanceLevel::Strong => strong.push((rank + 1, loc, level)),
            RelevanceLevel::Related => related.push((rank + 1, loc, level)),
            _ => {}
        }
    }

    RelevanceInfo {
        baseline: baseline.to_string(),
        query_id: query_data.id.clone(),
        query_text: query_data.text.clone(),
        retriever_type: retriever_type.to_string(),
        strong_chunks: strong,
        related_chunks: related,
    }
}

pub fn document_group_id(chunk_id: &str) -> String {
    for marker in ["_bm25_", "_emb_"] {
        if let Some((document_id, _)) = chunk_id.rsplit_once(marker) {
            return document_id.to_string();
        }
    }
    chunk_id.to_string()
}

pub fn score_file_documentation(
    baseline: &str,
    bench: &BenchmarkData,
) -> Result<(Vec<FileDocumentationScore>, Vec<FileDocumentationMetric>), String> {
    if bench.queries.is_empty() || bench.bm25.chunks.is_empty() || bench.embedding.chunks.is_empty()
    {
        return Err(
            "File-documentation benchmark must contain queries and both retrieval paths".into(),
        );
    }

    let mut scores = Vec::new();
    let mut metrics = Vec::new();

    let bm25_scores = bm25_scores_for_bench(bench);
    for (query_index, query) in bench.queries.iter().enumerate() {
        let ranked: Vec<_> = bm25_scores
            .get(query_index)
            .into_iter()
            .flatten()
            .copied()
            .enumerate()
            .collect();
        score_file_documentation_ranking(
            baseline,
            query,
            "BM25",
            ranked,
            &bench.bm25.chunks,
            &mut scores,
            &mut metrics,
        );
    }

    let dimension = bench.embedding.dimension as usize;
    let embedding_valid = dimension > 0
        && bench.embedding.vectors.len() == bench.embedding.chunks.len() * dimension
        && bench.embedding.query_vectors.len() == bench.queries.len() * dimension;

    if embedding_valid {
        for (query_index, query) in bench.queries.iter().enumerate() {
            let query_start = query_index * dimension;
            let query_vector = &bench.embedding.query_vectors[query_start..query_start + dimension];
            let ranked: Vec<(usize, f64)> = bench
                .embedding
                .chunks
                .iter()
                .enumerate()
                .map(|(chunk_index, _)| {
                    let chunk_start = chunk_index * dimension;
                    (
                        chunk_index,
                        cosine_similarity(
                            query_vector,
                            &bench.embedding.vectors[chunk_start..chunk_start + dimension],
                        ),
                    )
                })
                .collect();
            score_file_documentation_ranking(
                baseline,
                query,
                "Embedding",
                ranked,
                &bench.embedding.chunks,
                &mut scores,
                &mut metrics,
            );
        }
    } else {
        eprintln!(
            "  WARNING: {baseline} file-doc embedding vectors missing or invalid; skipping embedding evaluation"
        );
    }

    Ok((scores, metrics))
}

fn score_file_documentation_ranking(
    baseline: &str,
    query: &crate::bench_data::QueryData,
    retriever: &str,
    ranked_chunks: Vec<(usize, f64)>,
    chunks: &[ChunkData],
    scores: &mut Vec<FileDocumentationScore>,
    metrics: &mut Vec<FileDocumentationMetric>,
) {
    let mut best_by_document: HashMap<String, (usize, f64)> = HashMap::new();
    for (chunk_index, score) in ranked_chunks {
        let Some(chunk) = chunks.get(chunk_index) else {
            continue;
        };
        let document_id = document_group_id(&chunk.chunk_id);
        let replace = best_by_document
            .get(&document_id)
            .is_none_or(|(_, current)| score > *current);
        if replace {
            best_by_document.insert(document_id, (chunk_index, score));
        }
    }

    let mut ranked_documents: Vec<_> = best_by_document.into_iter().collect();
    ranked_documents.sort_by(|left, right| {
        right
            .1
            .1
            .total_cmp(&left.1.1)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.1.0.cmp(&right.1.0))
    });

    let mut rr = 0.0;

    for (index, (document_id, (chunk_index, score))) in ranked_documents.iter().enumerate() {
        let rank = index + 1;
        let Some(chunk) = chunks.get(*chunk_index) else {
            continue;
        };
        let is_expected = query.relevant_names.iter().any(|name| document_id == name);
        if is_expected && rr == 0.0 {
            rr = 1.0 / rank as f64;
        }
        scores.push(FileDocumentationScore {
            baseline: baseline.to_string(),
            query_id: query.id.clone(),
            retriever: retriever.to_string(),
            rank,
            document_id: document_id.clone(),
            chunk_id: chunk.chunk_id.clone(),
            score: *score,
            is_expected,
        });
    }

    metrics.push(FileDocumentationMetric {
        baseline: baseline.to_string(),
        query_id: query.id.clone(),
        retriever: retriever.to_string(),
        rr,
    });
}

/// Data paths for a benchmark project
pub struct BenchmarkPaths {
    pub project_name: &'static str,
}

impl BenchmarkPaths {
    pub fn new(project_name: &'static str) -> Self {
        Self { project_name }
    }

    pub fn output_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("outputs")
            .join("benchmark")
            .join(self.project_name)
    }

    pub fn data_dir(&self, baseline: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("benchmark")
            .join(baseline)
            .join(self.project_name)
            .join("bge-m3")
            .join("bench_data.rkyv")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::{ChunkSourceRange, QueryData};
    use cce_types::TestInfo;

    fn chunk(chunk_id: &str) -> ChunkData {
        ChunkData {
            chunk_id: chunk_id.to_string(),
            entity_name: String::new(),
            file_path: String::new(),
            start_line: 0,
            end_line: 0,
            source_ranges: Vec::<ChunkSourceRange>::new(),
            source_span_kind: String::new(),
            test_info: TestInfo::unknown(),
            language: None,
            entity_ids: Vec::new(),
            segment_id: String::new(),
        }
    }

    #[test]
    fn file_documentation_ranking_breaks_score_ties_by_document_id() {
        let query = QueryData {
            id: "FD1".to_string(),
            text: "query".to_string(),
            query_type: crate::bench_data::QueryType::FileDocumentation,
            relevant_names: vec!["file_doc_a.rs".to_string()],
            irrelevant_names: Vec::new(),
        };
        let chunks = vec![chunk("file_doc_b.rs_emb_0"), chunk("file_doc_a.rs_emb_0")];
        let mut scores = Vec::new();
        let mut metrics = Vec::new();

        score_file_documentation_ranking(
            "baseline",
            &query,
            "Embedding",
            vec![(0, 0.0), (1, 0.0)],
            &chunks,
            &mut scores,
            &mut metrics,
        );

        assert_eq!(scores[0].document_id, "file_doc_a.rs");
        assert_eq!(scores[0].rank, 1);
        assert_eq!(metrics[0].rr, 1.0);
    }

    fn chunk_at(file_path: &str) -> ChunkData {
        let mut data = chunk("id");
        data.file_path = file_path.to_string();
        data
    }

    #[test]
    fn example_path_filter_matches_directory_segments_exactly() {
        assert!(is_example_chunk(&chunk_at("examples/hello-world/index.js")));
        assert!(is_example_chunk(&chunk_at(
            "examples\\hello-world\\index.js"
        )));
        assert!(is_example_chunk(&chunk_at(
            "samples/MediatR.Examples/Program.cs"
        )));
        assert!(is_example_chunk(&chunk_at(
            "crates/ignore/examples/walk.rs"
        )));
        assert!(!is_example_chunk(&chunk_at("lib/express.js")));
        assert!(!is_example_chunk(&chunk_at("src/my_examples.rs")));
        assert!(!is_example_chunk(&chunk_at("src/sample_utils.py")));
        // Java package segments inside a source root are not demo dirs.
        assert!(!is_example_chunk(&chunk_at(
            "src/main/java/com/example/demo/service/UserService.java"
        )));
        // A demo tree outside the source root still counts, even when it
        // itself contains `src`.
        assert!(is_example_chunk(&chunk_at(
            "examples/demo/src/main/java/App.java"
        )));
    }

    #[test]
    fn impl_only_drops_test_and_example_chunks() {
        let lib = chunk_at("lib/application.js");
        let mut test = chunk_at("test/app.use.js");
        test.test_info = TestInfo::test_path();
        let example = chunk_at("examples/cookies/index.js");

        assert!(keep_chunk(&lib, EvalVariant::All));
        assert!(keep_chunk(&test, EvalVariant::All));
        assert!(keep_chunk(&example, EvalVariant::All));

        assert!(keep_chunk(&lib, EvalVariant::NoTest));
        assert!(!keep_chunk(&test, EvalVariant::NoTest));
        assert!(keep_chunk(&example, EvalVariant::NoTest));

        assert!(keep_chunk(&lib, EvalVariant::ImplOnly));
        assert!(!keep_chunk(&test, EvalVariant::ImplOnly));
        assert!(!keep_chunk(&example, EvalVariant::ImplOnly));
    }
}
