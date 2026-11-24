//! Retrieval-method benchmark: dense / sparse / hybrid fusion comparison.
//!
//! Reuses `data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv`
//! (no regeneration, no LLM calls) and evaluates the recall-method families
//! — `emb`, `bm25`, `minmax-*`, `rrf-*`, their single-path policy variants and
//! the multi-source aggregation strategies. Single-path `emb`/`bm25` rows
//! measure the raw chunk ranking (each chunk once), identical to the
//! no-aggregation baseline benchmark; the fused methods measure entity-level
//! rankings (one entry per alignment key).
//!
//! # Baseline dimensions
//!
//! Three dimensions are recorded next to the F1 tables because they are what
//! makes a later run comparable:
//!
//! - **single-path policy**: `minmax-0.5-vec` / `-bm25` / `-strict` measure
//!   what each production switch contributes, instead of assuming both sides
//!   are symmetric.
//! - **aggregation strategy**: `agg-max` / `agg-noisy-or` measure the sub-query
//!   merge on the same data, using the production score-combination formula.
//! - **score semantics**: [`ScoreStat`] records how many fused entries sit at
//!   their weight-share ceiling, so a change in the fused score scale is
//!   visible even when the ranking (and therefore F1) is unchanged.

use std::collections::{BTreeMap, HashSet};

use crate::bench_data::{
    BenchmarkData, ChunkData, QueryType, RelevanceJudgment, RelevanceLevel, compute_bm25_scores,
    cosine_similarity, load_benchmark_data,
};
use crate::judgments::evaluate::{BASELINES, BenchmarkPaths, DEFAULT_TOP_K, TOP_K_VALUES};
use crate::range_evaluator::RankedScan;
use crate::retrieval_method::aggregation::{
    AggSource, AggregationStat, merge_sources as merge_aggregation_sources,
};
use crate::retrieval_method::fusion::{
    FusedEntry, PathCoverage, PreparedFusion, RankedPath, SinglePathPolicy, count_unalignable,
    dedup_ranked,
};
use crate::retrieval_method::report;
use cce_config::modules::search::AggregationScoreStrategy;
use cce_storage_bm25::TermOperator;

/// Weight presets for `minmax-*` methods (vector_weight, bm25_weight).
pub const MINMAX_WEIGHTS: &[(f64, f64)] = &[
    (0.9, 0.1),
    (0.7, 0.3),
    (0.6, 0.4),
    (0.5, 0.5),
    (0.4, 0.6),
    (0.3, 0.7),
    (0.1, 0.9),
];

/// Single-path policy variants, evaluated at the balanced weight.
///
/// `Union` is the default and is already covered by `minmax-0.5`, so only the
/// three narrowed policies get their own rows.
pub const POLICY_VARIANTS: &[(&str, SinglePathPolicy)] = &[
    ("-vec", SinglePathPolicy::VectorOnly),
    ("-bm25", SinglePathPolicy::Bm25Only),
    ("-strict", SinglePathPolicy::Intersection),
];

/// Weight of the `minmax-*` rows the policy variants build on.
pub const POLICY_WEIGHT: f64 = 0.5;

/// Rank-smoothing factors for `rrf-*` methods.
pub const RRF_K_VALUES: &[f64] = &[30.0, 60.0, 100.0];

/// `k` of the strict-intersection RRF row.
pub const RRF_STRICT_K: f64 = 60.0;

/// Aggregation strategies evaluated over the two recall paths.
pub const AGGREGATION_STRATEGIES: &[(&str, AggregationScoreStrategy)] = &[
    ("agg-max", AggregationScoreStrategy::Max),
    ("agg-noisy-or", AggregationScoreStrategy::NoisyOr),
];

/// Per-query score for one retrieval method.
pub struct RetrievalMethodScore {
    /// Standard range-based P/R/F1 metrics.
    pub range: crate::range_evaluator::RangeBasedPerQueryScore,
    /// Average first-hit rank across the judgment's relevant ranges.
    pub avg_first_hit_rank: Option<f64>,
    /// Number of (chunk, relevant range) overlaps inside top-k.
    pub redundant_coverage: usize,
}

/// A single (baseline, method, query, top_k) result row.
pub struct MethodResult {
    pub baseline: String,
    pub method: String,
    pub query_id: String,
    pub query_type: QueryType,
    pub top_k: usize,
    pub score: RetrievalMethodScore,
}

