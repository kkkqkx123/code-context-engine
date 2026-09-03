use crate::types::DocumentClassification;
use cce_types::ChunkedResult;
use cce_types::Span;

use super::PlainTextChunker;

pub(crate) fn chunk_make(
    chunker: &PlainTextChunker,
    content: &str,
    file_path: &str,
    classification: &DocumentClassification,
) -> Vec<ChunkedResult> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    let base = file_path.replace(['/', '\\'], "_");
    let mut chunks = Vec::new();
    for (i, (start, end)) in make_target_ranges(content).into_iter().enumerate() {
        if let Some(span) = Span::from_byte_range(content, start, end) {
            chunks.extend(chunker.produce_unit(
                &content[start..end],
                span,
                &base,
                i,
                file_path,
                classification,
            ));
        }
    }
    if chunks.is_empty() {
        return super::text_chunker::chunk_text(chunker, content, file_path, classification);
    }
    chunks
}

fn make_target_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let line_ranges = super::non_empty_and_blank_line_ranges(content);
    let mut target_starts: Vec<usize> = Vec::new();
    for (idx, (start, end)) in line_ranges.iter().enumerate() {
        let text = &content[*start..*end];
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if text.starts_with('\t') || text.starts_with(' ') {
            continue;
        }
        if trimmed.contains(':') {
            if let Some(colon_pos) = trimmed.find(':') {
                let before_colon = &trimmed[..colon_pos];
                if before_colon.contains('=') {
                    continue;
                }
            }
            target_starts.push(idx);
        }
    }
    if target_starts.is_empty() {
        return Vec::new();
    }
    let first_target = target_starts[0];
    let preamble_start = 0;
    let preamble_end = if first_target > 0 {
        line_ranges[first_target - 1].1
    } else {
        0
    };
    let preamble_text = if preamble_end > preamble_start {
        content[preamble_start..preamble_end].trim()
    } else {
        ""
    };
    let mut all_starts = Vec::new();
    if !preamble_text.is_empty() {
        all_starts.push(preamble_start);
    }
    for &idx in &target_starts {
        all_starts.push(line_ranges[idx].0);
    }
    for (i, &start) in all_starts.iter().enumerate() {
        let end = if i + 1 < all_starts.len() {
            all_starts[i + 1]
        } else {
            content.len()
        };
        let actual_end = if i + 1 < all_starts.len() {
            let mut last_end = start;
            for (s, e) in &line_ranges {
                if *s >= start && *e < end {
                    last_end = *e;
                }
            }
            last_end
        } else {
            content.len()
        };
        let actual_end = actual_end.max(start);
        if actual_end > start {
            ranges.push((start, actual_end));
        }
    }
    ranges
}
