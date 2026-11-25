//! Aggregation-enhance benchmark orchestration.
//!
//! Full-pipeline baseline only. For every query the three base rankings
//! (`emb`, `bm25`, `minmax-0.5`) are computed exactly like the existing
//! benchmarks, then the offline relation/summary boosts from [`enhance`]
//! are applied per method. Only the `emb*` family matches the production
//! boost path (dense recall); `bm25*` and `minmax-0.5*` boosted rows are
//! exploratory and labeled as deviating from production in the manifest.

use std::collections::HashMap;

use crate::aggregation_enhance::enhance::{
    EnhanceParams, FileCohortGraph, FileVectors, apply_additive_boosts, primary_entity,
    relation_boosts, summary_boosts,
};
use crate::bench_data::{BenchmarkData, ChunkData, RelevanceLevel};
use crate::judgments::evaluate::{BenchmarkPaths, DEFAULT_TOP_K, TOP_K_VALUES};
use crate::range_evaluator::RankedScan;
use crate::retrieval_method::benchmark::{
    MethodRelevance, MethodResult, RetrievalMethodScore, rank_recall,
};
use crate::retrieval_method::fusion::{PreparedFusion, RankedPath, SinglePathPolicy, dedup_ranked};

use super::report;

/// The only evaluated baseline in this benchmark.
pub const BASELINE: &str = "full_pipeline";

/// Fixed balanced hybrid representative (not a weight sweep).
pub const MINMAX_WEIGHT: (f64, f64) = (0.5, 0.5);

/// Ordered list of every evaluated method name.
pub fn ordered_methods() -> Vec<String> {
    let mut methods = Vec::new();
    for base in ["emb", "bm25", "minmax-0.5"] {
        methods.push(base.to_string());
        methods.push(format!("{base}+rel"));
        methods.push(format!("{base}+sum"));
        methods.push(format!("{base}+both"));
    }
    methods
}

/// Whether a method row follows the production boost path.
///
/// Only dense (`emb`) recall collects boosts in production; hybrid recall
/// fuses without boosting and pure BM25 recall never boosts.
pub fn is_production_parity(method: &str) -> bool {
    method == "emb" || method == "emb+rel" || method == "emb+sum" || method == "emb+both"
}

/// Offline signal coverage for the manifest.
#[derive(Debug, Default)]
pub struct SignalStats {
    pub files: usize,
    pub entities: usize,
    pub emb_chunks: usize,
    pub bm25_chunks: usize,
}

/// Aggregated result of an aggregation-enhance benchmark run.
pub struct BenchmarkRun {
    pub output_dir: std::path::PathBuf,
    pub results: Vec<MethodResult>,
    pub relevance: Vec<MethodRelevance>,
    pub methods: Vec<String>,
    pub signals: SignalStats,
    pub skipped: Vec<String>,
}

/// Output directory for a project's aggregation-enhance reports.
pub fn output_dir(project: &'static str) -> std::path::PathBuf {
    BenchmarkPaths::new(project)
        .output_dir()
        .join("aggregation_enhance")
}

/// Run the aggregation-enhance benchmark for a project and write reports.
///
/// Loads only the `full_pipeline` dataset and reuses its vectors/texts.
pub fn run_aggregation_enhance_benchmark(
    project: &'static str,
    judgments: &[crate::bench_data::RelevanceJudgment],
) -> Result<BenchmarkRun, Box<dyn std::error::Error>> {
    let paths = BenchmarkPaths::new(project);
    let data_path = paths.data_dir(BASELINE);
    if !data_path.exists() {
        eprintln!("No baseline data loaded. Run `gen_bench_{project}` first.");
        return Ok(BenchmarkRun {
            output_dir: output_dir(project),
            results: Vec::new(),
            relevance: Vec::new(),
            methods: ordered_methods(),
            signals: SignalStats::default(),
            skipped: vec![format!("missing {}", data_path.display())],
        });
    }
    println!(
        "Loading baseline '{BASELINE}' from: {}",
        data_path.display()
    );
    let bench = crate::bench_data::load_benchmark_data(&data_path)?;
    println!(
        "  {} queries, {} embedding chunks, {} BM25 chunks",
        bench.queries.len(),
        bench.embedding.chunks.len(),
        bench.bm25.chunks.len()
    );

    let params = EnhanceParams::default();
    let (results, relevance, signals, mut skipped) =
        evaluate_baseline(BASELINE, &bench, judgments, &params);

    let out = output_dir(project);
    std::fs::create_dir_all(&out)?;
    let methods = ordered_methods();
    skipped.sort();
    skipped.dedup();
    report::write_all(
        &out,
        project,
        judgments.len(),
        &methods,
        &results,
        &relevance,
        &signals,
        &skipped,
        &params,
    )?;

    println!(
        "\n✓ Aggregation-enhance evaluation files written to: {}",
        out.display()
    );
    print_summary(&results);

    Ok(BenchmarkRun {
        output_dir: out,
        results,
        relevance,
        methods,
        signals,
        skipped,
    })
}