/// Top-k relevance detail rows for a retrieval method (used for
/// `relevance_top5.md`).
pub struct MethodRelevance {
    pub baseline: String,
    pub method: String,
    pub query_id: String,
    pub strong_chunks: Vec<(usize, String, RelevanceLevel)>,
    pub related_chunks: Vec<(usize, String, RelevanceLevel)>,
}

/// Cross-path alignment coverage for one baseline.
pub struct AlignmentStat {
    pub baseline: String,
    pub emb_chunks: usize,
    pub bm25_chunks: usize,
    pub emb_keys: usize,
    pub bm25_keys: usize,
    pub common_keys: usize,
    pub emb_only_keys: usize,
    pub bm25_only_keys: usize,
    /// Embedding chunks whose key can never match the BM25 path (no entity and
    /// no segment distinct from the chunk id).
    pub emb_unalignable: usize,
    /// BM25 chunks with the same limitation.
    pub bm25_unalignable: usize,
}

impl AlignmentStat {
    /// Share of all chunks that can never align, in `[0, 1]`.
    ///
    /// The offline counterpart of the runtime
    /// `search_hybrid_alignment_unalignable_ratio`: a non-trivial share is
    /// expected (chunk-id fallback), a share that keeps growing means the
    /// index is missing entity/segment fields.
    pub fn unalignable_ratio(&self) -> f64 {
        let total = self.emb_chunks + self.bm25_chunks;
        if total == 0 {
            0.0
        } else {
            (self.emb_unalignable + self.bm25_unalignable) as f64 / total as f64
        }
    }

    /// Share of distinct keys returned by only one path, in `[0, 1]`.
    pub fn single_path_share(&self) -> f64 {
        let distinct = self.emb_keys + self.bm25_keys - self.common_keys;
        if distinct == 0 {
            0.0
        } else {
            (self.emb_only_keys + self.bm25_only_keys) as f64 / distinct as f64
        }
    }
}

/// Fused-score distribution of one method, accumulated over every query.
///
/// F1 tables cannot show a change in the *scale* of the fused score — the
/// ranking (and therefore every metric) stays identical when the score
/// semantics change. This stat carries the numbers that do move: the ceiling
/// each entry can reach given its path coverage, and how many entries reach it.
#[derive(Debug, Clone, Default)]
pub struct ScoreSummary {
    pub entries: usize,
    pub dual_path: usize,
    pub vector_only: usize,
    pub bm25_only: usize,
    pub at_ceiling: usize,
    pub max_score: f64,
    pub sum_score: f64,
}

impl ScoreSummary {
    /// Record one fused entry against the weights that produced it.
    ///
    /// The ceiling is the score a key gets when every path that returned it
    /// contributes its full normalized credit: `w_v` for a vector-only key,
    /// `w_b` for a BM25-only key, `1.0` for a dual-path key (the weights sum to
    /// one). A single-path key can therefore never reach `1.0` — that is the
    /// property this counter verifies.
    pub fn observe(&mut self, entry: &FusedEntry, vector_weight: f64, bm25_weight: f64) {
        self.entries += 1;
        self.max_score = self.max_score.max(entry.fused_score);
        self.sum_score += entry.fused_score;
        let ceiling = match entry.paths {
            PathCoverage::Both => {
                self.dual_path += 1;
                vector_weight + bm25_weight
            }
            PathCoverage::VectorOnly => {
                self.vector_only += 1;
                vector_weight
            }
            PathCoverage::Bm25Only => {
                self.bm25_only += 1;
                bm25_weight
            }
        };
        if (entry.fused_score - ceiling).abs() < 1e-4 {
            self.at_ceiling += 1;
        }
    }

    pub fn mean_score(&self) -> f64 {
        if self.entries == 0 {
            0.0
        } else {
            self.sum_score / self.entries as f64
        }
    }

    /// Share of entries sitting at their path-coverage ceiling, in `[0, 1]`.
    pub fn ceiling_ratio(&self) -> f64 {
        if self.entries == 0 {
            0.0
        } else {
            self.at_ceiling as f64 / self.entries as f64
        }
    }
}

/// Score distribution baseline of one (baseline, method) pair.
pub struct ScoreStat {
    pub baseline: String,
    pub method: String,
    pub vector_weight: f64,
    pub bm25_weight: f64,
    pub summary: ScoreSummary,
}

