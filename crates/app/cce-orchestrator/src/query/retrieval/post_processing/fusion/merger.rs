//! Hybrid fusion merger: combines vector and BM25 results into a single ranked list.

use std::collections::HashMap;

use super::aligner::{
    FusionAlignmentStats, alignment_key, best_per_key, compute_alignment_coverage,
    expand_multi_entity_results,
};
use super::normalizer::{normalize_by_key, normalize_path_scores};
use super::HybridFusionConfig;
use crate::query::types::SearchResult;
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
/// normalized weighted sum (default), raw weighted sum, reciprocal rank
/// fusion, or weight-aware Borda count. Keys present in only one path
/// contribute that path's term alone when `include_single_path` is set.
/// Results are sorted by fused score descending with deterministic
/// tie-breaking.
///
/// An empty recall path is treated as an empty result set rather than a bypass:
/// the surviving path still goes through the unified fusion path, so its scores
/// are combined, weighted, and filtered by `min_score` exactly as they would
/// be when both paths are present. This keeps scoring semantics independent of
/// whether the other path happened to return nothing. When one path is entirely
/// empty the score-based algorithms run through a dedicated fast path
/// (`fuse_single_path`) that skips the union key set and the empty-side
/// aggregation instead of materializing both sides of the union.
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
        return (
            Vec::new(),
            FusionAlignmentStats {
                vector_keys: 0,
                bm25_keys: 0,
                matched_keys: 0,
            },
        );
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

    // Observe cross-path alignment coverage so silent degradation (e.g. all
    // keys single-path) stays visible instead of blending into a ranked list.
    let stats = compute_alignment_coverage(&vector_results, &bm25_results);
    tracing::trace!(
        vector_keys = stats.vector_keys,
        bm25_keys = stats.bm25_keys,
        matched_keys = stats.matched_keys,
        vector_only = stats.vector_keys - stats.matched_keys,
        bm25_only = stats.bm25_keys - stats.matched_keys,
        "Hybrid fusion alignment coverage"
    );

    let algorithm = create_fusion_algorithm(config);
    algorithm.fuse(vector_results, bm25_results, config, stats)
}

/// Union of alignment keys across both paths in deterministic (sorted) order.
///
/// Vector keys always seed the union; BM25-only keys join only when
/// `include_single_path` is set, so excluding single-path hits removes the
/// BM25-only tail while keeping single-path vector recall.
fn union_keys<V, W>(
    vector_by_key: &HashMap<String, V>,
    bm25_by_key: &HashMap<String, W>,
    include_single_path: bool,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut all_keys: Vec<String> = Vec::new();
    for key in vector_by_key.keys() {
        if seen.insert(key.clone()) {
            all_keys.push(key.clone());
        }
    }
    if include_single_path {
        for key in bm25_by_key.keys() {
            if seen.insert(key.clone()) {
                all_keys.push(key.clone());
            }
        }
    }
    all_keys.sort();
    all_keys
}

