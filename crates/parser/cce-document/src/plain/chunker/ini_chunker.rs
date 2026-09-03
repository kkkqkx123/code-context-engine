use crate::types::DocumentClassification;
use cce_types::ChunkedResult;
use cce_types::Span;

use super::PlainTextChunker;

pub(crate) fn chunk_ini(
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

    for (i, (start, end)) in ini_section_ranges(content).into_iter().enumerate() {
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

    chunks
}

fn ini_section_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut section_start: Option<usize> = None;
    let mut section_end = 0;

    for (start, end) in super::non_empty_and_blank_line_ranges(content) {
        let text = &content[start..end];
        let is_section_header = text.trim().starts_with('[') && text.trim().ends_with(']');
        if is_section_header && section_start.is_some() {
            ranges.push((section_start.unwrap(), section_end));
            section_start = Some(start);
        } else if section_start.is_none() {
            section_start = Some(start);
        }
        section_end = end;
    }
    if let Some(start) = section_start {
        ranges.push((start, section_end));
    }
    ranges
}
