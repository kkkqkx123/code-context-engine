//! Merge strategy: cost calculation, can-merge decision, and merge priority.
//!
//! Extracted from `merge.rs` so the mechanical merge (`merge_two_chunks`,
//! `merge_source_coverage`) stays separate from the policy that decides
//! *when* and *which* chunks to merge. All three policy pieces share the
//! single source of truth in `boundary::{cost, merge_limits}`.

use std::collections::HashMap;

use cce_types::Span;

use crate::ast_to_nl::chunker::SplitReason;
use crate::ast_to_nl::chunker::boundary::{cost, merge_limits};
use crate::ast_to_nl::chunker::chunk_builder::ChunkBuilder;
use crate::ast_to_nl::chunker::config::ChunkingConfig;
use crate::ast_to_nl::chunker::result::{ChunkPath, ChunkedResult};

/// Calculate the combined cost of two adjacent chunks.
///
/// The merged text is `a.text + "\n\n" + b.text`, identical to the text
/// produced by `merge_two_chunks`. Cost is path-dependent:
/// - BM25: terms produced by the index tokenizer
/// - Embedding: estimated tokens
pub(crate) fn combined_cost(a: &ChunkedResult, b: &ChunkedResult, path: ChunkPath) -> usize {
    cost(&format!("{}\n\n{}", a.text, b.text), path)
}