/// Evaluate the full method matrix for the baseline dataset.
pub fn evaluate_baseline(
    baseline: &str,
    bench: &BenchmarkData,
    judgments: &[crate::bench_data::RelevanceJudgment],
    params: &EnhanceParams,
) -> (
    Vec<MethodResult>,
    Vec<MethodRelevance>,
    SignalStats,
    Vec<String>,
) {
    let mut results = Vec::new();
    let mut relevance = Vec::new();
    let mut skipped = Vec::new();

    let all_refs: Vec<&ChunkData> = bench
        .embedding
        .chunks
        .iter()
        .chain(bench.bm25.chunks.iter())
        .collect();
    let graph = FileCohortGraph::build(&all_refs);
    let file_vectors = FileVectors::build(
        &bench.embedding.chunks,
        &bench.embedding.vectors,
        bench.embedding.dimension as usize,
    );
    let signals = SignalStats {
        files: file_vectors
            .as_ref()
            .map_or(graph.file_count(), |f| f.file_count()),
        entities: graph.entity_count(),
        emb_chunks: bench.embedding.chunks.len(),
        bm25_chunks: bench.bm25.chunks.len(),
    };
    if graph.entity_count() == 0 {
        skipped.push("relation graph empty: *+rel and *+both skipped".to_string());
    }
    if file_vectors.is_none() {
        skipped.push("file vectors unavailable: *+sum and *+both skipped".to_string());
    }

    let recall = rank_recall(bench);
    let ctx = QueryContext {
        baseline,
        bench,
        judgments,
        params,
        graph: &graph,
        file_vectors: file_vectors.as_ref(),
        recall: &recall,
        results: &mut results,
        relevance: &mut relevance,
        skipped: &mut skipped,
    };
    evaluate_all_queries(ctx);

    (results, relevance, signals, skipped)
}

struct QueryContext<'a> {
    baseline: &'a str,
    bench: &'a BenchmarkData,
    judgments: &'a [crate::bench_data::RelevanceJudgment],
    params: &'a EnhanceParams,
    graph: &'a FileCohortGraph,
    file_vectors: Option<&'a FileVectors>,
    recall: &'a crate::retrieval_method::benchmark::RecallRankings,
    results: &'a mut Vec<MethodResult>,
    relevance: &'a mut Vec<MethodRelevance>,
    skipped: &'a mut Vec<String>,
}

