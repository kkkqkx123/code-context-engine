//! Source-range retrieval evaluation.
//!
//! A labeled target is *recovered* when the retrieved items cover its whole
//! range between them: the retrieval may split a target across several
//! chunks, and taken together those chunks still carry the target's code.
//! The coverage of a target is therefore the union of the retrieved items'
//! source ranges, not the range of any single item.
//!
//! An item *contributes* to a target when it covers a part of the target
//! that no other retrieved item covers. An item that merely repeats code
//! another item already carries adds nothing to the recovery, so it does
//! not count towards precision.
//!
//! Overlap remains available as a per-item relation and is used for the
//! diagnostics (first-hit rank, redundant coverage).

use std::collections::HashMap;

use crate::bench_data::{
    ChunkData, ChunkSourceRange, RelevanceJudgment, RelevanceLevel, SourceRange,
};

/// All relevance targets a retrieved chunk overlaps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkRelevance {
    pub matched_targets: Vec<(usize, RelevanceLevel)>,
}

impl ChunkRelevance {
    pub fn highest_level(&self) -> Option<RelevanceLevel> {
        self.matched_targets.iter().map(|(_, level)| *level).max()
    }
}

/// A retrieved item's claimed source coverage.
#[derive(Debug, Clone)]
pub struct RetrievedCoverage {
    /// Normalized file path of the item.
    pub file: String,
    /// The source ranges the item claims to cover.
    pub ranges: Vec<ChunkSourceRange>,
}

/// How a set of retrieved items covers the judgment's labeled targets.
#[derive(Debug, Clone, Default)]
pub struct TargetCoverage {
    /// Per labeled target: whether the retrieved items recovered it.
    pub recovered: Vec<bool>,
    /// Per retrieved item: the targets the item contributes to.
    pub contributions: Vec<Vec<(usize, RelevanceLevel)>>,
}

impl TargetCoverage {
    /// Highest level a retrieved item contributes to.
    pub fn level_for(&self, item: usize) -> Option<RelevanceLevel> {
        self.contributions
            .get(item)
            .and_then(|targets| targets.iter().map(|(_, level)| *level).max())
    }
}

/// An inclusive line interval inside a labeled range.
type Interval = (usize, usize);

