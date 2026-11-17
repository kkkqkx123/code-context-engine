//! Hybrid fusion merger: combines vector and BM25 results into a single ranked list.

use std::collections::HashMap;

use super::HybridFusionConfig;
use super::aligner::{
    FusionAlignmentStats, best_per_key, compute_alignment_coverage, expand_multi_entity_results,
};
use super::normalizer::{normalize_by_key, normalize_path_scores};
use crate::query::types::{EntityScore, RecallPath, SearchResult, SecondaryEvidence};
use cce_config::modules::search::RecallFusionAlgorithm;

/// Trait for hybrid fusion algorithms.
///
/// Each algorithm implements its own score combination logic while sharing
/// the common pipeline (alignment keys, coverage stats, dedup, sorting).
pub trait FusionAlgorithmImpl {
    /// Fuse two result sets into a single ranked list.
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        bm25_results: Vec<SearchResult>,
        config: &HybridFusionConfig,
        stats: FusionAlignmentStats,
    ) -> (Vec<SearchResult>, FusionAlignmentStats);
}

/// Factory function to create a fusion algorithm instance from config.
pub fn create_fusion_algorithm(config: &HybridFusionConfig) -> Box<dyn FusionAlgorithmImpl> {
    match &config.algorithm {
        RecallFusionAlgorithm::WeightedMinMax => Box::new(WeightedMinMaxFuser),
        RecallFusionAlgorithm::Rrf { k } => Box::new(RrfFuser { k: *k }),
        RecallFusionAlgorithm::BordaCount => Box::new(BordaFuser),
    }
}

/// Fuse two sets of search results (vector path and BM25 path) into a single
/// ranked list using weighted fusion at the **alignment key** level.
///
/// Alignment key priority:
/// 1. First element of `entity_ids` for code chunks (function/class/method
///    entities). Results are normally pre-expanded so each entry carries at
///    most one entity; unexpanded input is expanded internally.
/// 2. `segment_id` for document/plain-text chunks (logical sections without entities)
///
/// How per-key scores combine is selected by
/// [`RecallFusionAlgorithm`](cce_config::modules::search::RecallFusionAlgorithm):
/// normalized weighted sum (default), reciprocal rank fusion, or weight-aware
/// Borda count. Keys present in only one path contribute that path's weighted
/// term alone (subject to the per-path single-key switches). Results are
/// sorted by fused score descending with deterministic tie-breaking.
///
/// An empty recall path is treated as an empty result set rather than a
/// bypass: the surviving path still goes through the unified fusion formula,
/// so a key scores the same whether the other path returned nothing or
/// returned unrelated hits. The fused score is always
/// `(w_v * n_v + w_b * n_b) / (w_v + w_b)` — the share of the available
/// weighted evidence a key earned — so a single-path key caps at its own
/// weight share instead of reaching full marks. When one path is entirely
/// empty the score-based algorithms run through a dedicated fast path
/// (`fuse_single_path`) that skips the union key set and the empty-side
/// aggregation while keeping that formula.
///
/// # Arguments
///
/// * `vector_results` - Results from the vector recall path (embedded chunks)
/// * `bm25_results` - Results from the BM25 recall path (BM25 chunks)
/// * `config` - Fusion configuration (weights, algorithm, thresholds)
///
/// # Returns
///
/// Fused and sorted list of search results, grouped by alignment key.
pub fn fuse_hybrid_results(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
) -> Vec<SearchResult> {
    fuse_hybrid_results_with_stats(vector_results, bm25_results, config).0
}