fn evaluate_all_queries(mut ctx: QueryContext<'_>) {
    for (q_idx, query_data) in ctx.bench.queries.iter().enumerate() {
        let Some(judgment) = ctx.judgments.iter().find(|j| j.id == query_data.id) else {
            continue;
        };
        let (Some(emb_ranked), Some(bm25_ranked)) = (
            ctx.recall.emb_ranked.get(q_idx),
            ctx.recall.bm25_ranked.get(q_idx),
        ) else {
            continue;
        };

        let emb_chunks = &ctx.bench.embedding.chunks[..ctx.recall.n_emb_chunks];
        let bm25_chunks = &ctx.bench.bm25.chunks[..];
        let query_vector = embedding_query_vector(ctx.bench, q_idx);

        if !emb_ranked.is_empty() {
            let base = dedup_ranked(emb_ranked);
            evaluate_single_path(
                &mut ctx,
                "emb",
                query_data,
                judgment,
                emb_chunks,
                &base,
                query_vector,
            );
        } else {
            ctx.skipped.push("emb ranking empty".to_string());
        }

        if !bm25_ranked.is_empty() {
            let base = dedup_ranked(bm25_ranked);
            evaluate_single_path(
                &mut ctx,
                "bm25",
                query_data,
                judgment,
                bm25_chunks,
                &base,
                query_vector,
            );
        } else {
            ctx.skipped.push("bm25 ranking empty".to_string());
        }

        let emb_raw = RankedPath::new(emb_chunks, emb_ranked.clone());
        let bm25_raw = RankedPath::new(bm25_chunks, bm25_ranked.clone());
        if !emb_ranked.is_empty() && !bm25_ranked.is_empty() {
            let prepared = PreparedFusion::prepare(&emb_raw, &bm25_raw);
            let fused =
                prepared.fuse_minmax(MINMAX_WEIGHT.0, MINMAX_WEIGHT.1, SinglePathPolicy::Union);
            if fused.is_empty() {
                ctx.skipped.push("minmax-0.5 fusion empty".to_string());
            } else {
                evaluate_fused(&mut ctx, query_data, judgment, &fused, query_vector);
            }
        }
    }
}

fn embedding_query_vector(bench: &BenchmarkData, q_idx: usize) -> &[f32] {
    let dim = bench.embedding.dimension as usize;
    if dim == 0 || bench.embedding.query_vectors.len() < (q_idx + 1) * dim {
        return &[];
    }
    let start = q_idx * dim;
    &bench.embedding.query_vectors[start..start + dim]
}

fn relation_cap(params: &EnhanceParams) -> f32 {
    params.relation.max_boost
}

fn summary_cap(params: &EnhanceParams) -> f32 {
    params.agg.cap_for("summary")
}

#[allow(clippy::too_many_arguments)]
fn evaluate_single_path(
    ctx: &mut QueryContext<'_>,
    base: &str,
    query_data: &crate::bench_data::QueryData,
    judgment: &crate::bench_data::RelevanceJudgment,
    chunks: &[ChunkData],
    base_ranked: &[(usize, f64)],
    query_vector: &[f32],
) {
    let ranked: Vec<&ChunkData> = base_ranked
        .iter()
        .filter_map(|&(i, _)| chunks.get(i))
        .collect();
    record_method(ctx, base, query_data, judgment, &ranked);

    let rel = relation_boosts(base_ranked, chunks, ctx.graph, ctx.params);
    let sum = match ctx.file_vectors {
        Some(files) if !query_vector.is_empty() => {
            summary_boosts(query_vector, files, chunks, base_ranked, ctx.params)
        }
        _ => HashMap::new(),
    };
    let want_rel = !rel.is_empty();
    let want_sum = !sum.is_empty();
    if ctx.graph.entity_count() == 0 {
        // Reason already recorded globally.
    } else if !want_rel {
        ctx.skipped
            .push(format!("{base}+rel: no related entities for some queries"));
    }
    if ctx.file_vectors.is_none() || query_vector.is_empty() {
        // Reason already recorded globally.
    } else if !want_sum {
        ctx.skipped
            .push(format!("{base}+sum: no summary files above threshold"));
    }

    let id_of = |i: usize| {
        chunks
            .get(i)
            .map(|c| c.chunk_id.clone())
            .unwrap_or_default()
    };
    if want_rel {
        let boosted = apply_additive_boosts(
            base_ranked,
            &[(&rel, relation_cap(ctx.params))],
            ctx.params.agg.max_addition,
            &id_of,
        );
        let ranked: Vec<&ChunkData> = boosted.iter().filter_map(|&(i, _)| chunks.get(i)).collect();
        record_method(ctx, &format!("{base}+rel"), query_data, judgment, &ranked);
    }
    if want_sum {
        let boosted = apply_additive_boosts(
            base_ranked,
            &[(&sum, summary_cap(ctx.params))],
            ctx.params.agg.max_addition,
            &id_of,
        );
        let ranked: Vec<&ChunkData> = boosted.iter().filter_map(|&(i, _)| chunks.get(i)).collect();
        record_method(ctx, &format!("{base}+sum"), query_data, judgment, &ranked);
    }
    if want_rel && want_sum {
        let boosted = apply_additive_boosts(
            base_ranked,
            &[
                (&rel, relation_cap(ctx.params)),
                (&sum, summary_cap(ctx.params)),
            ],
            ctx.params.agg.max_addition,
            &id_of,
        );
        let ranked: Vec<&ChunkData> = boosted.iter().filter_map(|&(i, _)| chunks.get(i)).collect();
        record_method(ctx, &format!("{base}+both"), query_data, judgment, &ranked);
    }
}