/// Aggregation baseline of one (baseline, strategy) pair.
pub struct AggregationBaseline {
    pub baseline: String,
    pub strategy: String,
    pub stat: AggregationStat,
}

/// Aggregated result of a retrieval-method benchmark run.
pub struct BenchmarkRun {
    pub output_dir: std::path::PathBuf,
    pub results: Vec<MethodResult>,
    pub relevance: Vec<MethodRelevance>,
    pub alignment: Vec<AlignmentStat>,
    pub scores: Vec<ScoreStat>,
    pub aggregation: Vec<AggregationBaseline>,
    pub methods: Vec<String>,
}

/// Output directory for a project's retrieval-method reports.
pub fn output_dir(project: &'static str) -> std::path::PathBuf {
    BenchmarkPaths::new(project)
        .output_dir()
        .join("retrieval_method")
}

/// Run the retrieval-method benchmark for a project and write all reports.
pub fn run_retrieval_method_benchmark(
    project: &'static str,
    judgments: &[RelevanceJudgment],
) -> Result<BenchmarkRun, Box<dyn std::error::Error>> {
    let paths = BenchmarkPaths::new(project);
    let mut results = Vec::new();
    let mut relevance = Vec::new();
    let mut alignment = Vec::new();
    let mut scores = Vec::new();
    let mut aggregation = Vec::new();

    for baseline in BASELINES {
        let data_path = paths.data_dir(baseline);
        if !data_path.exists() {
            eprintln!(
                "WARNING: {baseline} data not found at {}",
                data_path.display()
            );
            continue;
        }
        println!(
            "Loading baseline '{baseline}' from: {}",
            data_path.display()
        );
        let bench = load_benchmark_data(&data_path)?;
        println!(
            "  {} queries, {} embedding chunks, {} BM25 chunks",
            bench.queries.len(),
            bench.embedding.chunks.len(),
            bench.bm25.chunks.len()
        );
        let mut evaluated = evaluate_baseline_records(baseline, &bench, judgments);
        alignment.push(collect_alignment_stat(baseline, &bench));
        results.append(&mut evaluated.methods);
        relevance.append(&mut evaluated.relevance);
        scores.append(&mut evaluated.scores);
        aggregation.append(&mut evaluated.aggregation);
    }

    if results.is_empty() {
        eprintln!("No baseline data loaded. Run `gen_bench_{project}` first.");
        return Ok(BenchmarkRun {
            output_dir: output_dir(project),
            results,
            relevance,
            alignment,
            scores,
            aggregation,
            methods: ordered_methods(),
        });
    }
    scores.sort_by(|a, b| {
        a.baseline
            .cmp(&b.baseline)
            .then_with(|| a.method.cmp(&b.method))
    });
    aggregation.sort_by(|a, b| {
        a.baseline
            .cmp(&b.baseline)
            .then_with(|| a.strategy.cmp(&b.strategy))
    });

    let out = output_dir(project);
    std::fs::create_dir_all(&out)?;
    let methods = ordered_methods();
    report::write_all(
        &out,
        project,
        judgments.len(),
        &methods,
        &alignment,
        &scores,
        &aggregation,
        &results,
        &relevance,
    )?;

    println!(
        "\n✓ Retrieval-method evaluation files written to: {}",
        out.display()
    );
    print_summary(&results);

    Ok(BenchmarkRun {
        output_dir: out,
        results,
        relevance,
        alignment,
        scores,
        aggregation,
        methods,
    })
}

/// Ordered list of every evaluated method name.
pub fn ordered_methods() -> Vec<String> {
    let mut methods = vec!["emb".to_string(), "bm25".to_string()];
    for (alpha, _) in MINMAX_WEIGHTS {
        methods.push(format!("minmax-{alpha:.1}"));
    }
    for (suffix, _) in POLICY_VARIANTS {
        methods.push(format!("minmax-{POLICY_WEIGHT:.1}{suffix}"));
    }
    for k in RRF_K_VALUES {
        methods.push(format!("rrf-{k:.0}"));
    }
    methods.push(format!("rrf-{RRF_STRICT_K:.0}-strict"));
    for (name, _) in AGGREGATION_STRATEGIES {
        methods.push((*name).to_string());
    }
    methods
}

