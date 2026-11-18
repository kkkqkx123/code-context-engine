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

/// Whether a result's derived key can ever match the other recall path.
///
/// The key derives from an entity, a logical segment, or the chunk id. Only
/// the first two can be shared across paths: the chunk id carries a per-path
/// suffix, and the index substitutes the chunk id for a missing `segment_id`
/// (`storage_coordinator::mapping::chunk_segment_id`), which makes `s:` keys
/// path-local too. Both shapes are reported as unalignable so callers can
/// distinguish "intentional fallback" from "index lost the fields".
pub fn is_alignable(
    entity_ids: &[cce_types::EntityId],
    segment_id: Option<&str>,
    chunk_id: &str,
) -> bool {
    if !entity_ids.is_empty() {
        return true;
    }
    match segment_id {
        Some(segment) => !segment.is_empty() && segment != chunk_id,
        None => false,
    }
}

/// What a key-completion pass achieved, for logging and metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyCompletionOutcome {
    /// Candidates whose key was path-local before the pass.
    pub candidates: usize,
    /// Candidates that became alignable thanks to a chunk record.
    pub completed: usize,
    /// Candidates left path-local (no record, or the record carries nothing).
    pub still_unalignable: usize,
}

/// Fill path-local alignment keys from chunk records.
///
/// Only identifiers are touched — `entity_ids` and a meaningful `segment_id`.
/// A candidate whose key already aligns is never modified, so the pass can
/// only turn "never matches" into "may match", never the other way round.
/// Content, scores and line ranges stay untouched: the full enrichment still
/// runs once after fusion.
pub fn complete_keys_from_records(
    results: &mut [SearchResult],
    records: &HashMap<String, cce_storage_common::metadb::ChunkRecord>,
) -> KeyCompletionOutcome {
    let mut outcome = KeyCompletionOutcome::default();
    for result in results.iter_mut() {
        if is_alignable(&result.entity_ids, result.segment_id.as_deref(), &result.id) {
            continue;
        }
        outcome.candidates += 1;
        let Some(record) = records.get(&result.id) else {
            outcome.still_unalignable += 1;
            continue;
        };
        if result.entity_ids.is_empty() {
            let stored: Vec<i64> = serde_json::from_str(&record.entity_ids).unwrap_or_default();
            if !stored.is_empty() {
                result.entity_ids = stored
                    .into_iter()
                    .map(|id| cce_types::EntityId(id as u64))
                    .collect();
            }
        }
        let chunk_id = result.id.clone();
        let segment_is_fallback = result
            .segment_id
            .as_deref()
            .is_none_or(|segment| segment.is_empty() || segment == chunk_id);
        if segment_is_fallback && !record.segment_id.is_empty() && record.segment_id != chunk_id {
            result.segment_id = Some(record.segment_id.clone());
        }
        if is_alignable(&result.entity_ids, result.segment_id.as_deref(), &result.id) {
            outcome.completed += 1;
        } else {
            outcome.still_unalignable += 1;
        }
    }
    outcome
}

/// Cross-path alignment coverage between the vector and BM25 result sets.
#[derive(Debug, Clone, Copy, Default)]
pub struct FusionAlignmentStats {
    /// Number of distinct alignment keys on the vector path.
    pub vector_keys: usize,
    /// Number of distinct alignment keys on the BM25 path.
    pub bm25_keys: usize,
    /// Number of keys present in both paths.
    pub matched_keys: usize,
    /// Vector entries that can never match the other path: either no key at
    /// all, or the chunk-id fallback (whose id carries a path suffix, so the
    /// key is unique to this path by construction).
    pub vector_unalignable: usize,
    /// BM25 entries that can never match the other path (same rule).
    pub bm25_unalignable: usize,
}

impl FusionAlignmentStats {
    /// Total entries that can never align, both paths.
    pub fn unalignable(&self) -> usize {
        self.vector_unalignable + self.bm25_unalignable
    }

    /// Total entries observed, keyed plus unalignable, both paths.
    pub fn total_entries(&self) -> usize {
        self.vector_keys + self.bm25_keys + self.unalignable()
    }

