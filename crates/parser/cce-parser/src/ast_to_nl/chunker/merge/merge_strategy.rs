//! Merge strategy: cost calculation, can-merge decision, and merge priority.
//!
//! Extracted from `merge.rs` so the mechanical merge (`merge_two_chunks`,
//! `merge_source_coverage`) stays separate from the policy that decides
//! *when* and *which* chunks to merge. All three policy pieces share the
//! single source of truth in `boundary::{cost, can_merge, merge_limits}`.

use std::collections::HashMap;

use cce_types::Span;

use crate::ast_to_nl::chunker::boundary::{can_merge, cost};
use crate::ast_to_nl::chunker::config::ChunkingConfig;
use crate::ast_to_nl::chunker::result::{ChunkPath, ChunkedResult};

/// Calculate the combined cost of two adjacent chunks.
///
/// The merged text is `a.text + "\n\n" + b.text`, identical to the text
/// produced by `merge_two_chunks`. Cost is path-dependent:
/// - BM25: word count
/// - Embedding: estimated tokens
pub(crate) fn combined_cost(a: &ChunkedResult, b: &ChunkedResult, path: ChunkPath) -> usize {
    cost(&format!("{}\n\n{}", a.text, b.text), path)
}

/// Whether `prev` should absorb `next` according to merge policy.
///
/// Combines three checks:
/// 1. Self-contained exemption (Embedding-only): a chunk marked
///    `self_contained` (own docstring/behavior) keeps its pure topic and
///    never merges with neighbors on the Embedding path. This overrides the
///    size threshold.
/// 2. Test-boundary guard: a test chunk must never merge with a
///    non-test chunk (and vice versa). Merging would mark production
///    content as `Test` (or dilute a test chunk), causing the no-test
///    evaluation variant to drop production content. This mirrors the
///    group-level guard in `SmallFragmentMerger`.
/// 3. Size threshold via `boundary::can_merge`: the leading chunk must be
///    below the path's min threshold and the combined cost must stay within
///    the path's merge ceiling.
///
/// This is the single merge decision shared by the intra-splitter pass and
/// the cross-group pass; using one function prevents the two layers from
/// drifting apart.
pub(crate) fn should_merge(
    prev: &ChunkedResult,
    next: &ChunkedResult,
    path: ChunkPath,
    config: &ChunkingConfig,
) -> bool {
    let self_contained_blocks =
        path == ChunkPath::Embedding && (prev.self_contained || next.self_contained);
    if self_contained_blocks {
        return false;
    }
    if prev.metadata.test_info.is_test() != next.metadata.test_info.is_test() {
        return false;
    }
    can_merge(&prev.text, &next.text, path, config)
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