/// Gather cross-path alignment coverage for one baseline.
///
/// Counts the **expanded** per-entity keys (a multi-entity chunk contributes
/// one key per contained entity), mirroring the entity-level aggregation used
/// by both the single-path dedup and the fusion paths. The unalignable counts
/// use the production rule, so the reported share is directly comparable with
/// the runtime alignment metric.
pub fn collect_alignment_stat(baseline: &str, bench: &BenchmarkData) -> AlignmentStat {
    use crate::retrieval_method::fusion::alignment_keys;
    let emb_keys: HashSet<String> = bench
        .embedding
        .chunks
        .iter()
        .flat_map(alignment_keys)
        .collect();
    let bm25_keys: HashSet<String> = bench.bm25.chunks.iter().flat_map(alignment_keys).collect();
    let common = emb_keys.intersection(&bm25_keys).count();
    AlignmentStat {
        baseline: baseline.to_string(),
        emb_chunks: bench.embedding.chunks.len(),
        bm25_chunks: bench.bm25.chunks.len(),
        emb_keys: emb_keys.len(),
        bm25_keys: bm25_keys.len(),
        common_keys: common,
        emb_only_keys: emb_keys.len() - common,
        bm25_only_keys: bm25_keys.len() - common,
        emb_unalignable: count_unalignable(&bench.embedding.chunks),
        bm25_unalignable: count_unalignable(&bench.bm25.chunks),
    }
}

/// Evaluate every method family for one baseline dataset.
///
/// Exposed so integration tests can exercise the full method matrix on
/// in-memory `BenchmarkData` without writing report files.
pub fn evaluate_baseline(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
) -> (Vec<MethodResult>, Vec<MethodRelevance>) {
    let records = evaluate_baseline_records(baseline, bench, judgments);
    (records.methods, records.relevance)
}

/// Everything one baseline contributes to a run.
struct BaselineRecords {
    methods: Vec<MethodResult>,
    relevance: Vec<MethodRelevance>,
    scores: Vec<ScoreStat>,
    aggregation: Vec<AggregationBaseline>,
}

/// Collects the evaluation output for one baseline.
struct MethodSink<'a> {
    results: &'a mut Vec<MethodResult>,
    relevance: &'a mut Vec<MethodRelevance>,
    /// Per `(baseline, method)`: the weights and the accumulated score summary.
    scores: &'a mut BTreeMap<(String, String), (f64, f64, ScoreSummary)>,
    /// Per aggregation strategy: accumulated multi-source overlap.
    aggregation: &'a mut BTreeMap<String, AggregationStat>,
}

/// Pre-scored recall rankings for one baseline dataset.
///
/// Shared by the retrieval-method benchmark and the rerank benchmark so both
/// consume identical recall orderings (no drift between the two reports).
pub struct RecallRankings {
    /// Per query: `(embedding chunk index, cosine score)` sorted descending.
    pub emb_ranked: Vec<Vec<(usize, f64)>>,
    /// Per query: `(BM25 chunk index, BM25 score)` sorted descending.
    pub bm25_ranked: Vec<Vec<(usize, f64)>>,
    /// Number of embedding chunks covered by vectors.
    pub n_emb_chunks: usize,
}