/// Like [`fuse_hybrid_results`], but also returns the cross-path alignment
/// coverage statistics so callers can record the metric without recomputing it.
pub fn fuse_hybrid_results_with_stats(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    if vector_results.is_empty() && bm25_results.is_empty() {
        return (Vec::new(), FusionAlignmentStats::default());
    }
    // Contract: results must be pre-expanded so each entry carries at most one
    // entity (the searcher runs `expand_multi_entity_results` before fusion).
    // Unexpanded input would make the alignment key `e:{id}` ambiguous, so it
    // is expanded here defensively instead of silently picking
    // `entity_ids.first()` — a warning keeps the contract violation visible.
    // Shared by every algorithm so rank-based paths observe the same key space
    // as score-based paths.
    let vector_needs_expand = vector_results.iter().any(|r| r.entity_ids.len() > 1);
    let bm25_needs_expand = bm25_results.iter().any(|r| r.entity_ids.len() > 1);
    if vector_needs_expand || bm25_needs_expand {
        tracing::warn!(
            vector_unexpanded = vector_needs_expand,
            bm25_unexpanded = bm25_needs_expand,
            "Hybrid fusion received unexpanded multi-entity results; \
             expanding internally to keep alignment keys unambiguous"
        );
    }
    let vector_results = if vector_needs_expand {
        expand_multi_entity_results(vector_results)
    } else {
        vector_results
    };
    let bm25_results = if bm25_needs_expand {
        expand_multi_entity_results(bm25_results)
    } else {
        bm25_results
    };

    // Fusion min_score is hygiene only; business threshold lives in the final
    // result filter. A non-trivial fusion threshold hides which stage drops
    // candidates, so surface it loudly.
    if config.min_score > 0.05 {
        tracing::warn!(
            min_score = config.min_score,
            "fusion min_score above hygiene level; business threshold belongs in result.min_score"
        );
    }
    // Observe cross-path alignment coverage so silent degradation (e.g. all
    // keys single-path) stays visible instead of blending into a ranked list.
    let stats = compute_alignment_coverage(&vector_results, &bm25_results);
    tracing::trace!(
        vector_keys = stats.vector_keys,
        bm25_keys = stats.bm25_keys,
        matched_keys = stats.matched_keys,
        vector_only = stats.vector_keys - stats.matched_keys,
        bm25_only = stats.bm25_keys - stats.matched_keys,
        unalignable = stats.unalignable(),
        unalignable_ratio = stats.unalignable_ratio(),
        "Hybrid fusion alignment coverage"
    );
    // A key that can never align makes the other path's evidence invisible for
    // that entry. Chunk-id fallback is intentional, so a low share is normal;
    // a high share means the index stopped carrying entity/segment fields.
    if stats.unalignable_ratio() > 0.5 {
        tracing::warn!(
            unalignable = stats.unalignable(),
            ratio = stats.unalignable_ratio(),
            "Most hybrid candidates carry no cross-path alignment key; \
             hybrid fusion degenerates to a union of single-path rankings"
        );
    }

    let algorithm = create_fusion_algorithm(config);
    algorithm.fuse(vector_results, bm25_results, config, stats)
}

/// Union of alignment keys across both paths in deterministic (sorted) order.
///
/// Keys present in both paths always participate; the per-path switches only
/// decide whether that path's lone keys join. They are therefore symmetric at
/// the configuration level (disabling both leaves exactly the intersection),
/// while `require_strict_intersection` states that intent explicitly and takes
/// precedence.
fn union_keys<V, W>(
    vector_by_key: &HashMap<String, V>,
    bm25_by_key: &HashMap<String, W>,
    config: &HybridFusionConfig,
) -> Vec<String> {
    let strict_intersection = config.require_strict_intersection;
    let include_vector = config.include_vector_only_keys;
    let include_bm25 = config.include_bm25_only_keys;
    let mut seen = std::collections::HashSet::new();
    let mut all_keys: Vec<String> = Vec::new();
    for key in vector_by_key.keys() {
        // Matched keys are never gated; only vector-only keys consult the switch.
        if (include_vector || bm25_by_key.contains_key(key)) && seen.insert(key.clone()) {
            all_keys.push(key.clone());
        }
    }
    for key in bm25_by_key.keys() {
        if (include_bm25 || vector_by_key.contains_key(key)) && seen.insert(key.clone()) {
            all_keys.push(key.clone());
        }
    }
    if strict_intersection {
        all_keys.retain(|key| vector_by_key.contains_key(key) && bm25_by_key.contains_key(key));
    }
    all_keys.sort();
    all_keys
}

/// Collapse per-chunk duplicates and sort by fused score descending.
fn finish_fused(fused: Vec<SearchResult>, dedup_by_chunk: bool) -> Vec<SearchResult> {
    let mut fused = group_by_chunk(fused, dedup_by_chunk);
    sort_fused_by_score(&mut fused);
    fused
}

