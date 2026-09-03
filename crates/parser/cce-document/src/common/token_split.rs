use std::ops::Range;

use cce_config::modules::ChunkingConfig;
use cce_types::ChunkPath;
use cce_utils::token_estimation::TokenEstimator;

#[derive(Debug, Clone)]
pub struct ChunkBoundary {
    pub start_byte: usize,
    pub end_byte: usize,
}

impl ChunkBoundary {
    pub fn new(start_byte: usize, end_byte: usize) -> Self {
        Self {
            start_byte,
            end_byte,
        }
    }
}

pub fn split_by_tokens(
    text: &str,
    path: ChunkPath,
    config: &ChunkingConfig,
    estimator: &TokenEstimator,
) -> Vec<ChunkBoundary> {
    split_by_tokens_in_range(text, 0..text.len(), path, config, estimator)
}

pub fn split_by_tokens_in_range(
    text: &str,
    range: Range<usize>,
    path: ChunkPath,
    config: &ChunkingConfig,
    estimator: &TokenEstimator,
) -> Vec<ChunkBoundary> {
    let mut boundaries = Vec::new();
    let mut current_pos = range.start;

    while current_pos < range.end {
        let target = match path {
            ChunkPath::Bm25 => {
                find_bm25_split_point(text, current_pos, config.max_bm25_words).min(range.end)
            }
            ChunkPath::Embedding => {
                let relative =
                    estimator.find_split_point(&text[current_pos..range.end], config.max_tokens);
                (current_pos + relative).min(range.end)
            }
        };
        let safe_end = find_good_split_point(text, current_pos, target).min(range.end);

        if safe_end <= current_pos {
            let next_char_end = text[current_pos..range.end]
                .chars()
                .next()
                .map(|ch| current_pos + ch.len_utf8())
                .unwrap_or(range.end);
            if next_char_end <= current_pos {
                break;
            }
            boundaries.push(ChunkBoundary::new(current_pos, next_char_end));
            current_pos = next_char_end;
            continue;
        }

        boundaries.push(ChunkBoundary::new(current_pos, safe_end));
        current_pos = safe_end;
    }

    boundaries
}

fn find_good_split_point(text: &str, start: usize, target: usize) -> usize {
    debug_assert!(text.is_char_boundary(start));

    let target = floor_char_boundary(text, target.clamp(start, text.len()));
    if target >= text.len() {
        return text.len();
    }

    let search_window = target.saturating_sub(start).max(50);
    let window_start = floor_char_boundary(text, target.saturating_sub(search_window));

    if let Some(fence_pos) = find_preceding_line_start(text, window_start, target, "```") {
        return fence_pos;
    }

    let preferred_start = window_start;
    let mut sentence_boundary = None;
    let mut whitespace_boundary = None;
    let mut newline_boundary = None;
    for (relative, ch) in text[start..target].char_indices() {
        let end = start + relative + ch.len_utf8();
        if ch.is_whitespace() {
            whitespace_boundary = Some(end);
        }
        if ch == '\n' && end >= preferred_start {
            newline_boundary = Some(end);
        }
        if end >= preferred_start && matches!(ch, '.' | '!' | '?' | '。' | '！' | '？') {
            sentence_boundary = Some(end);
        }
    }

    let candidate = sentence_boundary
        .or(newline_boundary)
        .or(whitespace_boundary)
        .unwrap_or(target);

    avoid_incomplete_expression(text, start, candidate)
}

fn find_preceding_line_start(text: &str, start: usize, end: usize, marker: &str) -> Option<usize> {
    let search_region = &text[start..end];
    let rel = search_region.rfind(marker)?;
    let abs = start + rel;
    let line_start = text[start..abs]
        .rfind('\n')
        .map(|rel_nl| start + rel_nl + 1)
        .unwrap_or(start);
    Some(floor_char_boundary(text, line_start))
}

fn avoid_incomplete_expression(text: &str, chunk_start: usize, split: usize) -> usize {
    if split <= chunk_start || split >= text.len() {
        return split;
    }

    let prefix = &text[chunk_start..split];
    let last_non_ws = prefix.trim_end().chars().next_back();

    let is_incomplete = match last_non_ws {
        Some('=' | '{' | '(' | '[' | ',' | '.' | ':') => true,
        Some('-') | Some('>') => {
            prefix.trim_end().ends_with("->") || prefix.trim_end().ends_with("=>")
        }
        _ => false,
    };

    if is_incomplete {
        if let Some(rel) = prefix.rfind("\n\n") {
            let candidate = chunk_start + rel + 2;
            if candidate > chunk_start {
                return candidate;
            }
        }
        if let Some(rel) = prefix.rfind('\n') {
            let candidate = chunk_start + rel + 1;
            if candidate > chunk_start {
                return candidate;
            }
        }
    }

    split
}

fn find_bm25_split_point(text: &str, start: usize, max_words: usize) -> usize {
    if max_words == 0 {
        return text.len();
    }

    let mut words = 0;
    let mut in_word = false;
    for (relative, ch) in text[start..].char_indices() {
        if ch.is_whitespace() {
            if in_word {
                words += 1;
                if words >= max_words {
                    return start + relative + ch.len_utf8();
                }
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }

    text.len()
}

fn floor_char_boundary(text: &str, mut position: usize) -> usize {
    while position > 0 && !text.is_char_boundary(position) {
        position -= 1;
    }
    position
}