/// Score every query against both recall paths.
///
/// Embedding uses cosine similarity over the precomputed vectors; BM25 uses
/// the production three-field weighted scorer with the `Or` operator. Queries
/// without vectors/scores yield empty rankings, exactly as before.
pub fn rank_recall(bench: &BenchmarkData) -> RecallRankings {
    let emb_dim = bench.embedding.dimension as usize;
    let n_emb_chunks = if emb_dim > 0 {
        match bench.embedding.vectors.len().checked_div(emb_dim) {
            Some(n) => n.min(bench.embedding.chunks.len()),
            None => 0,
        }
    } else {
        0
    };
    let bm25_all = if bench.bm25_documents.is_empty() {
        Vec::new()
    } else {
        compute_bm25_scores(&bench.bm25_documents, &bench.query_texts, TermOperator::Or)
    };

    let mut emb_ranked = Vec::with_capacity(bench.queries.len());
    let mut bm25_ranked = Vec::with_capacity(bench.queries.len());
    for q_idx in 0..bench.queries.len() {
        let mut emb: Vec<(usize, f64)> = Vec::new();
        if emb_dim > 0 && bench.embedding.query_vectors.len() >= (q_idx + 1) * emb_dim {
            let q_start = q_idx * emb_dim;
            let q_vec = &bench.embedding.query_vectors[q_start..q_start + emb_dim];
            emb = (0..n_emb_chunks)
                .map(|c| {
                    let c_start = c * emb_dim;
                    let c_vec = &bench.embedding.vectors[c_start..c_start + emb_dim];
                    (c, cosine_similarity(c_vec, q_vec))
                })
                .collect();
            emb.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        emb_ranked.push(emb);

        let bm25: Vec<(usize, f64)> = bm25_all
            .get(q_idx)
            .map(|scores| {
                let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
                ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                ranked
            })
            .unwrap_or_default();
        bm25_ranked.push(bm25);
    }

    RecallRankings {
        emb_ranked,
        bm25_ranked,
        n_emb_chunks,
    }
}

/// Evaluate every method family for one baseline across its queries.
fn evaluate_baseline_records(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
) -> BaselineRecords {
    let mut results = Vec::new();
    let mut relevance = Vec::new();
    let mut score_map: BTreeMap<(String, String), (f64, f64, ScoreSummary)> = BTreeMap::new();
    let mut aggregation_stats: BTreeMap<String, AggregationStat> = BTreeMap::new();
    {
        let mut sink = MethodSink {
            results: &mut results,
            relevance: &mut relevance,
            scores: &mut score_map,
            aggregation: &mut aggregation_stats,
        };
        evaluate_baseline_methods(baseline, bench, judgments, &mut sink);
    }
    let scores = score_map
        .into_iter()
        .map(
            |((baseline, method), (vector_weight, bm25_weight, summary))| ScoreStat {
                baseline,
                method,
                vector_weight,
                bm25_weight,
                summary,
            },
        )
        .collect();
    let aggregation = aggregation_stats
        .into_iter()
        .map(|(strategy, stat)| AggregationBaseline {
            baseline: baseline.to_string(),
            strategy,
            stat,
        })
        .collect();
    BaselineRecords {
        methods: results,
        relevance,
        scores,
        aggregation,
    }
}

/// Evaluate every method family for one baseline across its queries.
fn evaluate_baseline_methods(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[RelevanceJudgment],
    sink: &mut MethodSink<'_>,
) {
    let recall = rank_recall(bench);

    for (q_idx, query_data) in bench.queries.iter().enumerate() {
        let Some(judgment) = judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };

        let Some(emb_ranked) = recall.emb_ranked.get(q_idx) else {
            continue;
        };
        let Some(bm25_ranked) = recall.bm25_ranked.get(q_idx) else {
            continue;
        };

        // Single paths are measured on the raw chunk ranking (each chunk
        // exactly once), identical to the no-aggregation baseline benchmark.
        // Alignment-key deduplication applies only to the fused methods, which
        // internally collapse per key via `best_chunk_per_key`.
        let emb_path = RankedPath::new(
            &bench.embedding.chunks[..recall.n_emb_chunks],
            dedup_ranked(emb_ranked),
        );
        let bm25_path = RankedPath::new(&bench.bm25.chunks, dedup_ranked(bm25_ranked));

        // Fusion consumes the RAW top-k rankings, mirroring the production
        // searcher which fuses the un-deduplicated recall of each path.
        let emb_path_raw = RankedPath::new(
            &bench.embedding.chunks[..recall.n_emb_chunks],
            emb_ranked.clone(),
        );
        let bm25_path_raw = RankedPath::new(&bench.bm25.chunks, bm25_ranked.clone());

        evaluate_query_methods(
            baseline,
            &QueryPaths {
                emb: emb_path,
                bm25: bm25_path,
                emb_raw: emb_path_raw,
                bm25_raw: bm25_path_raw,
            },
            query_data,
            judgment,
            sink,
        );
    }
}

/// Ranked paths for one query: the deduplicated single-path rankings used by
/// the `emb`/`bm25` methods and the raw top-k rankings fed to fusion methods
/// (production feeds fusion the un-deduplicated recall and performs per-key
/// selection internally).
struct QueryPaths<'a> {
    emb: RankedPath<'a>,
    bm25: RankedPath<'a>,
    emb_raw: RankedPath<'a>,
    bm25_raw: RankedPath<'a>,
}