/// Weighted min-max fusion algorithm implementation.
pub struct WeightedMinMaxFuser;

impl FusionAlgorithmImpl for WeightedMinMaxFuser {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        bm25_results: Vec<SearchResult>,
        config: &HybridFusionConfig,
        stats: FusionAlignmentStats,
    ) -> (Vec<SearchResult>, FusionAlignmentStats) {
        fuse_weighted_minmax(vector_results, bm25_results, config, stats)
    }
}

/// Reciprocal rank fusion algorithm implementation.
pub struct RrfFuser {
    k: u32,
}

impl FusionAlgorithmImpl for RrfFuser {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        bm25_results: Vec<SearchResult>,
        config: &HybridFusionConfig,
        stats: FusionAlignmentStats,
    ) -> (Vec<SearchResult>, FusionAlignmentStats) {
        fuse_rrf(vector_results, bm25_results, config, self.k, stats)
    }
}

/// Weight-aware Borda count algorithm implementation.
pub struct BordaFuser;

impl FusionAlgorithmImpl for BordaFuser {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        bm25_results: Vec<SearchResult>,
        config: &HybridFusionConfig,
        stats: FusionAlignmentStats,
    ) -> (Vec<SearchResult>, FusionAlignmentStats) {
        fuse_borda(vector_results, bm25_results, config, stats)
    }
}

/// Weighted min-max fusion (the default algorithm): normalize each path's
/// per-key best scores to [0, 1], then take the weighted linear combination
/// `score = (alpha * norm(vector) + beta * norm(bm25)) / (alpha + beta)`.
/// A key seen on one path only scores that path's weighted term, so it caps at
/// its own weight share — the same formula the empty-path fast path uses.
/// Per-path score fields on the output carry the normalized values so they
/// stay comparable with `score`.
fn fuse_weighted_minmax(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    let alpha = config.vector_weight;
    let beta = config.bm25_weight;

    // One path empty: the surviving path goes through the same weighted
    // formula via the fast path, which skips the union key set and the dead
    // work of normalizing an empty side.
    if vector_results.is_empty() || bm25_results.is_empty() {
        tracing::trace!(
            vector_results = vector_results.len(),
            bm25_results = bm25_results.len(),
            "Hybrid fusion single-path mode: one recall path returned no results; \
             scores follow the same weighted formula as dual-path mode"
        );
        if vector_results.is_empty() {
            return fuse_single_path(
                &bm25_results,
                |r| r.bm25_score.unwrap_or(0.0),
                false,
                config,
                stats,
            );
        }
        return fuse_single_path(&vector_results, |r| r.vector_score, true, config, stats);
    }

    // Aggregate each path to a single best-raw-score entry per alignment
    // key, then min-max normalize over the key set. Normalizing at key
    // granularity (rather than over every returned chunk) keeps long
    // entities with many fragments from stretching the min/max range and
    // compressing the normalized scores of single-fragment entities,
    // matching the granularity at which fusion actually combines scores.
    let (vector_path, bm25_path) = normalize_path_scores(&vector_results, &bm25_results);
    if vector_path.degenerate || bm25_path.degenerate {
        tracing::trace!(
            vector_degenerate = vector_path.degenerate,
            bm25_degenerate = bm25_path.degenerate,
            "Hybrid fusion normalized a path without score spread; \
             its keys share full credit and carry no ordering signal"
        );
    }
    let vector_by_key = &vector_path.scores;
    let bm25_by_key = &bm25_path.scores;

    let all_keys = union_keys(vector_by_key, bm25_by_key, config);

    let mut fused: Vec<SearchResult> = Vec::with_capacity(all_keys.len());

    for key in all_keys {
        let vec_entry = vector_by_key.get(&key);
        let bm25_entry = bm25_by_key.get(&key);

        let (base_result, v_norm, b_norm) = match (vec_entry, bm25_entry) {
            (Some(&(vi, vn)), Some(&(bi, bn))) => {
                // Pick the path with the larger weighted contribution as the
                // output chunk so the id/content/line fields belong to the same
                // hit that dominates the fused score (no cross-chunk identity
                // mixing). The losing hit stays attached as a reference so the
                // other path's evidence stays attributable. Per-path scores are
                // stored normalized to [0, 1] to keep them comparable with the
                // fused `score`.
                let v_contrib = alpha * vn;
                let b_contrib = beta * bn;
                let mut base = if v_contrib >= b_contrib {
                    let mut base = vector_results[vi].clone();
                    base.secondary_evidence =
                        Some(secondary_evidence(&bm25_results[bi], RecallPath::Bm25, bn));
                    base
                } else {
                    let mut base = bm25_results[bi].clone();
                    base.secondary_evidence = Some(secondary_evidence(
                        &vector_results[vi],
                        RecallPath::Vector,
                        vn,
                    ));
                    base
                };
                base.vector_score = vn;
                base.bm25_score = Some(bn);
                (base, vn, bn)
            }
            (Some(&(vi, vn)), None) => {
                let mut base = vector_results[vi].clone();
                if config.include_vector_only_keys {
                    base.vector_score = vn;
                    (base, vn, 0.0)
                } else {
                    continue;
                }
            }
            (None, Some(&(bi, bn))) => {
                let mut base = bm25_results[bi].clone();
                if config.include_bm25_only_keys {
                    base.vector_score = 0.0;
                    base.bm25_score = Some(bn);
                    (base, 0.0, bn)
                } else {
                    continue;
                }
            }
            (None, None) => {
                unreachable!("all_keys only contains keys present in at least one path")
            }
        };

        let fused_score = fused_score(alpha, beta, v_norm, b_norm);

        if fused_score < config.min_score {
            continue;
        }

        let mut result = base_result;
        result.score = fused_score;
        result.original_score = fused_score;
        result.sources = vec!["hybrid".to_string()];

        fused.push(result);
    }

    (finish_fused(fused, config.dedup_by_chunk), stats)
}