/// Whether `prev` should absorb `next` according to merge policy.
///
/// Combines three checks:
/// 1. Self-contained exemption (Embedding-only): a single-topic chunk
///    marked `self_contained` (own docstring/behavior) keeps its pure topic
///    and never merges with neighbors on the Embedding path — but only
///    while the self-contained side itself is still below the path's min
///    threshold. An already-large self-contained chunk must not strand a
///    tiny neighbor fragment (bare fields, header-only residue), and a
///    sub-entity fragment (sentence/line/token split) never claims topic
///    purity in the first place. A chunk carrying several content entities
///    is already mixed-topic, so merging it cannot dilute a pure topic and
///    it never vetoes either. Once every self-contained side is an
///    above-min topic unit, size-based merging applies.
///    This overrides the size threshold.
/// 2. Test-boundary guard: a test chunk must never merge with a
///    non-test chunk (and vice versa). Merging would mark production
///    content as `Test` (or dilute a test chunk), causing the no-test
///    evaluation variant to drop production content. This mirrors the
///    group-level guard in `SmallFragmentMerger`.
/// 3. Kind affinity: a merged chunk keeps the left chunk's identity (its
///    title, kind and keywords), so the right side must describe the same
///    kind of entity. A macro group that absorbs the following function
///    produces a unit titled after the macro whose body is an unrelated
///    function, and the mismatch in the other direction is just as
///    misleading. Only peers of one kind may merge across groups.
/// 4. Size threshold via `boundary::merge_limits`: either side below the
///    path's min threshold and the combined cost within the merge ceiling.
///
/// The intra-splitter pass uses the same limits, so the two layers cannot
/// drift apart.
pub(crate) fn should_merge(
    prev: &ChunkedResult,
    next: &ChunkedResult,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> bool {
    let budget = config.budget(path, false, 0);
    if path == ChunkPath::Embedding && (prev.self_contained || next.self_contained) {
        let (min_threshold, _) = merge_limits(path, &budget);
        let protects_small_topic =
            (prev.self_contained && is_topic_unit(prev) && cost(&prev.text, path) < min_threshold)
                || (next.self_contained
                    && is_topic_unit(next)
                    && cost(&next.text, path) < min_threshold);
        if protects_small_topic {
            return false;
        }
    }
    if prev.metadata.test_info.is_test() != next.metadata.test_info.is_test() {
        return false;
    }
    // Tiny-rescue (Embedding-only): a residue far below the min threshold
    // (sentence splinters, bare fields, header-only tails) is a poor
    // retrieval unit on its own. Within one group such fragments only
    // rejoin the unit they were split from, so fold a tiny side back into
    // its neighbor as long as the combined text stays within the hard
    // limit. A tiny chunk whose source span is contained in the neighbor's
    // span is the same code unit described twice (struct plus its field,
    // overlapping sentence splinters): reuniting them only removes a
    // duplicate claim, even across groups. Gluing span-disjoint entities
    // would mislabel the merged chunk, so the kind gate below still
    // applies there. Documented topics are protected by the veto above
    // and never reach this branch; the test boundary is never crossed.
    if path == ChunkPath::Embedding {
        let (min_threshold, _) = merge_limits(path, &budget);
        let tiny = min_threshold / 3;
        let prev_cost = cost(&prev.text, path);
        let next_cost = cost(&next.text, path);
        if (prev_cost <= tiny || next_cost <= tiny)
            && combined_cost(prev, next, path) <= budget.hard_limit
            && (prev.source_group_id == next.source_group_id
                || spans_contain_or_contained(
                    &prev.metadata.source_span,
                    &next.metadata.source_span,
                ))
        {
            return true;
        }
    }
    if !shares_kind(prev, next) {
        return false;
    }
    // A chunk that states no topic of its own inherits the topic of the
    // neighbour it follows. Size alone does not make a chunk self-sufficient:
    // a dense body of code says nothing about its subject, so a chunk that
    // says nothing is a poor retrieval unit even when it is long. The
    // neighbour keeps the merged chunk's identity, so the topic travels
    // with the merge.
    if path == ChunkPath::Embedding
        && ChunkBuilder::states_own_topic(&prev.text)
        && !ChunkBuilder::states_own_topic(&next.text)
        && is_topic_unit(prev)
    {
        let (_, ceiling) = merge_limits(path, &budget);
        if combined_cost(prev, next, path) <= ceiling {
            return true;
        }
    }
    let (min_threshold, ceiling) = merge_limits(path, &budget);
    let prev_cost = cost(&prev.text, path);
    let next_cost = cost(&next.text, path);
    let combined = combined_cost(prev, next, path);
    combined <= ceiling && (prev_cost < min_threshold || next_cost < min_threshold)
}

/// Whether one source span contains the other.
///
/// A tiny chunk whose span sits inside the neighbor's span (or vice versa)
/// describes the same code unit twice: reuniting them only removes a
/// duplicate claim. Spans without source locations never count as
/// contained, so chunks that lost their positions can never glue.
fn spans_contain_or_contained(a: &Span, b: &Span) -> bool {
    if !a.is_available() || !b.is_available() {
        return false;
    }
    (a.start_byte <= b.start_byte && b.end_byte <= a.end_byte)
        || (b.start_byte <= a.start_byte && a.end_byte <= b.end_byte)
}

/// Whether two chunks describe the same kind of entity.
///
/// The identity a merge keeps is the left chunk's, so merging chunks of
/// different kinds would label the right side's content with the left
/// side's kind. A chunk that carries no kind states nothing about its
/// content, so it cannot contradict the other side.
fn shares_kind(prev: &ChunkedResult, next: &ChunkedResult) -> bool {
    match (prev.metadata.entity_kind(), next.metadata.entity_kind()) {
        (Some(prev_kind), Some(next_kind)) => prev_kind == next_kind,
        _ => true,
    }
}

/// Whether a chunk is an entity-aligned unit that may claim topic purity.
///
/// A chunk with several content entities is already mixed-topic: merging it
/// with a neighbor cannot dilute a pure topic, so it never vetoes a rescue
/// merge. Sub-entity fragments (sentence/line/token splits of a larger member)
/// carry their parent's descriptor by reference only — the documented
/// entity they describe lives in another chunk. Letting such a fragment
/// veto merges strands neighboring residues (bare fields, header-only
/// tails) that can never merge anywhere else.
///
/// The group header is context, not content: a first-block chunk holds the
/// header plus one member, but its topic is still the member's own.
/// Counting the header as a second entity would disqualify every
/// first-block chunk from topic protection and let small documented
/// members be absorbed by their neighbors.
fn is_topic_unit(chunk: &ChunkedResult) -> bool {
    let content = chunk.metadata.content_entity_ids();
    // Chunks without any entity attribution keep the legacy answer below.
    let single_unit = if content.is_empty() {
        true
    } else {
        let context = chunk.metadata.context_entity_ids();
        let mut non_header = content.iter().filter(|id| !context.contains(id));
        non_header.next().is_some() && non_header.next().is_none()
    };
    if !single_unit {
        return false;
    }
    match chunk.metadata.as_code() {
        Some(code) => matches!(
            code.split_reason,
            SplitReason::MemberBoundary | SplitReason::NotSplit
        ),
        None => true,
    }
}

/// Sort chunks by source position (group span) for deterministic merge priority.
///
/// Merge priority is source-position order: chunks are merged left-to-right in
/// file order, adjacent groups only. Groups missing from `group_spans` sort
/// first (start_byte 0) which preserves a stable order without panicking on
/// incomplete span maps.
pub(crate) fn sort_chunks_by_span(
    chunks: &mut [ChunkedResult],
    group_spans: &HashMap<String, Span>,
) {
    chunks.sort_by_key(|c| {
        group_spans
            .get(&c.source_group_id)
            .map(|s| s.start_byte)
            .unwrap_or(0)
    });
}
