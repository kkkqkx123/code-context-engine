//! Hybrid retrieval fusion module
//!
//! Provides weighted normalized score fusion for combining results from
//! two independent recall paths (vector + BM25). This module performs
//! min-max normalization within each path's results, then computes a
//! weighted linear combination.
//!
//! This approach preserves score magnitude information and allows
//! per-query-intent weight customization.

use std::collections::HashMap;

use crate::query::types::SearchResult;

/// Configuration for hybrid vector + BM25 fusion
#[derive(Debug, Clone)]
pub struct HybridFusionConfig {
    /// Weight assigned to normalized vector scores [0.0, 1.0]
    pub vector_weight: f32,
    /// Weight assigned to normalized BM25 scores [0.0, 1.0]
    pub bm25_weight: f32,
    /// Whether to include items that only appear in one path
    pub include_single_path: bool,
    /// Minimum fused score threshold
    pub min_score: f32,
    /// Whether to keep at most one result per physical chunk after fusion.
    ///
    /// Entity-level alignment can surface the same chunk once per contained
    /// entity; enabling this collapses them to the best-scoring entry per
    /// `id` (chunk id). Defaults to `true` so multi-entity chunks do not
    /// inflate the result list with identical content.
    pub dedup_by_chunk: bool,
}

impl Default for HybridFusionConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: true,
        }
    }
}

/// Normalize a slice of scores to [0.0, 1.0] using min-max normalization.
///
/// Returns the normalized scores. If all scores are equal, returns all 1.0.
/// If the input is empty, returns an empty vec.
pub fn minmax_normalize(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }

    let min = scores.iter().copied().fold(f32::MAX, f32::min);
    let max = scores.iter().copied().fold(f32::MIN, f32::max);

    let range = max - min;
    if range <= f32::EPSILON {
        return vec![1.0; scores.len()];
    }

    scores.iter().map(|&s| (s - min) / range).collect()
}

/// Aggregate a result list into one best-raw-score entry per alignment key.
///
/// Keys derive from [`alignment_key`]; results without a key are skipped.
/// Returns `key -> (result index, raw score)`.
fn best_per_key(
    results: &[SearchResult],
    score: impl Fn(&SearchResult) -> f32,
) -> HashMap<String, (usize, f32)> {
    let mut map: HashMap<String, (usize, f32)> = HashMap::new();
    for (i, r) in results.iter().enumerate() {
        let Some(key) = alignment_key(&r.entity_ids, r.segment_id.as_deref(), &r.id) else {
            continue;
        };
        let s = score(r);
        let entry = map.entry(key).or_insert((i, s));
        if s > entry.1 {
            *entry = (i, s);
        }
    }
    map
}

/// Normalize the per-key best scores with min-max, returning
/// `key -> (result index, normalized score)`.
///
/// Keys are sorted before pairing so the order of the normalized values does
/// not depend on HashMap iteration order.
fn normalize_by_key(map: HashMap<String, (usize, f32)>) -> HashMap<String, (usize, f32)> {
    let mut entries: Vec<(String, (usize, f32))> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let raw: Vec<f32> = entries.iter().map(|(_, (_, s))| *s).collect();
    let normalized = minmax_normalize(&raw);
    entries
        .into_iter()
        .zip(normalized)
        .map(|((key, (i, _)), n)| (key, (i, n)))
        .collect()
}

/// Derive the cross-path alignment key for a search result.
///
/// Priority: the first entity in `entity_ids` for code chunks, `segment_id`
/// for document/plain-text chunks, chunk id as the final fallback so unkeyed
/// results survive as individual single-path entries instead of collapsing
/// onto one shared key or being dropped entirely. Returns `None` only when
/// every key source is empty.
///
/// The fallback uses the raw chunk id without normalization: the embedding
/// and BM25 paths chunk independently, so their ids for the "same" logical
/// block intentionally differ and must not be force-aligned by string surgery
/// at read time. Any chunk reaching this branch lacks both entity and segment
/// identity; the index side guarantees a non-empty `segment_id` at write time
/// (see `storage_coordinator.rs`), so this branch should be effectively
/// unreachable in production. When it does fire we log instead of silently
/// degrading.
///
/// Results are expected to be pre-expanded so `entity_ids` holds at most one
/// element for code chunks and the alignment key resolves to a single entity;
/// fusion expands unexpanded input defensively before deriving keys.
///
/// Exposed so the aggregation dedup in `query/coordinator.rs` derives the same
/// key format as hybrid fusion instead of duplicating it with a bare `{id}`.
pub(crate) fn alignment_key(
    entity_ids: &[cce_types::EntityId],
    segment_id: Option<&str>,
    chunk_id: &str,
) -> Option<String> {
    match entity_ids.first() {
        Some(eid) => Some(format!("e:{}", eid.0)),
        None => segment_id
            .filter(|s| !s.is_empty())
            .map(|s| format!("s:{}", s))
            .or_else(|| {
                if chunk_id.is_empty() {
                    None
                } else {
                    tracing::trace!(
                        chunk_id,
                        "Hybrid alignment key fell back to raw chunk id (no entity_id/segment_id)"
                    );
                    Some(format!("c:{}", chunk_id))
                }
            }),
    }
}