/// Evaluate all method families for a single query.
fn evaluate_query_methods(
    baseline: &str,
    paths: &QueryPaths<'_>,
    query_data: &crate::bench_data::QueryData,
    judgment: &RelevanceJudgment,
    sink: &mut MethodSink<'_>,
) {
    if !paths.emb.ranked.is_empty() {
        let ranked: Vec<&ChunkData> = paths
            .emb
            .ranked
            .iter()
            .filter_map(|&(i, _)| paths.emb.chunks.get(i))
            .collect();
        record_method(baseline, "emb", query_data, judgment, &ranked, sink);
    }

    if !paths.bm25.ranked.is_empty() {
        let ranked: Vec<&ChunkData> = paths
            .bm25
            .ranked
            .iter()
            .filter_map(|&(i, _)| paths.bm25.chunks.get(i))
            .collect();
        record_method(baseline, "bm25", query_data, judgment, &ranked, sink);
    }

    // The weight-independent fusion preprocessing (alignment lookups, expanded
    // search results, per-key rank maps) is shared by every weight variant.
    let prepared = PreparedFusion::prepare(&paths.emb_raw, &paths.bm25_raw);
    for (alpha, beta) in MINMAX_WEIGHTS {
        let fused = prepared.fuse_minmax(*alpha, *beta, SinglePathPolicy::Union);
        record_fused(
            baseline,
            &format!("minmax-{alpha:.1}"),
            query_data,
            judgment,
            &fused,
            Some((*alpha, *beta)),
            sink,
        );
    }
    // What each side of the single-path switch contributes, at the balanced
    // weight so the only difference between these rows and `minmax-0.5` is the
    // policy itself.
    for (suffix, policy) in POLICY_VARIANTS {
        let fused = prepared.fuse_minmax(POLICY_WEIGHT, POLICY_WEIGHT, *policy);
        record_fused(
            baseline,
            &format!("minmax-{POLICY_WEIGHT:.1}{suffix}"),
            query_data,
            judgment,
            &fused,
            Some((POLICY_WEIGHT, POLICY_WEIGHT)),
            sink,
        );
    }
    for k in RRF_K_VALUES {
        let fused = prepared.fuse_rrf(*k, SinglePathPolicy::Union);
        record_fused(
            baseline,
            &format!("rrf-{k:.0}"),
            query_data,
            judgment,
            &fused,
            None,
            sink,
        );
    }
    let fused = prepared.fuse_rrf(RRF_STRICT_K, SinglePathPolicy::Intersection);
    record_fused(
        baseline,
        &format!("rrf-{RRF_STRICT_K:.0}-strict"),
        query_data,
        judgment,
        &fused,
        None,
        sink,
    );

    // Aggregation stand-in: the two recall paths play the role of two
    // sub-query result lists, merged on the production dedup key with the
    // production score-combination formula.
    let sources = [
        AggSource::new(paths.emb_raw.chunks, paths.emb_raw.ranked.clone()),
        AggSource::new(paths.bm25_raw.chunks, paths.bm25_raw.ranked.clone()),
    ];
    for (name, strategy) in AGGREGATION_STRATEGIES {
        let merged = merge_aggregation_sources(&sources, *strategy);
        if merged.is_empty() {
            continue;
        }
        sink.aggregation
            .entry((*name).to_string())
            .or_default()
            .observe(&merged);
        let ranked: Vec<&ChunkData> = merged.iter().map(|entry| &entry.chunk).collect();
        record_method(baseline, name, query_data, judgment, &ranked, sink);
    }
}

/// Record results for a fused (deduplicated) ranking.
///
/// Evaluation uses the entity-level `coverage` (union of both paths' best
/// chunks per key) so a hit never depends on which path's chunk production
/// fusion selected as the representative output chunk.
fn record_fused(
    baseline: &str,
    method: &str,
    query_data: &crate::bench_data::QueryData,
    judgment: &RelevanceJudgment,
    fused: &[FusedEntry],
    weights: Option<(f64, f64)>,
    sink: &mut MethodSink<'_>,
) {
    if fused.is_empty() {
        return;
    }
    // Score-scale baseline: only the size algorithms report it, because their
    // fused score is a weighted share of [0, 1] and therefore comparable
    // across runs. RRF scores are `1 / (k + rank)` sums and carry no such
    // meaning.
    if let Some((vector_weight, bm25_weight)) = weights {
        let entry = sink
            .scores
            .entry((baseline.to_string(), method.to_string()))
            .or_insert_with(|| (vector_weight, bm25_weight, ScoreSummary::default()));
        for item in fused {
            entry.2.observe(item, vector_weight, bm25_weight);
        }
    }
    let ranked: Vec<&ChunkData> = fused.iter().map(|entry| &entry.coverage).collect();
    record_method(baseline, method, query_data, judgment, &ranked, sink);
}

