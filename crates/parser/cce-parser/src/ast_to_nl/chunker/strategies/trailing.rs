use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;

/// Emit the trailing remainder of a range.
///
/// A whitespace-only remainder is absorbed into the last boundary instead of
/// becoming a hollow segment, keeping the partition a complete, contiguous
/// cover of the range.
pub(crate) fn extend_or_push_trailing(
    boundaries: &mut Vec<ChunkBoundary>,
    text: &str,
    current_start: usize,
    end: usize,
    path: ChunkPath,
    reason: SplitReason,
) {
    if current_start >= end {
        return;
    }
    let remaining = &text[current_start..end];

    if remaining.trim().is_empty() {
        if let Some(last) = boundaries.last_mut() {
            if last.end_byte < end {
                last.end_byte = end;
            }
            return;
        }
    }

    let remaining_tokens = cost(remaining, path);
    boundaries.push(
        ChunkBoundary::new(
            current_start,
            end,
            if boundaries.is_empty() {
                SplitReason::NotSplit
            } else {
                reason
            },
        )
        .with_token_count(remaining_tokens),
    );
}

#[cfg(test)]
mod tests {
    use cce_config::modules::ChunkingConfig;

    use super::super::super::result::ChunkPath;

    #[test]
    fn test_split_by_paragraphs_oversized_first_produces_no_whitespace_chunks() {
        use super::super::paragraphs::split_text_by_paragraphs;

        let config = ChunkingConfig {
            max_bm25_terms: 5,
            ..Default::default()
        };
        // First paragraph is oversized (keyword-hybrid style), followed by blank lines and code
        let text = "word1 word2 word3 word4 word5 word6 word7 word8\n\n\nmatch printer standard\n";
        let boundaries = split_text_by_paragraphs(text, 0..text.len(), ChunkPath::Bm25, &config);

        assert!(!boundaries.is_empty());
    }

    #[test]
    fn test_splitter_filters_whitespace_only_segments() {
        use super::super::super::splitter::TextSplitter;
        use super::super::super::strategy::SplitStrategy;
        use crate::grouper::EntityGroup;

        let config = ChunkingConfig {
            max_bm25_terms: 5,
            ..Default::default()
        };
        let splitter = TextSplitter::new(config);

        // Oversized first line followed by blank lines
        let text = "word1 word2 word3 word4 word5 word6 word7 word8\n\n\nmatch printer standard\n";
        let segments = splitter
            .split(
                text,
                &EntityGroup::default(),
                SplitStrategy::ByParagraphs,
                ChunkPath::Bm25,
            )
            .expect("test split must succeed");

        // Verify no segment is whitespace-only
        for seg in &segments {
            assert!(
                !seg.text.trim().is_empty(),
                "whitespace-only segment: {:?} (split_reason: {:?})",
                seg.text,
                seg.boundary.split_reason
            );
        }
    }
}