/// Cross-path alignment coverage between the vector and BM25 result sets.
#[derive(Debug, Clone, Copy)]
pub struct FusionAlignmentStats {
    /// Number of distinct alignment keys on the vector path.
    pub vector_keys: usize,
    /// Number of distinct alignment keys on the BM25 path.
    pub bm25_keys: usize,
    /// Number of keys present in both paths.
    pub matched_keys: usize,
}

/// Compute cross-path alignment coverage for a pair of result sets.
///
/// Mirrors the key derivation used inside `fuse_hybrid_results`. Exposed so
/// callers (and the searcher's metrics) can observe silent degradation such as
/// zero matched keys, where hybrid fusion degenerates to a union of two
/// single-path rankings.
pub fn compute_alignment_coverage(
    vector_results: &[SearchResult],
    bm25_results: &[SearchResult],
) -> FusionAlignmentStats {
    fn keys(results: &[SearchResult]) -> std::collections::HashSet<String> {
        results
            .iter()
            .filter_map(|r| alignment_key(&r.entity_ids, r.segment_id.as_deref(), &r.id))
            .collect()
    }
    let vector_keys = keys(vector_results);
    let bm25_keys = keys(bm25_results);
    let matched_keys = vector_keys.intersection(&bm25_keys).count();
    FusionAlignmentStats {
        vector_keys: vector_keys.len(),
        bm25_keys: bm25_keys.len(),
        matched_keys,
    }
}