/// Merge overlapping or touching intervals into a sorted, disjoint list.
///
/// Touching means abutting (`[25,34]` + `[35,44]` merge to `[25,44]`); a
/// blank-line gap stays disjoint so judgments spanning unrelated methods do
/// not falsely recover. Keep single-entity judgment ranges to avoid
/// boundary gaps.
fn merge_intervals(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.sort_unstable();
    let mut merged: Vec<Interval> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        match merged.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Whether the merged intervals cover every line of `[start, end]`.
fn intervals_cover(intervals: &[(usize, usize)], start: usize, end: usize) -> bool {
    let mut cursor = start;
    for (from, to) in intervals {
        if *from > cursor {
            return false;
        }
        cursor = cursor.max(*to + 1);
        if cursor > end {
            return true;
        }
    }
    false
}

/// The parts of `own` that the merged `others` do not cover.
fn uncovered(own: &[(usize, usize)], others: &[(usize, usize)]) -> Vec<Interval> {
    let mut remaining = Vec::new();
    for (start, end) in own {
        let mut cursor = *start;
        for (from, to) in others {
            if *to < cursor {
                continue;
            }
            if *from > *end {
                break;
            }
            if *from > cursor {
                remaining.push((cursor, (*from - 1).min(*end)));
            }
            cursor = cursor.max(*to + 1);
            if cursor > *end {
                break;
            }
        }
        if cursor <= *end {
            remaining.push((cursor, *end));
        }
    }
    remaining
}

/// Compute how the retrieved items cover the judgment's labeled targets.
///
/// A target is recovered when the items' source ranges together span the
/// whole labeled range, and an item contributes to a target when it covers a
/// part of the target no other item covers.
pub fn coverage_over_retrieved(
    judgment: &RelevanceJudgment,
    retrieved: &[RetrievedCoverage],
) -> TargetCoverage {
    let range_files: Vec<String> = judgment
        .relevant_ranges
        .iter()
        .map(|(range, _)| normalize_path(&range.file))
        .collect();

    // Per item, the lines it covers inside each labeled target.
    let mut covered: Vec<Vec<Vec<Interval>>> = Vec::with_capacity(retrieved.len());
    for item in retrieved {
        let mut per_target = Vec::with_capacity(judgment.relevant_ranges.len());
        for ((range, _), range_file) in judgment.relevant_ranges.iter().zip(&range_files) {
            if item.file != *range_file {
                per_target.push(Vec::new());
                continue;
            }
            let mut intervals = Vec::new();
            for source_range in &item.ranges {
                if source_range.start_line == 0 || source_range.end_line < source_range.start_line {
                    continue;
                }
                let start = source_range.start_line.max(range.start_line);
                let end = source_range.end_line.min(range.end_line);
                if start <= end {
                    intervals.push((start, end));
                }
            }
            per_target.push(intervals);
        }
        covered.push(per_target);
    }

    // Per target, whether the items together cover the whole range.
    let mut recovered = vec![false; judgment.relevant_ranges.len()];
    for (index, (range, _)) in judgment.relevant_ranges.iter().enumerate() {
        let mut intervals = Vec::new();
        for per_target in &covered {
            intervals.extend_from_slice(&per_target[index]);
        }
        recovered[index] = intervals_cover(
            &merge_intervals(intervals),
            range.start_line,
            range.end_line,
        );
    }

    // Per item, the targets it contributes to.
    let mut contributions = Vec::with_capacity(retrieved.len());
    for (item_index, per_target) in covered.iter().enumerate() {
        let mut hits = Vec::new();
        for (target_index, intervals) in per_target.iter().enumerate() {
            if !recovered[target_index] || intervals.is_empty() {
                continue;
            }
            let mut others = Vec::new();
            for (other_index, other_target) in covered.iter().enumerate() {
                if other_index == item_index {
                    continue;
                }
                others.extend_from_slice(&other_target[target_index]);
            }
            if !uncovered(intervals, &merge_intervals(others)).is_empty() {
                hits.push((target_index, judgment.relevant_ranges[target_index].1));
            }
        }
        contributions.push(hits);
    }

    TargetCoverage {
        recovered,
        contributions,
    }
}

/// Compute how the retrieved chunks cover the judgment's labeled targets.
pub fn coverage_over_chunks(judgment: &RelevanceJudgment, chunks: &[ChunkData]) -> TargetCoverage {
    let retrieved: Vec<RetrievedCoverage> = chunks
        .iter()
        .map(|chunk| RetrievedCoverage {
            file: normalize_path(&chunk.file_path),
            ranges: chunk.source_ranges.clone(),
        })
        .collect();
    coverage_over_retrieved(judgment, &retrieved)
}

/// Return every judgment target whose range the chunk's source ranges overlap.
///
/// Overlap is a per-item relation and says nothing about whether the target
/// was recovered; the coverage of a whole result set does.
pub fn relevance_for_chunk(chunk: &ChunkData, judgment: &RelevanceJudgment) -> ChunkRelevance {
    relevance_for_ranges(
        &normalize_path(&chunk.file_path),
        &chunk.source_ranges,
        judgment,
    )
}

/// Return every judgment target whose range the given source ranges overlap.
///
/// The chunk file path must already be normalized; the judgment's own paths
/// are normalized here so that every caller shares one comparison basis.
pub fn relevance_for_ranges(
    chunk_file: &str,
    chunk_ranges: &[ChunkSourceRange],
    judgment: &RelevanceJudgment,
) -> ChunkRelevance {
    let normalized_files: Vec<String> = judgment
        .relevant_ranges
        .iter()
        .map(|(range, _)| normalize_path(&range.file))
        .collect();
    relevance_matches(
        chunk_file,
        chunk_ranges,
        &judgment.relevant_ranges,
        &normalized_files,
    )
}

/// Match a chunk (with pre-normalized file path) against judgment ranges
/// whose file paths are also pre-normalized.
///
/// Evaluators that scan many chunks against one judgment normalize each side
/// once and reuse the result here.
fn relevance_matches(
    chunk_file: &str,
    chunk_ranges: &[ChunkSourceRange],
    ranges: &[(SourceRange, RelevanceLevel)],
    normalized_files: &[String],
) -> ChunkRelevance {
    let mut matched_targets = Vec::new();
    for (index, ((range, level), range_file)) in ranges.iter().zip(normalized_files).enumerate() {
        if chunk_file != range_file {
            continue;
        }
        let overlaps = chunk_ranges.iter().any(|chunk_range| {
            chunk_range.start_line > 0
                && chunk_range.end_line >= chunk_range.start_line
                && chunk_range.start_line <= range.end_line
                && chunk_range.end_line >= range.start_line
        });
        if overlaps {
            matched_targets.push((index, *level));
        }
    }
    ChunkRelevance { matched_targets }
}

/// Compatibility helper for reports that only need a chunk's highest level.
pub fn is_relevant_to_query(
    chunk: &ChunkData,
    judgment: &RelevanceJudgment,
) -> Option<RelevanceLevel> {
    relevance_for_chunk(chunk, judgment).highest_level()
}

fn normalize_path(path: &str) -> String {
    crate::bench_data::normalize_path(path)
}

fn harmonic_mean(precision: f64, recall: f64) -> f64 {
    if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

#[derive(Debug, Clone)]
pub struct RangeBasedPerQueryScore {
    /// Retrieved chunks whose highest hit is a Strong target.
    pub strong_matches: usize,
    /// Retrieved chunks whose highest hit is a Related (non-Strong) target.
    pub related_matches: usize,
    /// Retrieved chunks containing at least one labeled target.
    pub any_matches: usize,
    /// Unique labeled targets recovered by the top-k chunks.
    pub total_relevant: usize,

    pub precision_strong: f64,
    pub precision_related: f64,
    pub precision_any: f64,
    pub recall_strong: f64,
    pub recall_related: f64,
    pub recall_any: f64,
    pub f1_strong: f64,
    pub f1_related: f64,
    pub f1_any: f64,
    pub f1_score: f64,
}

/// Evaluate source-fragment recovery at a fixed cutoff.
///
/// Precision is unweighted per-chunk. Recall is unweighted per-target and
/// reported separately for the Strong and Related layers plus the combined
/// `any` layer. Chunk counters are disjoint (`strong + related == any`);
/// Related precision is measured over non-Strong retrieval slots so the
/// Related layer excludes Strong hits instead of duplicating `any`.
pub fn evaluate_query_range_based(
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    scores: &[f64],
    top_k: usize,
) -> RangeBasedPerQueryScore {
    evaluate_query_range_based_cutoffs(judgment, chunks, scores, &[top_k])
        .into_iter()
        .next()
        .expect("a non-empty cutoff list always yields a score")
}

/// Evaluate source-fragment recovery at several top-k cutoffs with a single
/// sort of the dense score vector.
pub fn evaluate_query_range_based_cutoffs(
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    scores: &[f64],
    cutoffs: &[usize],
) -> Vec<RangeBasedPerQueryScore> {
    let mut indices: Vec<usize> = (0..chunks.len()).collect();
    indices.sort_by(|left, right| scores[*right].total_cmp(&scores[*left]));

    evaluate_ranked_chunks_range_based_cutoffs(judgment, chunks, &indices, cutoffs)
}

/// Evaluate source-fragment recovery from chunks that are already ranked.
///
/// This avoids materializing and sorting a dense score vector when a retrieval
/// backend has already returned its ranked top results.
pub fn evaluate_ranked_chunks_range_based(
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    ranked_indices: &[usize],
    top_k: usize,
) -> RangeBasedPerQueryScore {
    evaluate_ranked_chunks_range_based_cutoffs(judgment, chunks, ranked_indices, &[top_k])
        .into_iter()
        .next()
        .expect("a non-empty cutoff list always yields a score")
}

/// Evaluate source-fragment recovery at several top-k cutoffs with a single
/// pass over an already-ranked chunk list.
pub fn evaluate_ranked_chunks_range_based_cutoffs(
    judgment: &RelevanceJudgment,
    chunks: &[ChunkData],
    ranked_indices: &[usize],
    cutoffs: &[usize],
) -> Vec<RangeBasedPerQueryScore> {
    let mut scan = RankedScan::new(judgment, cutoffs, chunks.len());
    let max_cutoff = cutoffs.iter().copied().max().unwrap_or(0);
    for &index in ranked_indices.iter().take(max_cutoff) {
        if let Some(chunk) = chunks.get(index) {
            scan.advance(chunk);
        }
    }
    scan.finish().into_iter().map(|(range, _)| range).collect()
}

/// First-hit and redundant-coverage diagnostics captured at a cutoff during a
/// single-pass ranked scan.
#[derive(Debug, Clone, Copy)]
pub struct CutoffDiagnostics {
    /// Average first-hit rank across the judgment's relevant ranges.
    pub avg_first_hit_rank: Option<f64>,
    /// Number of (chunk, relevant range) overlaps inside the top-k.
    pub redundant_coverage: usize,
}

fn finalize_range_score(
    target_levels: &[RelevanceLevel],
    coverage: &TargetCoverage,
    top_k: usize,
    total_chunks: usize,
) -> RangeBasedPerQueryScore {
    // Precision counters: an item counts once, classified by the highest
    // level among the targets it contributes to.
    let mut strong_matches = 0;
    let mut related_matches = 0;
    let mut any_matches = 0;
    for hits in &coverage.contributions {
        let Some(highest) = hits.iter().map(|(_, level)| *level).max() else {
            continue;
        };
        any_matches += 1;
        // The three layer counters are disjoint: a chunk whose highest hit is
        // Strong only counts as strong, one whose highest hit is Related
        // counts as related, so `strong + related == any` always holds.
        match highest {
            RelevanceLevel::Strong => strong_matches += 1,
            RelevanceLevel::Related => related_matches += 1,
            RelevanceLevel::Irrelevant => {}
        }
    }
    let covered_targets = coverage
        .recovered
        .iter()
        .filter(|recovered| **recovered)
        .count();

    // Unweighted recall: each relevant target counts once toward total and
    // toward coverage, and the two relevance layers are measured separately.
    let exact_recall = |wanted: RelevanceLevel| -> f64 {
        let total = target_levels
            .iter()
            .filter(|level| **level == wanted)
            .count();
        if total == 0 {
            return 0.0;
        }
        let covered = coverage
            .recovered
            .iter()
            .enumerate()
            .filter(|(index, recovered)| {
                **recovered
                    && target_levels
                        .get(*index)
                        .map(|level| *level == wanted)
                        .unwrap_or(false)
            })
            .count();
        covered as f64 / total as f64
    };
    let any_recall = if target_levels.is_empty() {
        0.0
    } else {
        covered_targets as f64 / target_levels.len() as f64
    };

    let denominator = top_k.min(total_chunks);
    let precision = |matches| {
        if denominator == 0 {
            0.0
        } else {
            matches as f64 / denominator as f64
        }
    };
    // Related precision excludes Strong-matching chunks from the denominator:
    // a Strong hit is neither a Related success nor a Related miss, so the
    // Related layer is measured over the non-Strong retrieval slots only.
    // This keeps the `_r` columns independent of Strong ranking instead of
    // duplicating the `_a` (any) columns.
    let precision_related = {
        let related_denominator = denominator.saturating_sub(strong_matches);
        if related_denominator == 0 {
            0.0
        } else {
            related_matches as f64 / related_denominator as f64
        }
    };

    let precision_strong = precision(strong_matches);
    let precision_any = precision(any_matches);
    let recall_strong = exact_recall(RelevanceLevel::Strong);
    let recall_related = exact_recall(RelevanceLevel::Related);
    let recall_any = any_recall;

    RangeBasedPerQueryScore {
        strong_matches,
        related_matches,
        any_matches,
        total_relevant: covered_targets,
        precision_strong,
        precision_related,
        precision_any,
        recall_strong,
        recall_related,
        recall_any,
        f1_strong: harmonic_mean(precision_strong, recall_strong),
        f1_related: harmonic_mean(precision_related, recall_related),
        f1_any: harmonic_mean(precision_any, recall_any),
        f1_score: harmonic_mean(precision_any, recall_any),
    }
}

/// Single-pass evaluator over an already-ranked chunk list.
///
/// The retrieved chunks are accumulated as the scan advances, and the
/// coverage of the labeled targets is recomputed for every cutoff: whether
/// a target was recovered depends on all chunks retrieved so far, because
/// the retrieval may split a target across several chunks.
///
/// Overlap-based first-hit positions and redundant-coverage counts
/// accumulate monotonically, so one pass over the ranked prefix (up to the
/// largest requested cutoff) yields the results for every cutoff. Each
/// `advance` returns the chunk's overlap relation so callers that build
/// top-k detail rows do not re-evaluate it. Judgment range paths and each
/// chunk's file path are normalized once per scan.
pub struct RankedScan<'j> {
    judgment: &'j RelevanceJudgment,
    target_levels: Vec<RelevanceLevel>,
    normalized_range_files: Vec<String>,
    retrieved: Vec<RetrievedCoverage>,
    first_hit: Vec<Option<usize>>,
    redundant: usize,
    total_chunks: usize,
    requested_cutoffs: Vec<usize>,
    ordered_cutoffs: Vec<usize>,
    snapshots: Vec<(RangeBasedPerQueryScore, CutoffDiagnostics)>,
    next_cutoff: usize,
    position: usize,
}

impl<'j> RankedScan<'j> {
    /// Prepare a scan over a ranked list of `total_chunks` entries.
    ///
    /// `cutoffs` may be unsorted or duplicated; results come back in the
    /// requested order.
    pub fn new(judgment: &'j RelevanceJudgment, cutoffs: &[usize], total_chunks: usize) -> Self {
        let mut ordered_cutoffs: Vec<usize> = cutoffs.to_vec();
        ordered_cutoffs.sort_unstable();
        ordered_cutoffs.dedup();
        Self {
            judgment,
            target_levels: judgment
                .relevant_ranges
                .iter()
                .map(|(_, level)| *level)
                .collect(),
            normalized_range_files: judgment
                .relevant_ranges
                .iter()
                .map(|(range, _)| normalize_path(&range.file))
                .collect(),
            retrieved: Vec::new(),
            first_hit: vec![None; judgment.relevant_ranges.len()],
            redundant: 0,
            total_chunks,
            requested_cutoffs: cutoffs.to_vec(),
            ordered_cutoffs,
            snapshots: Vec::new(),
            next_cutoff: 0,
            position: 0,
        }
    }

    /// Advance the scan by one ranked position.
    pub fn advance(&mut self, chunk: &ChunkData) -> ChunkRelevance {
        self.position += 1;
        let chunk_file = normalize_path(&chunk.file_path);
        let relevance = relevance_matches(
            &chunk_file,
            &chunk.source_ranges,
            &self.judgment.relevant_ranges,
            &self.normalized_range_files,
        );
        self.retrieved.push(RetrievedCoverage {
            file: chunk_file,
            ranges: chunk.source_ranges.clone(),
        });
        for (target, _) in &relevance.matched_targets {
            if self.first_hit[*target].is_none() {
                self.first_hit[*target] = Some(self.position);
            }
        }
        self.redundant += relevance.matched_targets.len();
        self.snapshot_if_due();
        relevance
    }

    fn snapshot_if_due(&mut self) {
        while self.next_cutoff < self.ordered_cutoffs.len()
            && self.position >= self.ordered_cutoffs[self.next_cutoff]
        {
            let top_k = self.ordered_cutoffs[self.next_cutoff];
            self.snapshots.push(self.snapshot_at(top_k));
            self.next_cutoff += 1;
        }
    }

    fn snapshot_at(&self, top_k: usize) -> (RangeBasedPerQueryScore, CutoffDiagnostics) {
        let prefix = top_k.min(self.retrieved.len());
        let coverage = coverage_over_retrieved(self.judgment, &self.retrieved[..prefix]);
        let range = finalize_range_score(&self.target_levels, &coverage, top_k, self.total_chunks);
        let hits: Vec<f64> = self
            .first_hit
            .iter()
            .flatten()
            .map(|&rank| rank as f64)
            .collect();
        let avg = if hits.is_empty() {
            None
        } else {
            Some(hits.iter().sum::<f64>() / hits.len() as f64)
        };
        (
            range,
            CutoffDiagnostics {
                avg_first_hit_rank: avg,
                redundant_coverage: self.redundant,
            },
        )
    }

    /// Complete the scan and return per-cutoff results in requested order.
    pub fn finish(mut self) -> Vec<(RangeBasedPerQueryScore, CutoffDiagnostics)> {
        while self.next_cutoff < self.ordered_cutoffs.len() {
            let top_k = self.ordered_cutoffs[self.next_cutoff];
            self.snapshots.push(self.snapshot_at(top_k));
            self.next_cutoff += 1;
        }
        let by_cutoff: HashMap<usize, (RangeBasedPerQueryScore, CutoffDiagnostics)> = self
            .ordered_cutoffs
            .iter()
            .copied()
            .zip(self.snapshots)
            .collect();
        self.requested_cutoffs
            .iter()
            .filter_map(|k| by_cutoff.get(k).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::{QueryType, SourceRange};

    fn range(
        start_line: usize,
        end_line: usize,
        level: RelevanceLevel,
    ) -> (SourceRange, RelevanceLevel) {
        (
            SourceRange {
                file: "src/lib.rs".to_string(),
                start_line,
                end_line,
            },
            level,
        )
    }

    fn chunk(kind: &str, ranges: Vec<ChunkSourceRange>) -> ChunkData {
        ChunkData {
            chunk_id: "chunk".to_string(),
            entity_name: "entity".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            end_line: 100,
            source_ranges: ranges,
            source_span_kind: kind.to_string(),
            test_info: cce_types::TestInfo::unknown(),
            language: None,
            entity_ids: Vec::new(),
            segment_id: String::new(),
        }
    }

    fn judgment_with(ranges: Vec<(SourceRange, RelevanceLevel)>) -> RelevanceJudgment {
        RelevanceJudgment {
            id: "coverage".to_string(),
            query_text: "coverage".to_string(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: ranges,
        }
    }

    #[test]
    fn group_fallback_range_is_a_source_match() {
        let judgment = judgment_with(vec![range(5, 10, RelevanceLevel::Strong)]);
        let chunk = chunk(
            "group_fallback",
            vec![ChunkSourceRange {
                start_line: 5,
                end_line: 10,
            }],
        );

        assert_eq!(
            is_relevant_to_query(&chunk, &judgment),
            Some(RelevanceLevel::Strong)
        );
    }

    #[test]
    fn all_overlapping_targets_contribute_to_any_recall() {
        let judgment = judgment_with(vec![
            range(5, 10, RelevanceLevel::Strong),
            range(30, 35, RelevanceLevel::Related),
        ]);
        let chunk = chunk(
            "exact_entities",
            vec![
                ChunkSourceRange {
                    start_line: 5,
                    end_line: 10,
                },
                ChunkSourceRange {
                    start_line: 30,
                    end_line: 35,
                },
            ],
        );

        let relevance = relevance_for_chunk(&chunk, &judgment);
        assert_eq!(relevance.matched_targets.len(), 2);
        let score = evaluate_query_range_based(&judgment, &[chunk], &[1.0], 1);
        assert_eq!(score.total_relevant, 2);
        assert_eq!(score.recall_any, 1.0);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.recall_related, 1.0);
    }

    fn ranged_chunk(start_line: usize, end_line: usize) -> ChunkData {
        chunk(
            "exact_entities",
            vec![ChunkSourceRange {
                start_line,
                end_line,
            }],
        )
    }

    fn empty_chunk() -> ChunkData {
        chunk("exact_entities", Vec::new())
    }

    #[test]
    fn strong_only_query_keeps_related_layer_at_zero() {
        let judgment = judgment_with(vec![range(5, 10, RelevanceLevel::Strong)]);
        let chunks = vec![ranged_chunk(5, 10)];
        let score = evaluate_query_range_based(&judgment, &chunks, &[1.0], 5);
        assert_eq!(score.strong_matches, 1);
        assert_eq!(score.related_matches, 0);
        assert_eq!(score.any_matches, 1);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.recall_related, 0.0);
        assert_eq!(score.recall_any, 1.0);
        assert_eq!(score.precision_related, 0.0);
        assert!(score.precision_any > 0.0);
    }

    #[test]
    fn related_precision_excludes_strong_slots() {
        let judgment = judgment_with(vec![
            range(5, 10, RelevanceLevel::Strong),
            range(30, 35, RelevanceLevel::Related),
        ]);
        let chunks = vec![
            ranged_chunk(5, 10),
            ranged_chunk(30, 35),
            empty_chunk(),
            empty_chunk(),
            empty_chunk(),
        ];
        let scores = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &chunks, &scores, 5);
        assert_eq!(score.strong_matches, 1);
        assert_eq!(score.related_matches, 1);
        assert_eq!(score.any_matches, 2);
        assert_eq!(score.precision_strong, 1.0 / 5.0);
        assert_eq!(score.precision_related, 1.0 / 4.0);
        assert_eq!(score.precision_any, 2.0 / 5.0);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.recall_related, 1.0);
        assert_eq!(score.recall_any, 1.0);

        // Dropping the Related hit separates Related recall from any recall.
        let strong_only = vec![ranged_chunk(5, 10), empty_chunk()];
        let strong_scores = vec![2.0, 1.0];
        let partial = evaluate_query_range_based(&judgment, &strong_only, &strong_scores, 2);
        assert_eq!(partial.recall_strong, 1.0);
        assert_eq!(partial.recall_related, 0.0);
        assert_eq!(partial.recall_any, 0.5);
        assert_ne!(partial.recall_related, partial.recall_any);
    }

    #[test]
    fn unavailable_ranges_do_not_use_navigation_span() {
        let judgment = judgment_with(vec![range(5, 10, RelevanceLevel::Strong)]);
        let chunk = chunk("unavailable", Vec::new());

        assert_eq!(is_relevant_to_query(&chunk, &judgment), None);
    }

    #[test]
    fn a_grazing_chunk_does_not_recover_the_target() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);

        // The chunk carries one line from inside the target, so it overlaps
        // the target but does not cover it: the target is not recovered.
        let grazing = vec![ranged_chunk(30, 30)];
        let score = evaluate_query_range_based(&judgment, &grazing, &[1.0], 1);
        assert_eq!(score.total_relevant, 0);
        assert_eq!(score.recall_strong, 0.0);
        assert_eq!(score.strong_matches, 0);

        // A chunk that covers the target recovers it.
        let covering = vec![ranged_chunk(20, 50)];
        let score = evaluate_query_range_based(&judgment, &covering, &[1.0], 1);
        assert_eq!(score.total_relevant, 1);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.strong_matches, 1);
    }

    #[test]
    fn fragments_that_together_cover_the_target_recover_it() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);

        // The retrieval split the target: one chunk carries the first half,
        // the other the second half. Together they recover it.
        let chunks = vec![ranged_chunk(25, 34), ranged_chunk(35, 44)];
        let scores = vec![2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &chunks, &scores, 2);
        assert_eq!(score.total_relevant, 1);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.recall_any, 1.0);
        assert_eq!(
            score.strong_matches, 2,
            "both fragments contribute to the recovery"
        );
        assert_eq!(score.any_matches, 2);
    }

    #[test]
    fn touching_fragments_merge_for_coverage() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);
        let abutting = vec![ranged_chunk(25, 34), ranged_chunk(35, 44)];
        let scores = vec![2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &abutting, &scores, 2);
        assert_eq!(score.total_relevant, 1);
        assert_eq!(score.recall_any, 1.0);
    }

    #[test]
    fn blank_line_gap_stays_disjoint() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);
        let gapped = vec![ranged_chunk(25, 34), ranged_chunk(36, 44)];
        let scores = vec![2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &gapped, &scores, 2);
        assert_eq!(score.total_relevant, 0);
        assert_eq!(score.recall_any, 0.0);
    }

    #[test]
    fn a_chunk_repeating_covered_code_does_not_contribute() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);

        // The first chunk covers the whole target; the second repeats a line
        // of it and adds nothing to the recovery.
        let chunks = vec![ranged_chunk(25, 44), ranged_chunk(30, 30)];
        let scores = vec![2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &chunks, &scores, 2);
        assert_eq!(score.total_relevant, 1);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(
            score.strong_matches, 1,
            "the redundant chunk is not counted as a hit"
        );
    }

    #[test]
    fn coverage_counts_only_the_lines_inside_the_target() {
        let judgment = judgment_with(vec![range(25, 44, RelevanceLevel::Strong)]);

        // The first chunk covers the first half of the target and a line
        // beyond it; the second covers the rest.
        let chunks = vec![ranged_chunk(20, 34), ranged_chunk(35, 50)];
        let scores = vec![2.0, 1.0];
        let score = evaluate_query_range_based(&judgment, &chunks, &scores, 2);
        assert_eq!(score.total_relevant, 1);
        assert_eq!(score.recall_strong, 1.0);
        assert_eq!(score.strong_matches, 2);
    }
}
