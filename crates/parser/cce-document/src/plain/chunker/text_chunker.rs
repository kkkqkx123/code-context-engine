use crate::types::DocumentClassification;
use cce_types::ChunkedResult;
use cce_types::Span;

use super::PlainTextChunker;

pub(crate) fn chunk_text(
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

    for (i, (start, end)) in super::paragraph_ranges(content).into_iter().enumerate() {
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
