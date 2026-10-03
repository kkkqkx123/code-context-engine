//! Cross-path alignment key derivation and coverage computation.

use std::collections::HashMap;

use crate::query::types::SearchResult;

/// Derive the cross-path alignment key for a search result.
///
/// Delegates to the shared derivation in `cce_types`, which is the single
/// source of truth also consumed by the offline benchmark/e2e mirror.
///
/// Exposed so the aggregation dedup in `query/coordinator.rs` derives the same
/// key format as hybrid fusion instead of duplicating it with a bare `{id}`.
pub fn alignment_key(
    entity_ids: &[cce_types::EntityId],
    segment_id: Option<&str>,
    chunk_id: &str,
) -> Option<String> {
    let key = cce_types::alignment_key(entity_ids, segment_id, chunk_id);
    if key.is_none() {
        tracing::trace!(
            chunk_id,
            "Hybrid alignment key could not be derived (no entity_id/segment_id/chunk_id)"
        );
    }
    key
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

/// Aggregate a result list into one best-raw-score entry per alignment key.
///
/// Keys derive from [`alignment_key`]; results without a key are skipped.
/// Returns `key -> (result index, raw score)`.
pub fn best_per_key(
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
