use std::ops::Range;

use cce_config::modules::ChunkingConfig;
use cce_types::Span;

use crate::grouper::EntityGroup;

use super::super::boundary::{ChunkBoundary, SplitReason, cost};
use super::super::result::ChunkPath;
use super::lines::extend_or_push_trailing;

/// Partition `text[range]` at nested-group spans.
///
/// Nested-group spans from the grouper are source-code byte offsets, which
/// cannot slice natural-language text; this strategy maps them to NL-text
/// positions by locating each nested group's name inside the range. Returns
/// an empty list when no nested-group data is available in the range — the
/// caller (`split_range`) descends to a coarser boundary source.
pub fn split_by_nested_groups(
    text: &str,
    group: &EntityGroup,
    path: ChunkPath,
    _config: &ChunkingConfig,
    range: Range<usize>,
) -> Vec<ChunkBoundary> {
    let spans = locate_nested_spans_in_nl_text(text, group, &range);
    if spans.is_empty() {
        return Vec::new();
    }

    let base = range.start;
    let end = range.end;

    let mut boundaries: Vec<ChunkBoundary> = Vec::new();
    let mut current_start = base;

    for (group_id, span) in spans {
        let start_byte = span.start_byte.max(base).min(end);
        let end_byte = span.end_byte.max(base).min(end);
        if start_byte >= end_byte {
            continue;
        }

        // Gap before this span: emit it as its own piece (content) or fold a
        // whitespace-only gap into the previous piece to avoid hollow chunks.
        if start_byte > current_start {
            let gap_text = &text[current_start..start_byte];
            if gap_text.trim().is_empty() {
                if let Some(last) = boundaries.last_mut() {
                    if last.end_byte < start_byte {
                        last.end_byte = start_byte;
                    }
                }
            } else {
                boundaries.push(
                    ChunkBoundary::new(current_start, start_byte, SplitReason::MemberBoundary)
                        .with_token_count(cost(gap_text, path)),
                );
            }
        }

        // The span piece starts where the previous piece ended (folding any
        // leading whitespace into the first span piece).
        let piece_start = boundaries.last().map(|b| b.end_byte).unwrap_or(base);
        let nested_text = &text[piece_start..end_byte];
        let tokens = cost(nested_text, path);

        let boundary = ChunkBoundary::new(piece_start, end_byte, SplitReason::MemberBoundary)
            .with_token_count(tokens)
            .with_group_id(Some(group_id));
        boundaries.push(boundary);
        current_start = end_byte;
    }

    extend_or_push_trailing(
        &mut boundaries,
        text,
        current_start,
        end,
        path,
        SplitReason::MemberBoundary,
    );

    boundaries
}

/// Locate nested-group names in `text[range]`, returning NL-text-relative
/// byte ranges (group_id, span) sorted and de-overlapped.
fn locate_nested_spans_in_nl_text(
    text: &str,
    group: &EntityGroup,
    range: &Range<usize>,
) -> Vec<(String, Span)> {
    use cce_utils::text::split_camel_case;

    let sub = &text[range.clone()];
    let lower_sub = sub.to_lowercase();

    let mut spans = Vec::new();
    for nested in &group.nested_groups {
        let name = nested.name.as_str();
        let lower_name = name.to_lowercase();

        let (rel, len) = if let Some(rel) = lower_sub.find(&lower_name) {
            (rel, name.len())
        } else {
            let semantic = split_camel_case(name);
            let lower_semantic = semantic.to_lowercase();
            if !lower_semantic.is_empty() && lower_semantic != lower_name {
                if let Some(rel) = lower_sub.find(&lower_semantic) {
                    (rel, semantic.len())
                } else {
                    continue;
                }
            } else if let Some(last) = name.rsplit("::").next()
                && last != name
            {
                let lower_last = last.to_lowercase();
                match lower_sub.find(&lower_last) {
                    Some(rel) => (rel, last.len()),
                    None => continue,
                }
            } else {
                continue;
            }
        };
        spans.push((
            nested.group_id.to_string(),
            Span::new(range.start + rel, range.start + rel + len, 0, 0, 0, 0),
        ));
    }

    spans.sort_by_key(|(_, s)| (s.start_byte, s.end_byte));
    let spans: Vec<(String, Span)> = spans.into_iter().fold(Vec::new(), |mut acc, item| {
        if acc
            .last()
            .is_none_or(|(_, last): &(String, Span)| item.1.start_byte >= last.end_byte)
        {
            acc.push(item);
        }
        acc
    });
    spans
}

#[cfg(test)]
mod tests {
    use cce_config::modules::ChunkingConfig;

    use crate::grouper::types::EntityGroup;

    use super::super::super::result::ChunkPath;
    use super::*;

    #[test]
    fn test_split_by_nested_groups_no_nested_data_returns_empty() {
        let config = ChunkingConfig::default();

        let text = "some text without nested groups";
        let group = EntityGroup {
            nested_groups: Box::new([]),
            ..Default::default()
        };

        let boundaries =
            split_by_nested_groups(text, &group, ChunkPath::Embedding, &config, 0..text.len());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_split_by_nested_groups_located_by_name() {
        let config = ChunkingConfig::default();

        let text = "outer docs. Inner Config handles settings.";
        let group = EntityGroup {
            group_id: compact_str::CompactString::from("outer"),
            name: compact_str::CompactString::from("Outer"),
            nested_groups: Box::new([
                EntityGroup {
                    group_id: compact_str::CompactString::from("inner"),
                    name: compact_str::CompactString::from("Inner"),
                    ..Default::default()
                },
                EntityGroup {
                    group_id: compact_str::CompactString::from("config"),
                    name: compact_str::CompactString::from("Config"),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let boundaries =
            split_by_nested_groups(text, &group, ChunkPath::Embedding, &config, 0..text.len());
        assert!(boundaries.len() >= 2);
        assert_eq!(boundaries.first().unwrap().start_byte, 0);
        assert_eq!(boundaries.last().unwrap().end_byte, text.len());
        let mut prev = 0;
        for b in &boundaries {
            assert_eq!(b.start_byte, prev, "partition gap/overlap");
            assert!(b.start_byte < b.end_byte);
            assert!(!text[b.start_byte..b.end_byte].trim().is_empty());
            prev = b.end_byte;
        }
        assert_eq!(prev, text.len());
    }

    #[test]
    fn test_split_by_nested_groups_name_not_present_returns_empty() {
        let config = ChunkingConfig::default();

        let text = "completely unrelated prose";
        let group = EntityGroup {
            nested_groups: Box::new([EntityGroup {
                group_id: compact_str::CompactString::from("inner"),
                name: compact_str::CompactString::from("MissingSymbol"),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let boundaries =
            split_by_nested_groups(text, &group, ChunkPath::Embedding, &config, 0..text.len());
        assert!(boundaries.is_empty());
    }
}