/// Expand multi-entity results into single-entity results for entity-level fusion.
///
/// A single chunk may contain multiple entities. Before hybrid fusion, we expand
/// such results so each entity gets its own entry with the same score. This enables
/// entity-level alignment in fusion instead of chunk-level alignment.
///
/// Results with 0 or 1 entity_ids are passed through unchanged.
///
/// Public so the recall benchmark can exercise the exact production expansion
/// step before invoking `fuse_hybrid_results`; fusion itself also calls this
/// defensively when handed unexpanded input.
pub fn expand_multi_entity_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut expanded = Vec::with_capacity(results.len());
    for result in results {
        if result.entity_ids.len() > 1 {
            for entity_id in &result.entity_ids {
                let mut clone = result.clone();
                clone.entity_ids = vec![*entity_id];
                expanded.push(clone);
            }
        } else {
            expanded.push(result);
        }
    }
    expanded
}

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
/// whether the other path happened to return nothing.
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
    if vector_results.is_empty() || bm25_results.is_empty() {
        tracing::trace!(
            vector_results = vector_results.len(),
            bm25_results = bm25_results.len(),
            "Hybrid fusion single-path mode: one recall path returned no results; \
             scores are weighted and min_score-filtered as in dual-path mode"
        );
    }

    let alpha = config.vector_weight;
    let beta = config.bm25_weight;

    // Step 1+2: Aggregate each path to a single best-raw-score entry per
    // alignment key, then min-max normalize over the key set. Normalizing at
    // key granularity (rather than over every returned chunk) keeps long
    // entities with many fragments from stretching the min/max range and
    // compressing the normalized scores of single-fragment entities, matching
    // the granularity at which fusion actually combines scores.
    let vector_by_key = normalize_by_key(best_per_key(&vector_results, |r| r.vector_score));
    let bm25_by_key =
        normalize_by_key(best_per_key(&bm25_results, |r| r.bm25_score.unwrap_or(0.0)));

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

    // Optional chunk-level dedup: collapse entries pointing at the same
    // physical chunk (its id) to the best-scoring one. Entity-level alignment
    // can otherwise surface the same chunk once per contained entity.
    if config.dedup_by_chunk {
        let mut best_by_chunk: HashMap<String, SearchResult> = HashMap::new();
        for result in fused {
            best_by_chunk
                .entry(result.id.clone())
                .and_modify(|existing| {
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert(result);
        }
        fused = best_by_chunk.into_values().collect();
    }

    // Step 4: Sort by fused score descending, with deterministic tie-breaking.
    // Ties arise when a path yields few distinct raw scores (min-max then maps
    // them onto few discrete normalized values, e.g. two results -> {0, 1}) or
    // when raw scores coincide (multi-entity expansion duplicates scores);
    // without a stable secondary key the order would depend on HashMap
    // iteration order and differ across requests.
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

    (fused, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::SearchResult;
    use cce_types::EntityId;

    fn make_vector_result(id: &str, entity_id: u64, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: vec![EntityId(entity_id)],
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    fn make_bm25_result(id: &str, entity_id: u64, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: vec![EntityId(entity_id)],
            score,
            original_score: score,
            vector_score: 0.0,
            bm25_score: Some(score),
            sources: vec!["bm25".to_string()],
            ..Default::default()
        }
    }

    fn make_vector_result_with_segment(id: &str, segment_id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: Some(segment_id.to_string()),
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    fn make_bm25_result_with_segment(id: &str, segment_id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: Some(segment_id.to_string()),
            score,
            original_score: score,
            vector_score: 0.0,
            bm25_score: Some(score),
            sources: vec!["bm25".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_minmax_normalize() {
        let scores = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let normalized = minmax_normalize(&scores);
        assert!((normalized[0] - 0.0).abs() < 0.001);
        assert!((normalized[4] - 1.0).abs() < 0.001);
        assert!((normalized[2] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_normalize_all_equal() {
        let scores = vec![0.5, 0.5, 0.5];
        let normalized = minmax_normalize(&scores);
        assert!(normalized.iter().all(|&s| (s - 1.0).abs() < 0.001));
    }

    #[test]
    fn test_fuse_hybrid_results_both_paths() {
        // entity 1 in both, entity 2 in both, entity 3 only vector, entity 4 only BM25
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
            make_vector_result("emb_c", 3, 0.5),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_b", 2, 0.8),
            make_bm25_result("bm25_a", 1, 0.6),
            make_bm25_result("bm25_d", 4, 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        // All 4 unique entities should be present
        let entity_ids: Vec<i64> = fused
            .iter()
            .filter_map(|r| r.entity_ids.first())
            .map(|e| e.0 as i64)
            .collect();
        assert!(entity_ids.contains(&1));
        assert!(entity_ids.contains(&2));
        assert!(entity_ids.contains(&3));
        assert!(entity_ids.contains(&4));

        // entity 2 has high scores in both paths
        let e2_result = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(2)))
            .unwrap();
        assert!(e2_result.score > 0.5);

        // entity 3 is only in vector path with half weight
        let e3_result = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(3)))
            .unwrap();
        assert!(e3_result.score < 0.5);
    }

    #[test]
    fn test_fuse_hybrid_results_exclude_single_path() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_b", 2, 0.8),
            make_bm25_result("bm25_c", 3, 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: false,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let entity_ids: Vec<i64> = fused
            .iter()
            .filter_map(|r| r.entity_ids.first())
            .map(|e| e.0 as i64)
            .collect();
        assert!(entity_ids.contains(&2));
        // entities 1 and 3 should be excluded since they only appear in one path
        assert!(!entity_ids.contains(&1));
        assert!(!entity_ids.contains(&3));
    }

    #[test]
    fn test_fuse_empty_vectors() {
        let fused = fuse_hybrid_results(
            vec![],
            vec![make_bm25_result("a", 1, 0.9)],
            &HybridFusionConfig::default(),
        );
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(1)));
    }

    #[test]
    fn test_fuse_single_path_applies_weight_and_min_score() {
        // An empty path is not a bypass: the surviving path is normalized,
        // weighted, and filtered exactly as when both paths are present.
        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };
        let fused = fuse_hybrid_results(vec![], vec![make_bm25_result("a", 1, 0.9)], &config);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(1)));
        // Single-result path normalizes to 1.0, weighted by bm25_weight.
        assert!((fused[0].score - 0.5).abs() < 0.001);

        // min_score is now enforced on the weighted score.
        let filtered = fuse_hybrid_results(
            vec![],
            vec![make_bm25_result("a", 1, 0.9)],
            &HybridFusionConfig {
                min_score: 0.6,
                ..config
            },
        );
        assert!(filtered.is_empty());

        // Symmetric case: vector-only recall when BM25 returns nothing.
        let fused = fuse_hybrid_results(vec![make_vector_result("emb_a", 1, 0.9)], vec![], &config);
        assert_eq!(fused.len(), 1);
        assert!((fused[0].score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_fuse_empty_both() {
        let fused = fuse_hybrid_results(vec![], vec![], &HybridFusionConfig::default());
        assert!(fused.is_empty());
    }

    #[test]
    fn test_fuse_best_score_per_entity() {
        // Multiple chunks per entity: should pick the best one
        let vector = vec![
            make_vector_result("emb_a1", 1, 0.5),
            make_vector_result("emb_a2", 1, 0.9), // best
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result("bm25_b", 2, 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let e1 = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(1)))
            .unwrap();
        // vector best = 0.9, bm25 = 0.8 → norm vector ~1.0, norm bm25 ~1.0 → fused = 1.0
        assert!((e1.score - 1.0).abs() < 0.01 || e1.score > 0.8);
    }

    #[test]
    fn test_fuse_document_chunks_by_segment_id() {
        let vector = vec![
            make_vector_result_with_segment("emb_sec1", "section_1", 0.9),
            make_vector_result_with_segment("emb_sec2", "section_2", 0.7),
            make_vector_result_with_segment("emb_sec3", "section_3", 0.5),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_sec2", "section_2", 0.8),
            make_bm25_result_with_segment("bm25_sec1", "section_1", 0.6),
            make_bm25_result_with_segment("bm25_sec4", "section_4", 0.9),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let segment_ids: Vec<&str> = fused
            .iter()
            .map(|r| r.segment_id.as_deref().unwrap_or(""))
            .collect();
        assert!(segment_ids.contains(&"section_1"));
        assert!(segment_ids.contains(&"section_2"));
        assert!(segment_ids.contains(&"section_3"));
        assert!(segment_ids.contains(&"section_4"));

        let sec2 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("section_2"))
            .unwrap();
        assert!(sec2.score > 0.5);

        let sec3 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("section_3"))
            .unwrap();
        assert!(sec3.score < 0.5);
    }

    #[test]
    fn test_fuse_mixed_entity_and_segment() {
        let vector = vec![
            make_vector_result("emb_func1", 100, 0.95),
            make_vector_result_with_segment("emb_doc1", "doc_section_1", 0.8),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_func1", 100, 0.7),
            make_bm25_result_with_segment("bm25_doc1", "doc_section_1", 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.6,
            bm25_weight: 0.4,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 2);

        let func = fused
            .iter()
            .find(|r| r.entity_ids.first() == Some(&EntityId(100)))
            .unwrap();
        assert!(func.score > 0.5);

        let doc = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("doc_section_1"))
            .unwrap();
        assert!(doc.score >= 0.0);

        assert!(func.score > doc.score);
    }

    #[test]
    fn test_fuse_same_segment_different_entity_no_match() {
        let vector = vec![make_vector_result("emb_a", 100, 0.9)];
        let bm25 = vec![make_bm25_result("bm25_b", 200, 0.8)];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 2);

        for r in &fused {
            assert!(
                r.score <= 0.5,
                "single-path results should have reduced score <= 0.5"
            );
        }
    }

    #[test]
    fn test_fuse_entity_id_priority_over_segment_id() {
        let mut vector = make_vector_result("emb_a", 100, 0.9);
        vector.segment_id = Some("shared_segment".to_string());

        let mut bm25 = make_bm25_result("bm25_a", 200, 0.8);
        bm25.segment_id = Some("shared_segment".to_string());

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vec![vector], vec![bm25], &config);

        assert_eq!(fused.len(), 2);

        for r in &fused {
            assert!(
                r.score <= 0.5,
                "single-path results should have reduced score <= 0.5"
            );
        }
    }

    #[test]
    fn test_fuse_multiple_chunks_same_segment_picks_best() {
        let vector = vec![
            make_vector_result_with_segment("emb_s1_a", "sec1", 0.5),
            make_vector_result_with_segment("emb_s1_b", "sec1", 0.9),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_s1_a", "sec1", 0.4),
            make_bm25_result_with_segment("bm25_s1_b", "sec1", 0.8),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        assert_eq!(fused.len(), 1);
        let sec1 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("sec1"))
            .unwrap();
        assert!(sec1.score > 0.8);
    }

    #[test]
    fn test_fuse_exclude_single_path_segment_chunks() {
        let vector = vec![
            make_vector_result_with_segment("emb_s1", "sec1", 0.9),
            make_vector_result_with_segment("emb_s2", "sec2", 0.7),
        ];
        let bm25 = vec![make_bm25_result_with_segment("bm25_s2", "sec2", 0.8)];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: false,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        let segment_ids: Vec<&str> = fused
            .iter()
            .map(|r| r.segment_id.as_deref().unwrap_or(""))
            .collect();
        assert!(segment_ids.contains(&"sec2"));
        assert!(!segment_ids.contains(&"sec1"));
    }

    fn make_unkeyed_result(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            entity_ids: Vec::new(),
            segment_id: None,
            score,
            original_score: score,
            vector_score: score,
            bm25_score: None,
            sources: vec!["vector".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_fuse_unkeyed_results_do_not_collapse() {
        // Unkeyed results (no entity_id, no segment_id) must survive as
        // individual entries keyed by chunk id, not collapse onto one key.
        let vector = vec![
            make_unkeyed_result("emb_a", 0.9),
            make_unkeyed_result("emb_b", 0.8),
            make_unkeyed_result("emb_c", 0.7),
        ];
        let bm25 = vec![
            make_unkeyed_result("bm25_b", 0.9),
            make_unkeyed_result("bm25_c", 0.85),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        // All 5 distinct chunks survive (3 vector + 2 bm25), no collapse.
        assert_eq!(fused.len(), 5, "unkeyed chunks must not collapse");
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        for id in ["emb_a", "emb_b", "emb_c", "bm25_b", "bm25_c"] {
            assert_eq!(
                ids.iter().filter(|i| **i == id).count(),
                1,
                "chunk {id} should appear exactly once"
            );
        }
    }

    #[test]
    fn test_fuse_unkeyed_path_ids_do_not_fuse_across_paths() {
        // Chunk ids embedding the path discriminator (`{group}_{emb|bm25}_{i}`)
        // must NOT be force-aligned at read time: the embedding and BM25 paths
        // chunk independently, so `g_emb_0` and `g_bm25_0` are different
        // physical chunks even though a naive suffix strip would equate them.
        // Unkeyed results keep their raw chunk id key and survive as two
        // single-path entries (regression: previously normalized to `c:g`).
        let vector = vec![
            make_unkeyed_result("g_emb_0", 0.9),
            make_unkeyed_result("h_emb_1", 0.8),
        ];
        let bm25 = vec![
            make_unkeyed_result("g_bm25_0", 0.9),
            make_unkeyed_result("h_bm25_2", 0.85),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        // All 4 chunks survive; `g_emb_0` and `g_bm25_0` are NOT fused despite
        // sharing the stripped group prefix.
        assert_eq!(
            fused.len(),
            4,
            "unkeyed path-id chunks must not fuse across paths"
        );
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        for id in ["g_emb_0", "g_bm25_0", "h_emb_1", "h_bm25_2"] {
            assert_eq!(
                ids.iter().filter(|i| **i == id).count(),
                1,
                "chunk {id} should appear exactly once"
            );
        }
    }

    #[test]
    fn test_fuse_document_chunks_align_by_segment_id() {
        // BM25 and vector document chunks sharing a segment_id must fuse
        // (regression: BM25 document chunks previously had no segment_id and
        // collapsed to a single shared key).
        let vector = vec![
            make_vector_result_with_segment("emb_sec1", "doc_group_1", 0.9),
            make_vector_result_with_segment("emb_sec2", "doc_group_2", 0.7),
        ];
        let bm25 = vec![
            make_bm25_result_with_segment("bm25_sec1", "doc_group_1", 0.8),
            make_bm25_result_with_segment("bm25_sec2", "doc_group_2", 0.6),
        ];

        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);

        // Both segment keys are present in both paths and matched.
        assert_eq!(fused.len(), 2);
        for r in &fused {
            assert!(
                r.bm25_score.is_some(),
                "matched document chunk should carry the BM25 score"
            );
        }
        // doc_group_1 ranks first in both paths → fuses to 0.5*1.0 + 0.5*1.0.
        let sec1 = fused
            .iter()
            .find(|r| r.segment_id.as_deref() == Some("doc_group_1"))
            .unwrap();
        assert!((sec1.score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fuse_deterministic_tie_break() {
        // Equal fused scores (common with discrete min-max values) must not
        // depend on HashMap iteration order.
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result("emb_b", 2, 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result("bm25_b", 2, 0.6),
        ];
        let config = HybridFusionConfig {
            vector_weight: 0.5,
            bm25_weight: 0.5,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: false,
        };

        let fused = fuse_hybrid_results(vector, bm25, &config);
        let ids: Vec<String> = fused.iter().map(|r| r.id.clone()).collect();
        for _ in 0..20 {
            let again = fuse_hybrid_results(
                vec![
                    make_vector_result("emb_a", 1, 0.9),
                    make_vector_result("emb_b", 2, 0.7),
                ],
                vec![
                    make_bm25_result("bm25_a", 1, 0.8),
                    make_bm25_result("bm25_b", 2, 0.6),
                ],
                &config,
            );
            let ids_again: Vec<String> = again.iter().map(|r| r.id.clone()).collect();
            assert_eq!(ids, ids_again, "tie order must be deterministic");
        }
    }

    #[test]
    fn test_fuse_dedup_by_chunk_keeps_best_per_chunk() {
        // Two entities living in the same physical chunk produce two entries
        // at entity granularity; dedup_by_chunk collapses to the best one.
        let mut v1 = make_vector_result("chunk_x", 100, 0.9);
        v1.segment_id = Some("seg".to_string());
        let mut v2 = make_vector_result("chunk_x", 200, 0.95);
        v2.segment_id = Some("seg".to_string());
        let mut b1 = make_bm25_result("chunk_x", 100, 0.8);
        b1.segment_id = Some("seg".to_string());

        // 0.7/0.3 weights keep the fused scores distinct: entity 100 fuses to
        // 0.7*0.0 + 0.3*1.0 = 0.3, entity 200 (vector-only) to 0.7*1.0 = 0.7.
        let config = HybridFusionConfig {
            vector_weight: 0.7,
            bm25_weight: 0.3,
            include_single_path: true,
            min_score: 0.0,
            dedup_by_chunk: true,
        };
        let fused = fuse_hybrid_results(vec![v1, v2], vec![b1], &config);

        assert_eq!(fused.len(), 1, "dedup_by_chunk must collapse to one entry");
        assert_eq!(fused[0].entity_ids.first(), Some(&EntityId(200)));
        assert!((fused[0].score - 0.7).abs() < 0.001);

        // Same input without dedup keeps both entity-level entries.
        let config_no_dedup = HybridFusionConfig {
            dedup_by_chunk: false,
            ..config
        };
        let fused_no_dedup = fuse_hybrid_results(
            vec![
                {
                    let mut v = make_vector_result("chunk_x", 100, 0.9);
                    v.segment_id = Some("seg".to_string());
                    v
                },
                {
                    let mut v = make_vector_result("chunk_x", 200, 0.95);
                    v.segment_id = Some("seg".to_string());
                    v
                },
            ],
            vec![{
                let mut b = make_bm25_result("chunk_x", 100, 0.8);
                b.segment_id = Some("seg".to_string());
                b
            }],
            &config_no_dedup,
        );
        assert_eq!(fused_no_dedup.len(), 2);
    }

    #[test]
    fn test_compute_alignment_coverage_counts_matched_keys() {
        let vector = vec![
            make_vector_result("emb_a", 1, 0.9),
            make_vector_result_with_segment("emb_d1", "doc_1", 0.8),
            make_unkeyed_result("emb_u", 0.7),
        ];
        let bm25 = vec![
            make_bm25_result("bm25_a", 1, 0.8),
            make_bm25_result_with_segment("bm25_d1", "doc_1", 0.6),
            make_bm25_result_with_segment("bm25_d2", "doc_2", 0.7),
        ];

        let stats = compute_alignment_coverage(&vector, &bm25);
        assert_eq!(stats.vector_keys, 3);
        assert_eq!(stats.bm25_keys, 3);
        assert_eq!(stats.matched_keys, 2);
    }
}