/// Evaluate a ranked chunk list across all top-k cutoffs in a single scan and
/// record the per-cutoff rows plus the top-k relevance detail.
fn record_method(
    baseline: &str,
    method: &str,
    query_data: &crate::bench_data::QueryData,
    judgment: &RelevanceJudgment,
    ranked_chunks: &[&ChunkData],
    sink: &mut MethodSink<'_>,
) {
    let max_cutoff = TOP_K_VALUES.iter().copied().max().unwrap_or(0);
    let mut scan = RankedScan::new(judgment, TOP_K_VALUES, ranked_chunks.len());
    let mut strong = Vec::new();
    let mut related = Vec::new();
    for (pos, &chunk) in ranked_chunks.iter().take(max_cutoff).enumerate() {
        let relevance = scan.advance(chunk);
        if pos < DEFAULT_TOP_K {
            if let Some(level) = relevance.highest_level() {
                let loc = format!(
                    "{}:{}-{}",
                    chunk.file_path, chunk.start_line, chunk.end_line
                );
                match level {
                    RelevanceLevel::Strong => strong.push((pos + 1, loc, level)),
                    RelevanceLevel::Related => related.push((pos + 1, loc, level)),
                    _ => {}
                }
            }
        }
    }

    for (&top_k, (range, diagnostics)) in TOP_K_VALUES.iter().zip(scan.finish()) {
        sink.results.push(MethodResult {
            baseline: baseline.to_string(),
            method: method.to_string(),
            query_id: query_data.id.clone(),
            query_type: judgment.query_type,
            top_k,
            score: RetrievalMethodScore {
                range,
                avg_first_hit_rank: diagnostics.avg_first_hit_rank,
                redundant_coverage: diagnostics.redundant_coverage,
            },
        });
    }

    sink.relevance.push(MethodRelevance {
        baseline: baseline.to_string(),
        method: method.to_string(),
        query_id: query_data.id.clone(),
        strong_chunks: strong,
        related_chunks: related,
    });
}

