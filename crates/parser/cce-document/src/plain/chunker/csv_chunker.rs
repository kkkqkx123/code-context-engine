use crate::types::DocumentClassification;
use cce_types::Span;
use cce_types::{ChunkPath, ChunkedResult};

use super::PlainTextChunker;

pub(crate) fn chunk_csv(
    chunker: &PlainTextChunker,
    content: &str,
    file_path: &str,
    classification: &DocumentClassification,
) -> Vec<ChunkedResult> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    let base = file_path.replace(['/', '\\'], "_");

    let header = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");

    if header.is_empty() {
        return Span::from_byte_range(content, 0, content.len())
            .map(|span| chunker.produce_unit(content, span, &base, 0, file_path, classification))
            .unwrap_or_default();
    }

    let mut first_data_idx = 0;
    for (idx, line) in lines.iter().enumerate() {
        if line.trim() == header {
            first_data_idx = idx + 1;
            break;
        }
    }

    let mut chunks = Vec::new();
    let mut unit_idx = 0;

    if chunker.config.exceeds_limit(header, ChunkPath::Embedding) {
        let header_line = lines.iter().position(|l| l.trim() == header).unwrap_or(0);
        if let Some(span) = Span::from_byte_range(
            content,
            super::line_start(content, header_line),
            super::line_end(content, header_line),
        ) {
            chunks.extend(chunker.produce_unit(
                header,
                span,
                &base,
                unit_idx,
                file_path,
                classification,
            ));
        }
        unit_idx += 1;
    }

    let mut batch: Vec<&str> = vec![header];
    let mut batch_start_line = first_data_idx
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1));
    let _header_tokens = chunker.estimator.estimate_text(header);

    for (line_idx, line) in lines.iter().enumerate().skip(first_data_idx) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if chunker.config.exceeds_limit(line, ChunkPath::Embedding) {
            if batch.len() > 1 {
                let batch_end = line_idx;
                if let Some(span) = Span::from_byte_range(
                    content,
                    super::line_start(content, batch_start_line),
                    super::line_end(content, batch_end),
                ) {
                    chunks.extend(chunker.produce_unit(
                        &batch.join("\n"),
                        span,
                        &base,
                        unit_idx,
                        file_path,
                        classification,
                    ));
                }
                unit_idx += 1;
            }
            batch = vec![header];
            batch_start_line = line_idx;

            if let Some(span) = Span::from_byte_range(
                content,
                super::line_start(content, line_idx),
                super::line_end(content, line_idx),
            ) {
                chunks.extend(chunker.produce_unit(
                    line,
                    span,
                    &base,
                    unit_idx,
                    file_path,
                    classification,
                ));
            }
            unit_idx += 1;
            continue;
        }

        let current_batch_text = if batch.len() == 1 {
            format!("{}\n{}", batch[0], line)
        } else {
            format!("{}\n{}", batch.join("\n"), line)
        };

        if chunker
            .config
            .exceeds_limit(&current_batch_text, ChunkPath::Embedding)
            && batch.len() > 1
        {
            let batch_end = line_idx - 1;
            if let Some(span) = Span::from_byte_range(
                content,
                super::line_start(content, batch_start_line),
                super::line_end(content, batch_end),
            ) {
                chunks.extend(chunker.produce_unit(
                    &batch.join("\n"),
                    span,
                    &base,
                    unit_idx,
                    file_path,
                    classification,
                ));
            }
            unit_idx += 1;
            batch = vec![header];
            batch_start_line = line_idx - 1;
        }
        batch.push(line);
    }

    if !batch.is_empty() && batch.len() > 1 {
        let batch_end = lines.len().saturating_sub(1);
        if let Some(span) = Span::from_byte_range(
            content,
            super::line_start(content, batch_start_line),
            super::line_end(content, batch_end),
        ) {
            chunks.extend(chunker.produce_unit(
                &batch.join("\n"),
                span,
                &base,
                unit_idx,
                file_path,
                classification,
            ));
        }
    }

    chunks
}
