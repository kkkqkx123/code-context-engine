//! Multi-source score aggregation for the retrieval-method benchmark.
//!
//! The production aggregated search runs several sub-queries, merges their
//! hits onto one dedup key (`unified_dedup_key`), and combines the scores of
//! the hits that land on the same key with a configurable strategy
//! (`[search.aggregation] score_strategy`). How those repeated hits combine
//! changes the merged ranking, so it needs an offline baseline of its own.
//!
//! # What is measured
//!
//! Production sub-queries all run the same scorer configuration, so their
//! scores live on one scale. The benchmark has a single query text per query,
//! so it uses the two recall paths as two sources instead of inventing
//! sub-query texts: each path is min-max normalized per query (the same rule
//! fusion applies) and then merged exactly like sub-query results would be.
//! That is a two-source instance of the production merge, not a claim that
//! the two paths are sub-queries — the report labels it as such.
//!
//! The merge itself is not re-implemented: keys come from the production
//! `unified_dedup_key` and score combination from the production
//! `combine_aggregate_scores`, so a change in the shipped formula shows up in
//! the baseline instead of being masked by a drifting copy.
//!
//! # Representative selection
//!
//! The entry that contributed the strongest single score stays the
//! representative (its chunk carries the identity and line range), which is
//! the production rule. Unlike fusion, no line-range union is applied: a
//! sub-query hit is a whole result, not a fragment of an entity.

use std::collections::HashMap;

use crate::bench_data::ChunkData;
use cce_config::modules::search::AggregationScoreStrategy;
use cce_orchestrator::query::coordinator::combine_aggregate_scores;
use cce_orchestrator::query::retrieval::primitives::{minmax_normalize, unified_dedup_key};
use cce_orchestrator::query::types::SearchResult;
use cce_types::EntityId;

/// Items taken from each source before merging.
///
/// Stands in for the per-sub-query result limit. It is the largest evaluated
/// cutoff, so the aggregation effect is measured without also measuring a
/// truncation the benchmark itself chose.
pub const SOURCE_LIMIT: usize = 50;

/// Maximum size of the merged list, mirroring the production truncation.
pub const OUTPUT_LIMIT: usize = 50;

/// One ranked list standing in for a sub-query result list.
pub struct AggSource<'a> {
    /// Chunk metadata for the source (indices in `ranked` refer to this slice).
    pub chunks: &'a [ChunkData],
    /// `(chunk index, raw score)`, sorted by raw score descending.
    pub ranked: Vec<(usize, f64)>,
}

impl<'a> AggSource<'a> {
    pub fn new(chunks: &'a [ChunkData], ranked: Vec<(usize, f64)>) -> Self {
        Self { chunks, ranked }
    }

    /// Per-query normalized scores, in rank order.
    ///
    /// Degenerate input (a single item or no spread) maps to full credit for
    /// every item, matching the normalization the fusion path uses.
    fn normalized(&self) -> Vec<(usize, f64)> {
        let raw: Vec<f32> = self.ranked.iter().map(|&(_, s)| s as f32).collect();
        self.ranked
            .iter()
            .zip(minmax_normalize(&raw))
            .map(|(&(idx, _), normalized)| (idx, normalized as f64))
            .take(SOURCE_LIMIT)
            .collect()
    }
}

/// One merged entry: one dedup key with the contributions of every source.
#[derive(Debug, Clone)]
pub struct AggregatedEntry {
    pub key: String,
    pub chunk: ChunkData,
    /// Combined score on the `[0, 1]` scale.
    pub score: f64,
    /// Number of source hits that landed on this key.
    pub contributions: usize,
}

/// Dedup key of a chunk, through the production key derivation.
///
/// `content` is intentionally empty: the benchmark carries no body, so the
/// weak content key can never fire and unkeyed chunks stay unkeyed, exactly as
/// they would in production when no body is materialized.
fn dedup_key(chunk: &ChunkData) -> Option<String> {
    let entity_ids: Vec<EntityId> = chunk
        .entity_ids
        .iter()
        .map(|id| EntityId(*id as u64))
        .collect();
    let result = SearchResult {
        id: chunk.chunk_id.clone(),
        entity_ids,
        segment_id: Some(chunk.segment_id.clone()),
        file_path: chunk.file_path.clone(),
        ..Default::default()
    };
    unified_dedup_key(&result)
}

