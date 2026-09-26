//! License header block detection for file-leading comments.
//!
//! Matching is literal prefix/suffix based (see [`LicenseHeaderConfig`]):
//! a block starts at a comment line whose marker-stripped form begins with a
//! rule prefix, and ends either at the first line ending with the rule suffix
//! or at the end of the contiguous comment run when the rule has no suffix.
//! A candidate whose suffix never closes inside the run is kept entirely, so
//! a mis-tuned rule degrades to "no filtering" instead of deleting content.

use super::Comment;
use cce_config::LicenseHeaderConfig;
use cce_types::{Entity, Span};

/// Comment markers stripped from line ends/starts before literal matching.
fn is_marker_char(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '/'
            | '*'
            | '#'
            | '-'
            | '!'
            | ';'
            | '<'
            | '>'
            | '.'
            | ','
            | ':'
            | '%'
            | '@'
            | '='
    )
}

fn normalize(line: &str) -> String {
    line.trim_start_matches(is_marker_char)
        .trim_end_matches(is_marker_char)
        .to_lowercase()
}

fn normalized_lines(comment: &Comment) -> Vec<String> {
    comment
        .text
        .lines()
        .map(normalize)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Returns the source spans of license blocks in the file header.
///
/// Only comments located before the first entity participate; at most
/// `config.max_comments` of them are considered.
pub(crate) fn find_license_blocks(
    comments: &[Comment],
    entities: &[Entity],
    config: &LicenseHeaderConfig,
) -> Vec<Span> {
    if !config.enabled || config.rules.is_empty() {
        return Vec::new();
    }

    let first_entity_start = entities.iter().map(|e| e.span.start_byte).min();
    let header: Vec<&Comment> = comments
        .iter()
        .take_while(|c| first_entity_start.is_none_or(|start| c.span.start_byte < start))
        .take(config.max_comments)
        .collect();

    let mut blocks = Vec::new();
    let mut run_start = 0;
    while run_start < header.len() {
        let mut run_end = run_start + 1;
        while run_end < header.len() && contiguous(header[run_end - 1], header[run_end]) {
            run_end += 1;
        }
        consume_run(&header[run_start..run_end], config, &mut blocks);
        run_start = run_end;
    }
    blocks
}

/// A run is contiguous when the next comment starts at most one blank line
/// after the previous one ends (row-based, immune to line endings).
fn contiguous(prev: &Comment, next: &Comment) -> bool {
    next.span.start_position.row <= prev.span.end_position.row + 2
}

/// Match rules inside one contiguous comment run, appending accepted block
/// spans; scanning resumes after each accepted block so a run can contain
/// several license blocks.
fn consume_run(run: &[&Comment], config: &LicenseHeaderConfig, blocks: &mut Vec<Span>) {
    let mut offset = 0;
    while offset < run.len() {
        let Some((start, end)) = match_block(&run[offset..], config) else {
            return;
        };
        blocks.push(Span {
            start_byte: run[offset + start].span.start_byte,
            end_byte: run[offset + end].span.end_byte,
            start_position: run[offset + start].span.start_position,
            end_position: run[offset + end].span.end_position,
        });
        offset += end + 1;
    }
}

/// Try each rule in order against the run; return the (first, last) indices
/// of the accepted block. Rules without a suffix take the whole remainder of
/// the run; rules with a suffix must find a closing line, otherwise the next
/// rule is tried.
fn match_block(run: &[&Comment], config: &LicenseHeaderConfig) -> Option<(usize, usize)> {
    let lines: Vec<Vec<String>> = run.iter().map(|c| normalized_lines(c)).collect();

    for rule in &config.rules {
        let prefix = rule.prefix.to_lowercase();
        let start = match lines.iter().enumerate().find_map(|(ci, comment_lines)| {
            comment_lines
                .iter()
                .any(|line| line.starts_with(&prefix))
                .then_some(ci)
        }) {
            Some(start) => start,
            None => continue,
        };

        let Some(suffix) = &rule.suffix else {
            return Some((start, run.len() - 1));
        };
        let suffix = suffix.to_lowercase();
        for (ci, comment_lines) in lines.iter().enumerate().skip(start) {
            if comment_lines.iter().any(|line| line.ends_with(&suffix)) {
                return Some((start, ci));
            }
        }
    }
    None
}

/// Whether a comment or merged block belongs to a license block.
pub(crate) fn is_license_span(span: &Span, blocks: &[Span]) -> bool {
    blocks
        .iter()
        .any(|block| span.start_byte < block.end_byte && span.end_byte > block.start_byte)
}