/// Print a compact top-5 console summary grouped by (baseline, method).
fn print_summary(results: &[MethodResult]) {
    use std::collections::BTreeMap;
    println!("\n=== Retrieval-Method Summary (top-5, F1_any) ===");
    let top5: Vec<_> = results.iter().filter(|r| r.top_k == 5).collect();
    let mut map: BTreeMap<(&str, &str), Vec<&MethodResult>> = BTreeMap::new();
    for r in &top5 {
        map.entry((r.baseline.as_str(), r.method.as_str()))
            .or_default()
            .push(r);
    }
    for ((baseline, method), group) in &map {
        let n = group.len() as f64;
        let precision = group
            .iter()
            .map(|r| r.score.range.precision_any)
            .sum::<f64>()
            / n;
        let recall = group.iter().map(|r| r.score.range.recall_any).sum::<f64>() / n;
        let f1 = group.iter().map(|r| r.score.range.f1_any).sum::<f64>() / n;
        println!(
            "  {baseline:22} {method:10}  P={precision:.3} R={recall:.3} F1={f1:.3} (n={})",
            group.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::ChunkSourceRange;

    fn chunk(id: &str, entity_ids: Vec<i64>, segment: &str) -> ChunkData {
        ChunkData {
            chunk_id: id.to_string(),
            entity_name: String::new(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            end_line: 10,
            source_ranges: vec![ChunkSourceRange {
                start_line: 1,
                end_line: 10,
            }],
            source_span_kind: String::new(),
            test_info: cce_types::TestInfo::unknown(),
            language: None,
            entity_ids,
            segment_id: segment.to_string(),
        }
    }

    #[test]
    fn alignment_stat_counts_expanded_entity_keys() {
        // A multi-entity chunk contributes one key per contained entity, so a
        // chunk shared by both paths counts as one common key per entity
        // (regression: it used to count only `entity_ids.first()`).
        let bench = BenchmarkData {
            queries: Vec::new(),
            query_texts: Vec::new(),
            embedding: crate::bench_data::RetrieverDataset {
                chunks: vec![
                    chunk("a_emb_0", vec![7, 8], "seg_a"),
                    chunk("b_emb_0", vec![9], "seg_b"),
                ],
                texts: Vec::new(),
                vectors: Vec::new(),
                query_vectors: Vec::new(),
                dimension: 0,
            },
            bm25: crate::bench_data::RetrieverDataset {
                chunks: vec![
                    chunk("a_bm25_0", vec![7, 8], "seg_a"),
                    chunk("c_bm25_0", vec![10], "seg_c"),
                ],
                texts: Vec::new(),
                vectors: Vec::new(),
                query_vectors: Vec::new(),
                dimension: 0,
            },
            bm25_documents: Vec::new(),
            call_edges: Vec::new(),
        };

        let stat = collect_alignment_stat("test", &bench);
        assert_eq!(stat.emb_keys, 3, "two entities + one single-entity chunk");
        assert_eq!(stat.bm25_keys, 3);
        assert_eq!(stat.common_keys, 2, "entities 7 and 8 are common");
        assert_eq!(stat.emb_only_keys, 1);
        assert_eq!(stat.bm25_only_keys, 1);
        assert_eq!(stat.emb_unalignable, 0, "every chunk carries an entity");
        assert_eq!(stat.bm25_unalignable, 0);
    }

    #[test]
    fn alignment_stat_counts_unalignable_chunks() {
        // No entity and a segment equal to the chunk id: the index's fallback
        // shape, which can never match the other path.
        let bare = chunk("group_1_emb_0", Vec::new(), "group_1_emb_0");
        let bench = BenchmarkData {
            queries: Vec::new(),
            query_texts: Vec::new(),
            embedding: crate::bench_data::RetrieverDataset {
                chunks: vec![bare, chunk("a_emb_0", vec![7], "seg_a")],
                texts: Vec::new(),
                vectors: Vec::new(),
                query_vectors: Vec::new(),
                dimension: 0,
            },
            bm25: crate::bench_data::RetrieverDataset {
                chunks: vec![chunk("a_bm25_0", vec![7], "seg_a")],
                texts: Vec::new(),
                vectors: Vec::new(),
                query_vectors: Vec::new(),
                dimension: 0,
            },
            bm25_documents: Vec::new(),
            call_edges: Vec::new(),
        };
        let stat = collect_alignment_stat("test", &bench);
        assert_eq!(stat.emb_unalignable, 1);
        assert_eq!(stat.bm25_unalignable, 0);
        assert!((stat.unalignable_ratio() - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn ordered_methods_are_unique_and_cover_every_family() {
        let methods = ordered_methods();
        let unique: HashSet<&String> = methods.iter().collect();
        assert_eq!(unique.len(), methods.len(), "method names key report rows");
        for expected in [
            "emb",
            "bm25",
            "minmax-0.5",
            "minmax-0.5-vec",
            "minmax-0.5-bm25",
            "minmax-0.5-strict",
            "rrf-60",
            "rrf-60-strict",
            "agg-max",
            "agg-noisy-or",
        ] {
            assert!(
                methods.iter().any(|m| m == expected),
                "missing method row {expected}"
            );
        }
    }

    #[test]
    fn single_path_entries_never_reach_the_full_score() {
        // Equal weights: a single-path entry tops out at its own weight share,
        // a dual-path entry reaches 1.0. This is the score-scale contract the
        // baseline table exists to protect.
        let mut summary = ScoreSummary::default();
        let entry = |paths, score| FusedEntry {
            key: "k".to_string(),
            chunk: chunk("c", vec![1], "seg"),
            fused_score: score,
            vector_score: Some(score),
            bm25_score: None,
            paths,
            coverage: chunk("c", vec![1], "seg"),
        };
        summary.observe(&entry(PathCoverage::VectorOnly, 0.5), 0.5, 0.5);
        summary.observe(&entry(PathCoverage::Bm25Only, 0.5), 0.5, 0.5);
        summary.observe(&entry(PathCoverage::Both, 1.0), 0.5, 0.5);
        assert_eq!(summary.entries, 3);
        assert_eq!(summary.at_ceiling, 3);
        assert!((summary.ceiling_ratio() - 1.0).abs() < 1e-9);
        assert!((summary.max_score - 1.0).abs() < 1e-9);

        // A single-path entry scored 1.0 is off-contract and must not count as
        // sitting at its ceiling.
        let mut regressed = ScoreSummary::default();
        regressed.observe(&entry(PathCoverage::VectorOnly, 1.0), 0.5, 0.5);
        assert_eq!(regressed.at_ceiling, 0);
        assert!((regressed.max_score - 1.0).abs() < 1e-9);
    }
}