/// Fused score on the `[0, 1]` evidence-strength scale.
///
/// A key's score is the share of the available weighted evidence it earned:
/// both paths' best key reaches 1.0, a single-path key caps at its own weight
/// share, and a key with no evidence on either path never reaches this
/// function. Dividing by the fixed weight sum (instead of the weights present
/// in a given query) is what keeps the scale independent of whether the other
/// path happened to return anything.
fn fused_score(alpha: f32, beta: f32, v_norm: f32, b_norm: f32) -> f32 {
    let total = alpha + beta;
    if total <= f32::EPSILON {
        return 0.0;
    }
    (alpha * v_norm + beta * b_norm) / total
}

/// Reference to the losing path's representative hit.
fn secondary_evidence(
    result: &SearchResult,
    path: RecallPath,
    normalized_score: f32,
) -> SecondaryEvidence {
    SecondaryEvidence {
        path,
        chunk_id: result.id.clone(),
        normalized_score,
        raw_score: match path {
            RecallPath::Vector => result.vector_score,
            RecallPath::Bm25 => result.bm25_score.unwrap_or(0.0),
        },
        start_line: result.start_line,
        end_line: result.end_line,
    }
}

/// Rank pair for a single key: (rank, score) from vector and BM25 respectively.
type RankPair<'a> = (Option<&'a (usize, u32)>, Option<&'a (usize, u32)>);

/// Rank context shared by rank-based fusion algorithms (RRF, Borda).
///
/// Encapsulates the per-path ranking and key union so individual algorithms
/// only implement their score combination logic.
struct RankContext {
    vector_ranks: HashMap<String, (usize, u32)>,
    bm25_ranks: HashMap<String, (usize, u32)>,
    all_keys: Vec<String>,
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
}

impl RankContext {
    fn new(
        vector_results: Vec<SearchResult>,
        bm25_results: Vec<SearchResult>,
        config: &HybridFusionConfig,
    ) -> Self {
        let vector_ranks = rank_by_key(&vector_results, |r| r.vector_score);
        let bm25_ranks = rank_by_key(&bm25_results, |r| r.bm25_score.unwrap_or(0.0));
        let all_keys = union_keys(&vector_ranks, &bm25_ranks, config);
        Self {
            vector_ranks,
            bm25_ranks,
            all_keys,
            vector_results,
            bm25_results,
        }
    }

