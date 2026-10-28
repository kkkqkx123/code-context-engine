//! Per-segment max limit enforcement.
//!
//! `TextSplitter::split_range` already guarantees that every produced piece
//! stays within the path limit, so segments coming out of the splitter are
//! compliant. This module keeps a thin adapter for callers that hold
//! arbitrary segments (e.g. oversized segments built by hand): each oversized
//! segment is re-split through the unified `split_range` recursion instead of
//! a separate count-based fallthrough chain.

use crate::grouper::EntityGroup;
use cce_types::ParseError;

use super::boundary::{ChunkSegment, NlEntityBoundary};
use super::chunker::ChunkInfrastructure;
use super::result::ChunkPath;
use super::strategy::SplitStrategy;

/// Enforce per-segment max limit after splitting.
///
/// Each oversized segment is re-split through the unified `split_range`
/// recursion (starting at the entity boundary level); compliant segments are
/// never touched. `split_by_tokens` inside the recursion is the
/// guaranteed-to-succeed terminal and must stay the chain's last resort.
pub(crate) fn enforce_segment_max_limit(
    segments: Vec<ChunkSegment>,
    path: ChunkPath,
    infra: &ChunkInfrastructure,
    group: &EntityGroup,
    nl_boundaries: &[NlEntityBoundary],
) -> Result<Vec<ChunkSegment>, ParseError> {
    let budget = infra.budget(path);
    if budget.hard_limit == 0 {
        return Ok(segments);
    }
    // Retain segments that fit the pre-injection budget, matching the
    // splitter's small over-limit retention: re-splitting them only strands
    // tail fragments while the hard limit still holds.
    let limit = budget.pre_injection_limit;

    let mut validated = Vec::with_capacity(segments.len());
    for seg in segments {
        if infra.config.cost(&seg.text, path) <= limit {
            validated.push(seg);
            continue;
        }
        validated.extend(re_split_segment(&seg, path, infra, group, nl_boundaries)?);
    }
    Ok(validated)
}

/// Re-split a single oversized segment through `split_range`.
///
/// `nl_boundaries` are full-text offsets; they are shifted to be local to the
/// segment so the recursion sees one consistent coordinate system, then every
/// produced piece is rebased back to full-text offsets.
fn re_split_segment(
    seg: &ChunkSegment,
    path: ChunkPath,
    infra: &ChunkInfrastructure,
    group: &EntityGroup,
    nl_boundaries: &[NlEntityBoundary],
) -> Result<Vec<ChunkSegment>, ParseError> {
    let base = seg.boundary.start_byte;
    let seg_end = base + seg.text.len();

    let shifted: Vec<NlEntityBoundary> = nl_boundaries
        .iter()
        .filter(|b| b.start_byte < seg_end && b.end_byte > base)
        .map(|b| NlEntityBoundary {
            entity_id: b.entity_id,
            start_byte: b.start_byte.saturating_sub(base),
            end_byte: b.end_byte.saturating_sub(base),
        })
        .collect();

    let boundaries = infra.splitter.try_split_range(
        &seg.text,
        group,
        SplitStrategy::ByNlEntityBoundaries,
        path,
        Some(&shifted),
        0..seg.text.len(),
    )?;

    let mut result = Vec::with_capacity(boundaries.len());
    for mut b in boundaries {
        let start = b.start_byte;
        let end = b.end_byte;
        if start >= end
            || end > seg.text.len()
            || !seg.text.is_char_boundary(start)
            || !seg.text.is_char_boundary(end)
        {
            return Err(ParseError::code_splitting(format!(
                "re-split boundary [{start}, {end}) is invalid for segment of length {}",
                seg.text.len()
            )));
        }
        if b.entity_ids.is_empty() {
            b.entity_ids = seg.boundary.entity_ids.clone();
        }
        b.start_byte = base + start;
        b.end_byte = base + end;
        result.push(ChunkSegment::new(b, seg.text[start..end].to_string()));
    }
    Ok(result)
}
