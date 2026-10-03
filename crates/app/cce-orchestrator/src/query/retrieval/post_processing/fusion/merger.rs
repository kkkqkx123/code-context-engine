//! Hybrid fusion merger: combines vector and BM25 results into a single ranked list.

use std::collections::HashMap;

use super::aligner::{compute_alignment_coverage, expand_multi_entity_results, FusionAlignmentStats};
use super::normalizer::{normalize_by_key, normalize_path_scores};
use crate::query::types::SearchResult;

/// Fuse two sets of search results (vector path and BM25 path) into a single
/// ranked list using weighted normalized fusion at the **alignment key** level.
///
/// Alignment key priority:
/// 1. First element of `entity_ids` for code chunks (function/class/method
///    entities). Results are normally pre-expanded so each entry carries at
///    most one entity; unexpanded input is expanded internally.
/// 2. `segment_id` for document/plain-text chunks (logical sections without entities)
///
/// # Algorithm
///
/// 1. Normalize scores within each path (vector path, BM25 path) using min-max.
/// 2. Match results across paths by alignment key (entity or segment_id).
/// 3. For each key present in both paths, select the **best-matching chunk**
///    from each path and compute:
///    `score = alpha * norm(vector_score) + beta * norm(bm25_score)`
/// 4. For keys present in only one path (if `include_single_path`), compute:
///    `score = path_weight * norm(path_score)` (partial score).
/// 5. Sort by fused score descending.
///
/// An empty recall path is treated as an empty result set rather than a bypass:
/// the surviving path still goes through the unified fusion path, so its scores
/// are normalized, weighted, and filtered by `min_score` exactly as they would
/// be when both paths are present. This keeps scoring semantics independent of
/// whether the other path happened to return nothing. When one path is entirely
/// empty the same semantics run through a dedicated fast path
/// (`fuse_single_path`) that skips the union key set and the empty-side
/// aggregation instead of materializing both sides of the union.
///
/// # Arguments
///
/// * `vector_results` - Results from the vector recall path (embedded chunks)
/// * `bm25_results` - Results from the BM25 recall path (BM25 chunks)
/// * `config` - Fusion configuration (weights, thresholds)
///
/// # Returns
///
/// Fused and sorted list of search results, grouped by alignment key.
pub fn fuse_hybrid_results(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &super::HybridFusionConfig,
) -> Vec<SearchResult> {
    fuse_hybrid_results_with_stats(vector_results, bm25_results, config).0
}

/// Like [`fuse_hybrid_results`], but also returns the cross-path alignment
/// coverage statistics so callers can record the metric without recomputing it.
pub fn fuse_hybrid_results_with_stats(
    vector_results: Vec<SearchResult>,
    bm25_results: Vec<SearchResult>,
    config: &super::HybridFusionConfig,
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
    let alpha = config.vector_weight;
    let beta = config.bm25_weight;

    // Step 1+2: Aggregate each path to a single best-raw-score entry per
    // alignment key, then min-max normalize over the key set. Normalizing at
    // key granularity (rather than over every returned chunk) keeps long
    // entities with many fragments from stretching the min/max range and
    // compressing the normalized scores of single-fragment entities, matching
    // the granularity at which fusion actually combines scores.
    let (vector_by_key, bm25_by_key) = normalize_path_scores(&vector_results, &bm25_results);

    // Step 3: Fuse results by alignment key
    let mut all_keys: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for key in vector_by_key.keys() {
        if seen.insert(key.clone()) {
            all_keys.push(key.clone());
        }
    }
    if config.include_single_path {
        for key in bm25_by_key.keys() {
            if seen.insert(key.clone()) {
                all_keys.push(key.clone());
            }
        }
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
        "Hybrid fusion alignment coverage"
    );

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
                beta,
                false,
                config,
                stats,
            );
        }
        return fuse_single_path(
            &vector_results,
            |r| r.vector_score,
            alpha,
            true,
            config,
            stats,
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

        let fused_score = alpha * v_norm + beta * b_norm;

        if fused_score < config.min_score {
            continue;
        }

        let mut result = base_result;
        result.score = fused_score;
        result.original_score = fused_score;
        result.sources = vec!["hybrid".to_string()];

        fused.push(result);
    }

    let mut fused = dedup_by_chunk_id(fused, config.dedup_by_chunk);
    sort_fused_by_score(&mut fused);

    (fused, stats)
}

/// Score the surviving recall path when the other path returned nothing.
///
/// Semantics match the dual-path loop exactly for this shape: the surviving
/// path is aggregated to one best entry per alignment key, min-max
/// normalized, weighted, `min_score`-filtered, and marked as `hybrid`.
/// Skipped is only the dead work: no union key set, no aggregation or
/// normalization of the empty side, no per-key empty-side lookups.
/// `from_vector` selects which side's score field and weight apply, mirroring
/// the single-path arms of the dual-path loop.
fn fuse_single_path(
    surviving: &[SearchResult],
    raw_score: impl Fn(&SearchResult) -> f32,
    weight: f32,
    from_vector: bool,
    config: &super::HybridFusionConfig,
    stats: FusionAlignmentStats,
) -> (Vec<SearchResult>, FusionAlignmentStats) {
    if !config.include_single_path {
        return (Vec::new(), stats);
    }
    let by_key = normalize_by_key(super::aligner::best_per_key(surviving, raw_score));
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
        let fused_score = weight * norm;
        if fused_score < config.min_score {
            continue;
        }
        base.score = fused_score;
        base.original_score = fused_score;
        base.sources = vec!["hybrid".to_string()];
        fused.push(base);
    }
    let mut fused = dedup_by_chunk_id(fused, config.dedup_by_chunk);
    sort_fused_by_score(&mut fused);
    (fused, stats)
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
