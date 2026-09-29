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

/// Whether a comment line can serve as a Go documentation line.
///
/// Go has no `///` marker; godoc treats `//` comment blocks immediately
/// preceding a declaration as its documentation. Compiler directives
/// (`//go:...`) are never documentation.
pub(crate) fn is_go_doc_line(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("//")
        && !trimmed.starts_with("///")
        && !trimmed.starts_with("//!")
        && !trimmed.starts_with("//go:")
}

/// Merge consecutive Go documentation lines into runs.
///
/// Rows must be adjacent (same rule as plain-comment merging); anything else
/// (`//go:` directives, block comments, gaps) ends the current run.
/// Returns each run with the indices of its member comments so the caller
/// can withhold consumed comments from other channels.
pub(crate) fn merge_go_doc_runs(comments: &[Comment]) -> Vec<(Vec<usize>, Comment)> {
    let mut runs = Vec::new();
    let mut indices: Vec<usize> = Vec::new();
    let mut text = String::new();
    let mut last_row: Option<usize> = None;

    for (idx, comment) in comments.iter().enumerate() {
        if !is_go_doc_line(&comment.text) {
            flush_go_doc_run(&mut indices, &mut text, comments, &mut runs);
            last_row = None;
            continue;
        }
        let consecutive = last_row.is_some_and(|row| {
            comment.span.start_position.row == row || comment.span.start_position.row == row + 1
        });
        if !consecutive {
            flush_go_doc_run(&mut indices, &mut text, comments, &mut runs);
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&comment.text);
        last_row = Some(comment.span.end_position.row);
        indices.push(idx);
    }
    flush_go_doc_run(&mut indices, &mut text, comments, &mut runs);
    runs
}

fn flush_go_doc_run(
    indices: &mut Vec<usize>,
    text: &mut String,
    comments: &[Comment],
    runs: &mut Vec<(Vec<usize>, Comment)>,
) {
    if indices.is_empty() {
        return;
    }
    let first = &comments[indices[0]];
    let last = &comments[indices[indices.len() - 1]];
    let mut span = first.span;
    span.end_byte = last.span.end_byte;
    span.end_position = last.span.end_position;
    runs.push((
        std::mem::take(indices),
        Comment {
            text: std::mem::take(text),
            span,
            capture_name: "comment".to_string(),
        },
    ));
}

/// Attach Go `//` documentation runs to entities following the godoc
/// convention: a run documents the declaration that starts on the very
/// next row (no blank line in between).
///
/// Returns the indices of comments consumed as documentation so the caller
/// can withhold them from the plain-comment behavior channel.
pub(crate) fn attach_go_doc_comments(comments: &[Comment], entities: &mut [Entity]) -> Vec<usize> {
    let mut consumed = Vec::new();
    for (indices, run) in merge_go_doc_runs(comments) {
        let run_end_row = run.span.end_position.row;
        let target = entities
            .iter()
            .enumerate()
            .filter(|(_, entity)| entity.span.start_byte >= run.span.end_byte)
            .min_by_key(|(_, entity)| entity.span.start_byte)
            .filter(|(_, entity)| entity.span.start_position.row == run_end_row + 1)
            .map(|(idx, _)| idx);
        if let Some(idx) = target {
            attach_doc_comment(&mut entities[idx], &run);
            consumed.extend(indices);
        }
    }
    consumed
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