/// Collapse per-chunk duplicates and sort by fused score descending.
fn finish_fused(fused: Vec<SearchResult>, dedup_by_chunk: bool) -> Vec<SearchResult> {
    let mut fused = dedup_by_chunk_id(fused, dedup_by_chunk);
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
/// `score = alpha * norm(vector) + beta * norm(bm25)`. A key seen on one
/// path only scores that path's weighted term. Per-path score fields on the
/// output carry the normalized values so they stay comparable with `score`.
fn fuse_weighted_minmax(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &HybridFusionConfig,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    let alpha = config.vector_weight;
    let beta = config.bm25_weight;

    // Aggregate each path to a single best-raw-score entry per alignment
    // key, then min-max normalize over the key set. Normalizing at key
    // granularity (rather than over every returned chunk) keeps long
    // entities with many fragments from stretching the min/max range and
    // compressing the normalized scores of single-fragment entities,
    // matching the granularity at which fusion actually combines scores.
    let (vector_by_key, bm25_by_key) = normalize_path_scores(&vector_results, &bm25_results);

    // Step 3: Fuse results by alignment key
    let all_keys = union_keys(&vector_by_key, &bm25_by_key, config.include_single_path);

    if vector_results.is_empty() || bm25_results.is_empty() {
        tracing::trace!(
            vector_results = vector_results.len(),
            bm25_results = bm25_results.len(),
            "Hybrid fusion single-path mode: one recall path returned no results; \
             scores are weighted and min_score-filtered as in dual-path mode"
        );
        if vector_results.is_empty() {
            return fuse_single_path(
                &bm25_results,
                |r| r.bm25_score.unwrap_or(0.0),
                false,
                config,
                stats,
                true,
            );
        }
        return fuse_single_path(
            &vector_results,
            |r| r.vector_score,
            true,
            config,
            stats,
            true,
        );
    }

    let mut fused: Vec<SearchResult> = Vec::with_capacity(all_keys.len());

    for key in all_keys {
        let vec_entry = vector_by_key.get(&key);
        let bm25_entry = bm25_by_key.get(&key);

        let (base_result, v_norm, b_norm) = match (vec_entry, bm25_entry) {
            (Some(&(vi, vn)), Some(&(bi, bn))) => {
                // Pick the path with the larger weighted contribution as the
                // output chunk so the id/content/line fields belong to the same
                // hit that dominates the fused score (no cross-chunk identity
                // mixing). Per-path scores are stored normalized to [0, 1] to
                // keep them comparable with the fused `score`.
                let v_contrib = alpha * vn;
                let b_contrib = beta * bn;
                let mut base = if v_contrib >= b_contrib {
                    vector_results[vi].clone()
                } else {
                    bm25_results[bi].clone()
                };
                base.vector_score = vn;
                base.bm25_score = Some(bn);
                (base, vn, bn)
            }
            (Some(&(vi, vn)), None) => {
                let mut base = vector_results[vi].clone();
                if config.include_single_path {
                    base.vector_score = vn;
                    (base, vn, 0.0)
                } else {
                    continue;
                }
            }
            (None, Some(&(bi, bn))) => {
                let mut base = bm25_results[bi].clone();
                if config.include_single_path {
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

        let raw_score = alpha * v_norm + beta * b_norm;
        let fused_score = raw_score / (alpha + beta);

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
        include_single_path: bool,
    ) -> Self {
        let vector_ranks = rank_by_key(&vector_results, |r| r.vector_score);
        let bm25_ranks = rank_by_key(&bm25_results, |r| r.bm25_score.unwrap_or(0.0));
        let all_keys = union_keys(&vector_ranks, &bm25_ranks, include_single_path);
        Self {
            vector_ranks,
            bm25_ranks,
            all_keys,
            vector_results,
            bm25_results,
        }
    }

    fn get(
        &self,
        key: &str,
    ) -> (Option<&(usize, u32)>, Option<&(usize, u32)>) {
        (
            self.vector_ranks.get(key),
            self.bm25_ranks.get(key),
        )
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
/// (when `include_single_path`) gets that path's term alone. The raw per-path
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

    let ctx = RankContext::new(vector_results, bm25_results, config.include_single_path);
    let mut fused: Vec<SearchResult> = Vec::with_capacity(ctx.all_keys.len());

    for key in &ctx.all_keys {
        let (vec_rank, bm25_rank) = ctx.get(key);

        let (base, rrf_score) = match (vec_rank, bm25_rank) {
            (Some(&(vi, rv)), Some(&(bi, rb))) => {
                let v_term = alpha / (k + rv as f32);
                let b_term = beta / (k + rb as f32);
                let base = if v_term >= b_term {
                    let mut b = ctx.vector_result(vi).clone();
                    b.bm25_score = ctx.bm25_result(bi).bm25_score;
                    b
                } else {
                    let mut b = ctx.bm25_result(bi).clone();
                    b.vector_score = ctx.vector_result(vi).vector_score;
                    b
                };
                (base, v_term + b_term)
            }
            (Some(&(vi, rv)), None) if config.include_single_path => {
                let base = ctx.vector_result(vi).clone();
                (base, alpha / (k + rv as f32))
            }
            (None, Some(&(bi, rb))) if config.include_single_path => {
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
/// points when `include_single_path` is set). Normalization pins the score
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

    let ctx = RankContext::new(vector_results, bm25_results, config.include_single_path);
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
                    b
                } else {
                    let mut b = ctx.bm25_result(bi).clone();
                    b.vector_score = ctx.vector_result(vi).vector_score;
                    b
                };
                (base, v_term + b_term)
            }
            (Some(&(vi, rv)), None) if config.include_single_path => {
                let base = ctx.vector_result(vi).clone();
                let v_norm_points = (vector_points - rv as f32 + 1.0) / vector_points;
                (base, alpha * v_norm_points)
            }
            (None, Some(&(bi, rb))) if config.include_single_path => {
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
/// score are broken by key so the ranking is deterministic.
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
            .then_with(|| a.0.cmp(&b.0))
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
/// path is aggregated to one best entry per alignment key, optionally
/// min-max normalized (`normalize`), weighted, `min_score`-filtered, and
/// marked as `hybrid`. Skipped is only the dead work: no union key set, no
/// aggregation or normalization of the empty side, no per-key empty-side
/// lookups. `from_vector` selects which side's score field and weight apply,
/// mirroring the single-path arms of the dual-path loop.
///
/// The `include_single_path` switch keeps its asymmetric meaning here: the
/// vector path is the primary semantic recall, so a surviving vector path is
/// always kept and the switch is consulted only when the surviving path is
/// BM25. This matches `union_keys` (vector keys seed the union) and the RRF
/// and Borda paths.
fn fuse_single_path(
    surviving: &[SearchResult],
    raw_score: impl Fn(&SearchResult) -> f32,
    from_vector: bool,
    config: &HybridFusionConfig,
    stats: FusionAlignmentStats,
    normalize: bool,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    if !config.include_single_path && !from_vector {
        return (Vec::new(), stats);
    }
    let best = best_per_key(surviving, raw_score);
    let by_key = if normalize {
        normalize_by_key(best)
    } else {
        best
    };
    let mut keys: Vec<&String> = by_key.keys().collect();
    keys.sort();
    let mut fused = Vec::with_capacity(by_key.len());
    for key in keys {
        let (idx, norm) = by_key[key];
        let mut base = surviving[idx].clone();
        if from_vector {
            base.vector_score = norm;
        } else {
            base.vector_score = 0.0;
            base.bm25_score = Some(norm);
        }
        let fused_score = norm;
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

/// Collapse entries pointing at the same physical chunk (its id) to the
/// best-scoring one. Entity-level alignment can otherwise surface the same
/// chunk once per contained entity.
fn dedup_by_chunk_id(results: Vec<SearchResult>, enabled: bool) -> Vec<SearchResult> {
    if !enabled {
        return results;
    }
    let mut best_by_chunk: HashMap<String, SearchResult> = HashMap::new();
    for result in results {
        best_by_chunk
            .entry(result.id.clone())
            .and_modify(|existing| {
                if result.score > existing.score {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
    }
    best_by_chunk.into_values().collect()
}

/// Sort by fused score descending, with deterministic tie-breaking.
/// Ties arise when a path yields few distinct raw scores (min-max then maps
/// them onto few discrete normalized values, e.g. two results -> {0, 1}) or
/// when raw scores coincide (multi-entity expansion duplicates scores);
/// without a stable secondary key the order would depend on HashMap
/// iteration order and differ across requests.
fn sort_fused_by_score(fused: &mut [SearchResult]) {
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_key = (
                    a.entity_ids.first().map(|e| e.0),
                    a.segment_id.clone().unwrap_or_default(),
                    a.id.clone(),
                );
                let b_key = (
                    b.entity_ids.first().map(|e| e.0),
                    b.segment_id.clone().unwrap_or_default(),
                    b.id.clone(),
                );
                a_key.cmp(&b_key)
            })
    });
}