/// Merge source rankings onto their dedup keys and combine the scores.
///
/// Mirrors the production merge loop: the first hit establishes the entry,
/// every later hit on the same key raises the score through `strategy` and
/// takes over as the representative only when it contributed the stronger
/// single score. Entries without a dedup key are appended untouched, as
/// production does. The result is sorted by combined score descending with a
/// deterministic key tie-break, then truncated to [`OUTPUT_LIMIT`].
pub fn merge_sources(
    sources: &[AggSource<'_>],
    strategy: AggregationScoreStrategy,
) -> Vec<AggregatedEntry> {
    let mut merged: HashMap<String, AggregatedEntry> = HashMap::new();
    let mut strongest: HashMap<String, f64> = HashMap::new();
    let mut unkeyed: Vec<AggregatedEntry> = Vec::new();

    for source in sources {
        for (idx, score) in source.normalized() {
            let Some(chunk) = source.chunks.get(idx) else {
                continue;
            };
            let Some(key) = dedup_key(chunk) else {
                unkeyed.push(AggregatedEntry {
                    key: format!("c:{}", chunk.chunk_id),
                    chunk: chunk.clone(),
                    score,
                    contributions: 1,
                });
                continue;
            };
            match merged.get_mut(&key) {
                Some(existing) => {
                    let combined =
                        combine_aggregate_scores(existing.score as f32, score as f32, strategy)
                            as f64;
                    existing.contributions += 1;
                    if score > strongest.get(&key).copied().unwrap_or(f64::MIN) {
                        existing.chunk = chunk.clone();
                        strongest.insert(key, score);
                    }
                    existing.score = combined;
                }
                None => {
                    strongest.insert(key.clone(), score);
                    merged.insert(
                        key.clone(),
                        AggregatedEntry {
                            key,
                            chunk: chunk.clone(),
                            score,
                            contributions: 1,
                        },
                    );
                }
            }
        }
    }

    let mut out: Vec<AggregatedEntry> = merged.into_values().collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    out.extend(unkeyed);
    out.truncate(OUTPUT_LIMIT);
    out
}

/// How much multi-source overlap one baseline actually has.
///
/// Without overlap the aggregation strategy cannot change anything; this stat
/// makes a "no difference" result explainable instead of looking like a bug in
/// the benchmark.
#[derive(Debug, Clone, Copy, Default)]
pub struct AggregationStat {
    /// Keys contributed by more than one source, summed over queries.
    pub multi_source_keys: usize,
    /// Keys contributed by exactly one source, summed over queries.
    pub single_source_keys: usize,
    /// Summed contributions across all queries.
    pub contributions: usize,
}

impl AggregationStat {
    /// Record the merge outcome of one query.
    pub fn observe(&mut self, entries: &[AggregatedEntry]) {
        for entry in entries {
            self.contributions += entry.contributions;
            if entry.contributions > 1 {
                self.multi_source_keys += 1;
            } else {
                self.single_source_keys += 1;
            }
        }
    }

    /// Share of keys that received more than one contribution, in `[0, 1]`.
    pub fn multi_source_ratio(&self) -> f64 {
        let total = self.multi_source_keys + self.single_source_keys;
        if total == 0 {
            0.0
        } else {
            self.multi_source_keys as f64 / total as f64
        }
    }

    /// Mean contributions per key.
    pub fn contributions_per_key(&self) -> f64 {
        let total = self.multi_source_keys + self.single_source_keys;
        if total == 0 {
            0.0
        } else {
            self.contributions as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, entity_id: i64) -> ChunkData {
        ChunkData {
            chunk_id: id.to_string(),
            entity_name: String::new(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            end_line: 5,
            source_ranges: Vec::new(),
            source_span_kind: String::new(),
            test_info: cce_types::TestInfo::unknown(),
            language: None,
            entity_ids: vec![entity_id],
            segment_id: "group_1".to_string(),
        }
    }

    #[test]
    fn repeated_key_is_merged_once_with_combined_score() {
        // Entity 7 is hit by both sources; entity 8 only by the second.
        let a_chunks = [chunk("a_emb_0", 7)];
        let b_chunks = [chunk("b_bm25_0", 7), chunk("b_bm25_1", 8)];
        let sources = vec![
            AggSource::new(&a_chunks, vec![(0, 10.0)]),
            AggSource::new(&b_chunks, vec![(0, 8.0), (1, 2.0)]),
        ];
        let merged = merge_sources(&sources, AggregationScoreStrategy::NoisyOr);
        assert_eq!(merged.len(), 2, "one entry per dedup key");
        let seven = merged
            .iter()
            .find(|e| e.key == "e:7")
            .expect("entity 7 merged");
        assert_eq!(seven.contributions, 2);
        // Source A is degenerate (single item) so it normalizes to 1.0; source
        // B maps 8.0/2.0 to 1.0/0.0. Two saturated contributions leave 0.999999
        // because contributions are clamped just below 1.
        assert!(
            seven.score > 0.999,
            "noisy-OR saturates near 1: {}",
            seven.score
        );
    }

    #[test]
    fn noisy_or_rewards_agreement_but_max_does_not() {
        // Entity 7 is the middle hit on both sources, so both normalize it to
        // 0.5. `max` keeps 0.5 (agreement carries no information); noisy-OR
        // accumulates the two independent hits into 1 - 0.5 * 0.5 = 0.75.
        let a_chunks = [chunk("a_0", 8), chunk("a_1", 7), chunk("a_2", 9)];
        let b_chunks = [chunk("b_0", 10), chunk("b_1", 7), chunk("b_2", 9)];
        let sources = vec![
            AggSource::new(&a_chunks, vec![(0, 3.0), (1, 2.0), (2, 1.0)]),
            AggSource::new(&b_chunks, vec![(0, 3.0), (1, 2.0), (2, 1.0)]),
        ];
        let score_of = |strategy, key: &str| {
            merge_sources(&sources, strategy)
                .into_iter()
                .find(|e| e.key == key)
                .expect("key merged")
                .score
        };
        let noisy = score_of(AggregationScoreStrategy::NoisyOr, "e:7");
        let max = score_of(AggregationScoreStrategy::Max, "e:7");
        assert!(
            (max - 0.5).abs() < 1e-6,
            "max keeps the strongest single hit"
        );
        assert!(
            (noisy - 0.75).abs() < 1e-6,
            "noisy-OR accumulates two half-strength hits"
        );
        // A single-source key is unaffected by the strategy.
        assert!((score_of(AggregationScoreStrategy::Max, "e:8") - 1.0).abs() < 1e-6);
        assert!((score_of(AggregationScoreStrategy::NoisyOr, "e:8") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn representative_is_the_strongest_contribution() {
        let a_chunks = [chunk("a_0", 7), chunk("a_1", 8)];
        let b_chunks = [chunk("b_0", 7), chunk("b_1", 8)];
        let sources = vec![
            AggSource::new(&a_chunks, vec![(0, 1.0), (1, 9.0)]),
            AggSource::new(&b_chunks, vec![(0, 9.0), (1, 1.0)]),
        ];
        let merged = merge_sources(&sources, AggregationScoreStrategy::Max);
        let seven = merged
            .iter()
            .find(|e| e.key == "e:7")
            .expect("entity 7 merged");
        // Source A normalizes 1.0 -> 0.0 and source B 9.0 -> 1.0, so the B hit
        // is the strongest contribution and owns the representative chunk.
        assert_eq!(seven.chunk.chunk_id, "b_0");
    }

    #[test]
    fn unkeyed_chunks_are_appended_without_merging() {
        let mut bare = chunk("bare_0", 7);
        bare.entity_ids.clear();
        bare.segment_id = String::new();
        bare.language = Some(cce_types::Language::Rust);
        let chunks = [bare];
        let sources = vec![AggSource::new(&chunks, vec![(0, 5.0)])];
        let merged = merge_sources(&sources, AggregationScoreStrategy::NoisyOr);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "c:bare_0");
        assert_eq!(merged[0].contributions, 1);
    }

    #[test]
    fn stat_reports_overlap_ratio() {
        let a_chunks = [chunk("a_0", 7), chunk("a_1", 8)];
        let b_chunks = [chunk("b_0", 7), chunk("b_1", 9)];
        let sources = vec![
            AggSource::new(&a_chunks, vec![(0, 9.0), (1, 8.0)]),
            AggSource::new(&b_chunks, vec![(0, 9.0), (1, 8.0)]),
        ];
        let merged = merge_sources(&sources, AggregationScoreStrategy::NoisyOr);
        let mut stat = AggregationStat::default();
        stat.observe(&merged);
        assert_eq!(stat.multi_source_keys, 1, "entity 7 hit by both sources");
        assert_eq!(stat.single_source_keys, 2);
        assert!((stat.multi_source_ratio() - 1.0 / 3.0).abs() < 1e-9);
        assert!((stat.contributions_per_key() - 4.0 / 3.0).abs() < 1e-9);
    }
}