    /// Share of entries that can never align, in `[0, 1]`; zero when empty.
    ///
    /// A non-trivial share is expected (chunk-id fallback is intentional and
    /// applies to plugin or abnormal chunks); a share that keeps growing
    /// points at missing index fields instead. Distinguishing the two is what
    /// this ratio exists for.
    pub fn unalignable_ratio(&self) -> f64 {
        let total = self.total_entries();
        if total == 0 {
            0.0
        } else {
            self.unalignable() as f64 / total as f64
        }
    }

    /// Share of vector keys that also exist on the BM25 path, in `[0, 1]`.
    ///
    /// The denominator is the vector key count because the vector path is the
    /// primary semantic recall; a zero-key vector path reports zero coverage.
    pub fn match_ratio(&self) -> f64 {
        if self.vector_keys == 0 {
            0.0
        } else {
            self.matched_keys as f64 / self.vector_keys as f64
        }
    }

    /// Number of distinct keys returned by exactly one path.
    pub fn single_path_keys(&self) -> usize {
        self.vector_keys + self.bm25_keys - 2 * self.matched_keys
    }

    /// Share of distinct keys returned by exactly one path, in `[0, 1]`.
    ///
    /// Path-local keys are excluded from both the numerator and the
    /// denominator: they are counted by `unalignable_ratio` instead, so the two
    /// ratios remain additive views of the same candidate set.
    pub fn single_path_ratio(&self) -> f64 {
        let distinct = self.vector_keys + self.bm25_keys - self.matched_keys;
        if distinct == 0 {
            0.0
        } else {
            self.single_path_keys() as f64 / distinct as f64
        }
    }
}