fn evaluate_fused(
    ctx: &mut QueryContext<'_>,
    query_data: &crate::bench_data::QueryData,
    judgment: &crate::bench_data::RelevanceJudgment,
    fused: &[crate::retrieval_method::fusion::FusedEntry],
    query_vector: &[f32],
) {
    let base_ranked: Vec<(usize, f64)> = fused
        .iter()
        .enumerate()
        .map(|(i, e)| (i, e.fused_score))
        .collect();
    let ranked: Vec<&ChunkData> = fused.iter().map(|e| &e.coverage).collect();
    record_method(ctx, "minmax-0.5", query_data, judgment, &ranked);

    // Seeds are the top-N fused entries; entity identity comes from the
    // alignment key (`e:{id}`) with a fallback to the representative chunk.
    let top_n = ctx.params.relation.top_n.min(fused.len());
    let mut order: Vec<usize> = (0..fused.len()).collect();
    order.sort_by(|&a, &b| fused[b].fused_score.total_cmp(&fused[a].fused_score));
    let seeds: Vec<i64> = order[..top_n]
        .iter()
        .filter_map(|&i| fused_entity(&fused[i]))
        .collect();
    let related = if seeds.is_empty() {
        HashMap::new()
    } else {
        crate::aggregation_enhance::enhance::expand_cohort(
            ctx.graph,
            &seeds,
            ctx.params.relation.max_hops,
        )
    };
    let mut rel: HashMap<usize, f64> = HashMap::new();
    for (i, entry) in fused.iter().enumerate() {
        if let Some(entity) = fused_entity(entry)
            && let Some(&hops) = related.get(&entity)
        {
            let value = ctx.params.relation.max_boost as f64 / (hops as f64).sqrt();
            if value > 0.0 {
                rel.insert(i, value);
            }
        }
    }

    let mut sum: HashMap<usize, f64> = HashMap::new();
    if let Some(files) = ctx.file_vectors
        && !query_vector.is_empty()
    {
        let min_score = ctx.params.summary.min_score as f64;
        let mut file_scores: Vec<(&str, f64)> = files
            .files
            .iter()
            .zip(files.vectors.iter())
            .map(|(file, vec)| {
                (
                    file.as_str(),
                    crate::bench_data::cosine_similarity(&vec.to_vec(), query_vector),
                )
            })
            .filter(|(_, s)| *s >= min_score)
            .collect();
        file_scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        file_scores.truncate(ctx.params.summary.top_k);
        let matched: HashMap<&str, f64> = file_scores.into_iter().collect();
        for (i, entry) in fused.iter().enumerate() {
            if let Some(&score) = matched.get(entry.coverage.file_path.as_str()) {
                let norm = ((score - min_score) / (1.0 - min_score)).clamp(0.0, 1.0);
                let value = ctx.params.agg.cap_for("summary") as f64 * norm;
                if value > 0.0 {
                    sum.insert(i, value);
                }
            }
        }
    }

    let want_rel = !rel.is_empty();
    let want_sum = !sum.is_empty();
    if !want_rel {
        ctx.skipped
            .push("minmax-0.5+rel: no related entities".to_string());
    }
    if !want_sum {
        ctx.skipped
            .push("minmax-0.5+sum: no summary files above threshold".to_string());
    }
    let id_of = |i: usize| {
        fused
            .get(i)
            .map(|e| e.chunk.chunk_id.clone())
            .unwrap_or_default()
    };
    let apply = |map: &HashMap<usize, f64>, cap: f32| {
        apply_additive_boosts(
            &base_ranked,
            &[(map, cap)],
            ctx.params.agg.max_addition,
            &id_of,
        )
    };
    if want_rel {
        let boosted = apply(&rel, relation_cap(ctx.params));
        let ranked: Vec<&ChunkData> = boosted
            .iter()
            .filter_map(|&(i, _)| fused.get(i).map(|e| &e.coverage))
            .collect();
        record_method(ctx, "minmax-0.5+rel", query_data, judgment, &ranked);
    }
    if want_sum {
        let boosted = apply(&sum, summary_cap(ctx.params));
        let ranked: Vec<&ChunkData> = boosted
            .iter()
            .filter_map(|&(i, _)| fused.get(i).map(|e| &e.coverage))
            .collect();
        record_method(ctx, "minmax-0.5+sum", query_data, judgment, &ranked);
    }
    if want_rel && want_sum {
        let boosted = apply_additive_boosts(
            &base_ranked,
            &[
                (&rel, relation_cap(ctx.params)),
                (&sum, summary_cap(ctx.params)),
            ],
            ctx.params.agg.max_addition,
            &id_of,
        );
        let ranked: Vec<&ChunkData> = boosted
            .iter()
            .filter_map(|&(i, _)| fused.get(i).map(|e| &e.coverage))
            .collect();
        record_method(ctx, "minmax-0.5+both", query_data, judgment, &ranked);
    }
}