    fn get(&self, key: &str) -> RankPair<'_> {
        (self.vector_ranks.get(key), self.bm25_ranks.get(key))
    }

    fn vector_result(&self, index: usize) -> &SearchResult {
        &self.vector_results[index]
    }

    fn bm25_result(&self, index: usize) -> &SearchResult {
        &self.bm25_results[index]
    }

    fn vector_key_count(&self) -> usize {
        self.vector_ranks.len()
    }

    fn bm25_key_count(&self) -> usize {
        self.bm25_ranks.len()
    }
}

/// Reciprocal Rank Fusion: rank-based fusion that ignores raw score
/// distributions, making it robust to min-max's sensitivity to outliers and
/// compressed score ranges.
///
/// Each path's keys are ranked by their best raw score (rank starts at 1,
/// ties broken by alignment key for determinism) and fused as
/// `score = w_v/(k+rank_v) + w_b/(k+rank_b)`; a key present in only one path
/// (when that path's single-key switch allows it) gets that path's term alone. The raw per-path
/// scores are kept on the result fields untouched — only `score`/`original_score`
/// carry the RRF value, so `min_score` is interpreted on the RRF scale
/// `(0, (w_v+w_b)/(k+1)]`.
fn fuse_rrf(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
    k: u32,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    let alpha = config.vector_weight;
    let beta = config.bm25_weight;
    let k = k.max(1) as f32;

    let ctx = RankContext::new(vector_results, bm25_results, config);
    let mut fused: Vec<SearchResult> = Vec::with_capacity(ctx.all_keys.len());

    for key in &ctx.all_keys {
        let (vec_rank, bm25_rank) = ctx.get(key);

        let (base, rrf_score) = match (vec_rank, bm25_rank) {
            (Some(&(vi, rv)), Some(&(bi, rb))) => {
                let v_term = alpha / (k + rv as f32);
                let b_term = beta / (k + rb as f32);
                // Per-path normalized contribution ((k + 1) / (k + rank)),
                // in (0, 1], for the secondary evidence reference.
                let v_norm = (k + 1.0) / (k + rv as f32);
                let b_norm = (k + 1.0) / (k + rb as f32);
                let base = if v_term >= b_term {
                    let mut b = ctx.vector_result(vi).clone();
                    b.bm25_score = ctx.bm25_result(bi).bm25_score;
                    b.secondary_evidence = Some(secondary_evidence(
                        ctx.bm25_result(bi),
                        RecallPath::Bm25,
                        b_norm,
                    ));
                    b
                } else {
                    let mut b = ctx.bm25_result(bi).clone();
                    b.vector_score = ctx.vector_result(vi).vector_score;
                    b.secondary_evidence = Some(secondary_evidence(
                        ctx.vector_result(vi),
                        RecallPath::Vector,
                        v_norm,
                    ));
                    b
                };
                (base, v_term + b_term)
            }
            (Some(&(vi, rv)), None) if config.include_vector_only_keys => {
                let base = ctx.vector_result(vi).clone();
                (base, alpha / (k + rv as f32))
            }
            (None, Some(&(bi, rb))) if config.include_bm25_only_keys => {
                let base = ctx.bm25_result(bi).clone();
                (base, beta / (k + rb as f32))
            }
            _ => continue,
        };

        let max_score = (alpha + beta) / (k + 1.0);
        let normalized_score = rrf_score / max_score;

        if normalized_score < config.min_score {
            continue;
        }
        let mut result = base;
        result.score = normalized_score;
        result.original_score = normalized_score;
        result.sources = vec!["hybrid".to_string()];
        fused.push(result);
    }

    (finish_fused(fused, config.dedup_by_chunk), stats)
}