/// Compute cross-path alignment coverage for a pair of result sets.
///
/// Mirrors the key derivation used inside `fuse_hybrid_results`. Exposed so
/// callers (and the searcher's metrics) can observe silent degradation such as
/// zero matched keys, where hybrid fusion degenerates to a union of two
/// single-path rankings, plus the share of entries that can never align.
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
    fn unalignable(results: &[SearchResult]) -> usize {
        results
            .iter()
            .filter(|r| !is_alignable(&r.entity_ids, r.segment_id.as_deref(), &r.id))
            .count()
    }
    let vector_keys = keys(vector_results);
    let bm25_keys = keys(bm25_results);
    let matched_keys = vector_keys.intersection(&bm25_keys).count();
    FusionAlignmentStats {
        vector_keys: vector_keys.len(),
        bm25_keys: bm25_keys.len(),
        matched_keys,
        vector_unalignable: unalignable(vector_results),
        bm25_unalignable: unalignable(bm25_results),
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

/// Weak post-enrichment key for results without an alignment key.
///
/// Uses the normalized file path plus a stable hash of the materialized body.
/// Empty bodies yield no weak key and are kept as-is. The single source of
/// truth for unkeyed collapsing across post-processing and aggregation.
pub fn weak_post_key(result: &crate::query::types::SearchResult) -> Option<String> {
    if alignment_key(&result.entity_ids, result.segment_id.as_deref(), &result.id).is_some() {
        return None;
    }
    if result.content.trim().is_empty() || result.file_path.is_empty() {
        return None;
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    result.file_path.hash(&mut hasher);
    result.content.hash(&mut hasher);
    Some(format!("w:{}:{:x}", result.file_path, hasher.finish()))
}

/// Unified dedup key: alignment key first, weak post key second.
///
/// Shared by single-search post-processing and aggregated merging so both
/// collapse onto one key space.
pub fn unified_dedup_key(result: &crate::query::types::SearchResult) -> Option<String> {
    if let Some(key) = alignment_key(&result.entity_ids, result.segment_id.as_deref(), &result.id) {
        return Some(key);
    }
    weak_post_key(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cce_storage_common::metadb::ChunkRecord;

    fn record(chunk_id: &str, entity_ids: &str, segment_id: &str) -> ChunkRecord {
        let mut record = ChunkRecord::new(
            chunk_id.to_string(),
            "src/lib.rs".to_string(),
            String::new(),
            0,
            4,
        );
        record.entity_ids = entity_ids.to_string();
        record.segment_id = segment_id.to_string();
        record
    }

    fn hit(chunk_id: &str, segment_id: Option<&str>) -> SearchResult {
        SearchResult {
            id: chunk_id.to_string(),
            segment_id: segment_id.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn fallback_chunk_key_is_not_alignable() {
        // The index substitutes the chunk id for a missing segment, so an
        // `s:` key pointing at the chunk's own id is just as path-local as the
        // `c:` fallback.
        assert!(!is_alignable(&[], Some("group_1_emb_0"), "group_1_emb_0"));
        assert!(!is_alignable(&[], None, "group_1_emb_0"));
        assert!(!is_alignable(&[], Some(""), "group_1_emb_0"));
        assert!(is_alignable(&[], Some("group_1"), "group_1_emb_0"));
        assert!(is_alignable(
            &[cce_types::EntityId(7)],
            None,
            "group_1_emb_0"
        ));
    }

    #[test]
    fn completion_restores_entity_key_from_record() {
        let mut results = vec![hit("group_1_emb_0", Some("group_1_emb_0"))];
        let records = std::collections::HashMap::from([(
            "group_1_emb_0".to_string(),
            record("group_1_emb_0", "[7,8]", ""),
        )]);
        let outcome = complete_keys_from_records(&mut results, &records);
        assert_eq!(outcome.candidates, 1);
        assert_eq!(outcome.completed, 1);
        assert_eq!(outcome.still_unalignable, 0);
        assert_eq!(
            results[0].entity_ids,
            vec![cce_types::EntityId(7), cce_types::EntityId(8)]
        );
        assert!(is_alignable(
            &results[0].entity_ids,
            results[0].segment_id.as_deref(),
            &results[0].id
        ));
    }

    #[test]
    fn completion_restores_logical_segment_key() {
        let mut results = vec![hit("doc_emb_3", None)];
        let records = std::collections::HashMap::from([(
            "doc_emb_3".to_string(),
            record("doc_emb_3", "[]", "section_2"),
        )]);
        let outcome = complete_keys_from_records(&mut results, &records);
        assert_eq!(outcome.completed, 1);
        assert_eq!(results[0].segment_id.as_deref(), Some("section_2"));
    }

    #[test]
    fn completion_degrades_when_record_is_missing_or_empty() {
        let mut results = vec![hit("group_9_emb_0", None), hit("group_8_emb_0", None)];
        let records = std::collections::HashMap::from([(
            "group_9_emb_0".to_string(),
            record("group_9_emb_0", "[]", ""),
        )]);
        let outcome = complete_keys_from_records(&mut results, &records);
        assert_eq!(outcome.candidates, 2);
        assert_eq!(outcome.completed, 0);
        assert_eq!(outcome.still_unalignable, 2);
    }

    #[test]
    fn single_path_ratio_counts_keys_present_on_one_path() {
        let keyed = |chunk_id: &str, entity: u64| SearchResult {
            id: chunk_id.to_string(),
            entity_ids: vec![cce_types::EntityId(entity)],
            ..Default::default()
        };
        let vector = vec![keyed("emb_a", 1), keyed("emb_b", 2), keyed("emb_c", 3)];
        let bm25 = vec![keyed("bm25_a", 1), keyed("bm25_d", 4)];
        let stats = compute_alignment_coverage(&vector, &bm25);
        assert_eq!(stats.single_path_keys(), 3);
        assert!((stats.single_path_ratio() - 0.75).abs() < 1e-9);
        assert!(compute_alignment_coverage(&[], &[]).single_path_ratio() == 0.0);
    }

    #[test]
    fn completion_leaves_alignable_keys_alone() {
        let mut results = vec![SearchResult {
            id: "group_1_emb_0".to_string(),
            entity_ids: vec![cce_types::EntityId(5)],
            segment_id: Some("group_1".to_string()),
            ..Default::default()
        }];
        let records = std::collections::HashMap::from([(
            "group_1_emb_0".to_string(),
            record("group_1_emb_0", "[99]", "other"),
        )]);
        let outcome = complete_keys_from_records(&mut results, &records);
        assert_eq!(outcome.candidates, 0);
        assert_eq!(results[0].entity_ids, vec![cce_types::EntityId(5)]);
        assert_eq!(results[0].segment_id.as_deref(), Some("group_1"));
    }
}