fn fused_entity(entry: &crate::retrieval_method::fusion::FusedEntry) -> Option<i64> {
    if let Some(rest) = entry.key.strip_prefix("e:")
        && let Ok(id) = rest.parse::<i64>()
    {
        return Some(id);
    }
    primary_entity(&entry.chunk)
}

/// Evaluate a ranked chunk list across all top-k cutoffs in a single scan.
fn record_method(
    ctx: &mut QueryContext<'_>,
    method: &str,
    query_data: &crate::bench_data::QueryData,
    judgment: &crate::bench_data::RelevanceJudgment,
    ranked_chunks: &[&ChunkData],
) {
    let max_cutoff = TOP_K_VALUES.iter().copied().max().unwrap_or(0);
    let mut scan = RankedScan::new(judgment, TOP_K_VALUES, ranked_chunks.len());
    let mut strong = Vec::new();
    let mut related = Vec::new();
    for (pos, &chunk) in ranked_chunks.iter().take(max_cutoff).enumerate() {
        let relevance = scan.advance(chunk);
        if pos < DEFAULT_TOP_K
            && let Some(level) = relevance.highest_level()
        {
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

    for (&top_k, (range, diagnostics)) in TOP_K_VALUES.iter().zip(scan.finish()) {
        ctx.results.push(MethodResult {
            baseline: ctx.baseline.to_string(),
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

    ctx.relevance.push(MethodRelevance {
        baseline: ctx.baseline.to_string(),
        method: method.to_string(),
        query_id: query_data.id.clone(),
        strong_chunks: strong,
        related_chunks: related,
    });
}

/// Print a compact top-5 console summary grouped by method.
fn print_summary(results: &[MethodResult]) {
    use std::collections::BTreeMap;
    println!("\n=== Aggregation-Enhance Summary (top-5, F1_any) ===");
    let top5: Vec<_> = results.iter().filter(|r| r.top_k == 5).collect();
    let mut map: BTreeMap<&str, Vec<&MethodResult>> = BTreeMap::new();
    for r in &top5 {
        map.entry(r.method.as_str()).or_default().push(r);
    }
    for (method, group) in &map {
        let n = group.len() as f64;
        let mark = if is_production_parity(method) {
            ""
        } else {
            " (exploratory)"
        };
        let precision = group
            .iter()
            .map(|r| r.score.range.precision_any)
            .sum::<f64>()
            / n;
        let recall = group.iter().map(|r| r.score.range.recall_any).sum::<f64>() / n;
        let f1 = group.iter().map(|r| r.score.range.f1_any).sum::<f64>() / n;
        println!(
            "  {method:16}  P={precision:.3} R={recall:.3} F1={f1:.3} (n={}){mark}",
            group.len()
        );
    }
}