/// Weight-aware Borda count: rank-based fusion with linear (instead of
/// hyperbolic) rank decay.
///
/// Each path ranks its alignment keys by best raw score exactly like RRF.
/// With `m` keys on a path, the key at 1-based rank `r` earns `m - r + 1`
/// points (best key earns `m`, worst earns 1), normalized by `m` to
/// `(m - r + 1) / m ∈ (0, 1]`; the fused score is
/// `score = w_v * norm_points_v + w_b * norm_points_b`, with a missing path
/// contributing zero (single-path keys keep their own path's normalized
/// points when that path's single-key switch allows it). Normalization pins the score
/// scale to `[0, w_v + w_b]` regardless of how many keys a query returns, so
/// `min_score` carries the same meaning across queries and algorithms (it
/// matches the weighted min-max scale). Like RRF it ignores raw score
/// magnitudes, but awards linearly instead of hyperbolically, so deep ranks
/// keep contributing instead of decaying toward zero as in RRF, which suits
/// recall-heavy queries where the tail still matters. Raw per-path scores
/// stay untouched on the result fields.
fn fuse_borda(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    let alpha = config.vector_weight;
    let beta = config.bm25_weight;

    let ctx = RankContext::new(vector_results, bm25_results, config);
    let vector_points = ctx.vector_key_count() as f32;
    let bm25_points = ctx.bm25_key_count() as f32;

    let mut fused: Vec<SearchResult> = Vec::with_capacity(ctx.all_keys.len());
    for key in &ctx.all_keys {
        let (vec_rank, bm25_rank) = ctx.get(key);

        let (base, borda_score) = match (vec_rank, bm25_rank) {
            (Some(&(vi, rv)), Some(&(bi, rb))) => {
                let v_norm_points = (vector_points - rv as f32 + 1.0) / vector_points;
                let b_norm_points = (bm25_points - rb as f32 + 1.0) / bm25_points;
                let v_term = alpha * v_norm_points;
                let b_term = beta * b_norm_points;
                let base = if v_term >= b_term {
                    let mut b = ctx.vector_result(vi).clone();
                    b.bm25_score = ctx.bm25_result(bi).bm25_score;
                    b.secondary_evidence = Some(secondary_evidence(
                        ctx.bm25_result(bi),
                        RecallPath::Bm25,
                        b_norm_points,
                    ));
                    b
                } else {
                    let mut b = ctx.bm25_result(bi).clone();
                    b.vector_score = ctx.vector_result(vi).vector_score;
                    b.secondary_evidence = Some(secondary_evidence(
                        ctx.vector_result(vi),
                        RecallPath::Vector,
                        v_norm_points,
                    ));
                    b
                };
                (base, v_term + b_term)
            }
            (Some(&(vi, rv)), None) if config.include_vector_only_keys => {
                let base = ctx.vector_result(vi).clone();
                let v_norm_points = (vector_points - rv as f32 + 1.0) / vector_points;
                (base, alpha * v_norm_points)
            }
            (None, Some(&(bi, rb))) if config.include_bm25_only_keys => {
                let base = ctx.bm25_result(bi).clone();
                let b_norm_points = (bm25_points - rb as f32 + 1.0) / bm25_points;
                (base, beta * b_norm_points)
            }
            _ => continue,
        };

        let normalized_score = borda_score / (alpha + beta);

        if normalized_score < config.min_score {
            continue;
        }
        let mut result = base;
        result.score = normalized_score;
        result.original_score = normalized_score;
        result.sources = vec!["hybrid".to_string()];
        fused.push(result);
    }

    (finish_fused(fused, config.dedup_by_chunk), stats)
}

/// Rank each path's alignment keys by their best raw score.
///
/// Returns `key -> (result index, rank)` with ranks starting at 1; ties on
/// score resolve on the stable content order of the representative hits, so
/// rank-based fusion stays identical across index rebuilds.
fn rank_by_key(
    results: &[SearchResult],
    raw_score: impl Fn(&SearchResult) -> f32,
) -> HashMap<String, (usize, u32)> {
    let mut by_key: Vec<(String, (usize, f32))> =
        best_per_key(results, raw_score).into_iter().collect();
    by_key.sort_by(|a, b| {
        b.1.1
            .partial_cmp(&a.1.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| results[a.1.0].cmp_stable(&results[b.1.0]))
    });
    by_key
        .into_iter()
        .enumerate()
        .map(|(i, (key, (idx, _)))| (key, (idx, (i + 1) as u32)))
        .collect()
}

