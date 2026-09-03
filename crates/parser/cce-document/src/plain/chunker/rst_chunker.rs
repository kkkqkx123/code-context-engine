use crate::types::DocumentClassification;
use cce_types::ChunkedResult;
use cce_types::Span;

use super::PlainTextChunker;

pub(crate) fn chunk_rst(
    chunker: &PlainTextChunker,
    content: &str,
    file_path: &str,
    classification: &DocumentClassification,
) -> Vec<ChunkedResult> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    let base = file_path.replace(['/', '\\'], "_");
    let paras = super::paragraph_ranges(content);

    let heading_indices: Vec<usize> = paras
        .iter()
        .enumerate()
        .filter_map(|(i, &(start, end))| {
            if is_rst_heading_paragraph(&content[start..end]) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if heading_indices.is_empty() {
        return super::text_chunker::chunk_text(chunker, content, file_path, classification);
    }

    let mut chunks = Vec::new();
    let mut unit_idx = 0;

    let first_heading = heading_indices[0];
    if first_heading > 0 {
        let (start, end) = super::merge_paragraph_ranges(&paras, 0, first_heading);
        if let Some(span) = Span::from_byte_range(content, start, end) {
            chunks.extend(chunker.produce_unit(
                &content[start..end],
                span,
                &base,
                unit_idx,
                file_path,
                classification,
            ));
            unit_idx += 1;
        }
    }

    for (si, &hi) in heading_indices.iter().enumerate() {
        let next = heading_indices.get(si + 1).copied().unwrap_or(paras.len());
        let (start, end) = super::merge_paragraph_ranges(&paras, hi, next);
        if let Some(span) = Span::from_byte_range(content, start, end) {
            chunks.extend(chunker.produce_unit(
                &content[start..end],
                span,
                &base,
                unit_idx,
                file_path,
                classification,
            ));
            unit_idx += 1;
        }
    }

    chunks
}

pub(crate) fn is_rst_underline(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    if first != b'=' && first != b'-' && first != b'~' && first != b'^' && first != b'"' {
        return false;
    }
    trimmed.bytes().all(|b| b == first)
}

pub(crate) fn is_rst_heading_paragraph(text: &str) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() < 2 {
        return false;
    }
    let last = lines.last().unwrap().trim();
    is_rst_underline(last) && !lines[..lines.len() - 1].iter().all(|l| l.trim().is_empty())
}
