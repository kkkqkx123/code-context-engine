use super::Comment;
use super::classifier::{CommentClass, classify_comment};
use cce_types::Entity;

/// Merge consecutive plain comments (row difference ≤ 1) into single blocks
/// whose span covers the whole run.
pub(crate) fn merge_plain_comment_blocks(comments: &[Comment]) -> Vec<Comment> {
    let mut merged: Vec<Comment> = Vec::new();

    for comment in comments {
        if classify_comment(comment) != CommentClass::Plain {
            continue;
        }

        if let Some(last) = merged.last_mut() {
            let consecutive = comment.span.start_position.row == last.span.end_position.row
                || comment.span.start_position.row == last.span.end_position.row + 1;

            if consecutive {
                if !last.text.ends_with('\n') {
                    last.text.push('\n');
                }
                last.text.push_str(&comment.text);
                last.span.end_byte = comment.span.end_byte;
                last.span.end_position = comment.span.end_position;
                continue;
            }
        }

        merged.push(comment.clone());
    }

    merged
}

/// Check whether the gap between a comment end and an entity start contains
/// nothing but blank lines or attribute/decorator lines.
///
/// Allowed lines: blank; starting with `#[`/`[`/`@` (attribute/decorator
/// first line); starting or ending with `(`/`,` (attribute continuation);
/// ending with `)`/`]` (attribute closing line). Anything else means the
/// comment is not adjacent — it is left unassociated rather than guessed.
pub(crate) fn gap_is_adjacent(source: &str, comment_end: usize, entity_start: usize) -> bool {
    if entity_start < comment_end || entity_start > source.len() {
        return false;
    }
    let gap = &source[comment_end..entity_start];
    for line in gap.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[")
            || trimmed.starts_with('[')
            || trimmed.starts_with('@')
            || trimmed.starts_with('(')
            || trimmed.starts_with(',')
            || trimmed.ends_with('(')
            || trimmed.ends_with(',')
            || trimmed.ends_with(')')
            || trimmed.ends_with(']')
        {
            continue;
        }
        return false;
    }
    true
}

/// First entity whose span starts at or after the comment end, provided the
/// gap in between is blank/attribute-only.
pub(crate) fn forward_adjacent_entity(
    source: &str,
    comment: &Comment,
    entities: &[Entity],
) -> Option<usize> {
    entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.span.start_byte >= comment.span.end_byte)
        .min_by_key(|(_, entity)| entity.span.start_byte)
        .filter(|(_, entity)| {
            gap_is_adjacent(source, comment.span.end_byte, entity.span.start_byte)
        })
        .map(|(idx, _)| idx)
}

/// Smallest entity whose span fully contains the comment (innermost container).
pub(crate) fn smallest_containing_entity(comment: &Comment, entities: &[Entity]) -> Option<usize> {
    entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| {
            entity.span.start_byte <= comment.span.start_byte
                && entity.span.end_byte >= comment.span.end_byte
        })
        .min_by_key(|(_, entity)| entity.span.end_byte.saturating_sub(entity.span.start_byte))
        .map(|(idx, _)| idx)
}

/// Attach a cleaned doc comment to an entity slot (first-wins).
pub(crate) fn attach_doc_comment(entity: &mut Entity, comment: &Comment) {
    if entity.doc_comment.is_some() {
        return;
    }
    let cleaned = super::clean_doc_comment_impl(&comment.text, true);
    entity.doc_comment = Some(cleaned);
    // Store doc comment start line in metadata for row range calculation
    let doc_start_line = comment.span.start_position.row + 1; // 1-indexed
    entity.metadata.insert(
        "doc_comment_start_line".to_string(),
        doc_start_line.to_string(),
    );
}