/// Score the surviving recall path when the other path returned nothing.
///
/// Semantics match the dual-path loop exactly for this shape: the surviving
/// path is aggregated to one best entry per alignment key, min-max normalized,
/// weighted, `min_score`-filtered, and marked as `hybrid`. The score is
/// `w * norm / (alpha + beta)` — the same evidence-share formula the dual-path
/// loop applies to single-path keys — so a key does not score differently just
/// because the other path returned nothing. Skipped is only the dead work: no
/// union key set, no aggregation or normalization of the empty side.
/// `from_vector` selects which side's score field and weight apply.
///
/// Each side honours its own single-key switch here too, so the fast path and
/// the union key set agree on which side's lone keys survive.
fn fuse_single_path(
    surviving: &[SearchResult],
    raw_score: impl Fn(&SearchResult) -> f32,
    from_vector: bool,
    config: &HybridFusionConfig,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    if config.require_strict_intersection {
        return (Vec::new(), stats);
    }
    if from_vector && !config.include_vector_only_keys {
        return (Vec::new(), stats);
    }
    if !from_vector && !config.include_bm25_only_keys {
        return (Vec::new(), stats);
    }
    let best = best_per_key(surviving, raw_score);
    let path = normalize_by_key(best);
    if path.degenerate {
        tracing::trace!(
            from_vector,
            keys = path.len(),
            "Single-path fusion normalized a degenerate path; keys share full credit"
        );
    }
    let mut keys: Vec<&String> = path.scores.keys().collect();
    keys.sort();
    let mut fused = Vec::with_capacity(path.len());
    for key in keys {
        let (idx, norm) = path.scores[key];
        let mut base = surviving[idx].clone();
        if from_vector {
            base.vector_score = norm;
        } else {
            base.vector_score = 0.0;
            base.bm25_score = Some(norm);
        }
        let (v_norm, b_norm) = if from_vector {
            (norm, 0.0)
        } else {
            (0.0, norm)
        };
        let fused_score = fused_score(config.vector_weight, config.bm25_weight, v_norm, b_norm);
        if fused_score < config.min_score {
            continue;
        }
        base.score = fused_score;
        base.original_score = fused_score;
        base.sources = vec!["hybrid".to_string()];
        fused.push(base);
    }
    (finish_fused(fused, config.dedup_by_chunk), stats)
}

/// Group entries pointing at the same physical chunk (its id) onto one entry.
///
/// Entity-level alignment surfaces the same chunk once per contained entity.
/// Grouping keeps one entry per chunk — so multi-entity chunks do not inflate
/// the result list with identical content — while the sibling entities and
/// their fused scores are recorded on `entity_scores` instead of being
/// dropped. The representative (`entity_ids[0]`, id, body) is the
/// highest-scoring entity, which keeps the single-entity contract that later
/// key derivation and enrichment depend on.
fn group_by_chunk(results: Vec<SearchResult>, enabled: bool) -> Vec<SearchResult> {
    if !enabled {
        return results;
    }
    let mut best_by_chunk: HashMap<String, SearchResult> = HashMap::new();
    let mut entity_scores: HashMap<String, Vec<EntityScore>> = HashMap::new();
    for result in results {
        if let Some(&entity_id) = result.entity_ids.first() {
            entity_scores
                .entry(result.id.clone())
                .or_default()
                .push(EntityScore {
                    entity_id,
                    score: result.score,
                });
        }
        best_by_chunk
            .entry(result.id.clone())
            .and_modify(|existing| {
                if result.score > existing.score {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
    }
    for (chunk_id, mut scores) in entity_scores {
        if scores.len() < 2 {
            continue;
        }
        // Highest-scoring entity first so the representative stays readable.
        scores.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.entity_id.0.cmp(&b.entity_id.0))
        });
        if let Some(entry) = best_by_chunk.get_mut(&chunk_id) {
            entry.entity_scores = scores;
        }
    }
    best_by_chunk.into_values().collect()
}

/// Sort by fused score descending, with deterministic tie-breaking.
/// Ties arise when a path yields few distinct raw scores (min-max then maps
/// them onto few discrete normalized values, e.g. two results -> {0, 1}) or
/// when raw scores coincide (multi-entity expansion duplicates scores).
/// Ties resolve on stable content coordinates, never on index-assigned
/// identifiers, so the order survives index rebuilds instead of following
/// identifier allocation order.
fn sort_fused_by_score(fused: &mut [SearchResult]) {
    fused.sort_by(|a, b| a.cmp_score_desc_then_stable(b));
}
