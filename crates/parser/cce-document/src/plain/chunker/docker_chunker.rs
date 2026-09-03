use crate::types::DocumentClassification;
use cce_types::ChunkedResult;
use cce_types::Span;

use super::PlainTextChunker;

pub(crate) fn chunk_docker(
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
    for (i, (start, end)) in docker_instruction_ranges(content).into_iter().enumerate() {
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

fn docker_instruction_ranges(content: &str) -> Vec<(usize, usize)> {
    const INSTRUCTIONS: &[&str] = &[
        "FROM",
        "RUN",
        "CMD",
        "LABEL",
        "MAINTAINER",
        "EXPOSE",
        "ENV",
        "ADD",
        "COPY",
        "ENTRYPOINT",
        "VOLUME",
        "USER",
        "WORKDIR",
        "ARG",
        "ONBUILD",
        "STOPSIGNAL",
        "HEALTHCHECK",
        "SHELL",
    ];
    let lines: Vec<(usize, usize, &str)> = {
        let mut offset = 0;
        let mut v = Vec::new();
        for line in content.split_inclusive('\n') {
            let end = offset + line.len();
            let text_end = offset + line.trim_end_matches(['\r', '\n']).len();
            v.push((offset, text_end, &content[offset..text_end]));
            offset = end;
        }
        v
    };
    let mut instr_indices = Vec::new();
    for (idx, (_, _, text)) in lines.iter().enumerate() {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let upper = trimmed.to_ascii_uppercase();
        let first_word = upper.split_whitespace().next().unwrap_or("");
        if INSTRUCTIONS.contains(&first_word) {
            instr_indices.push(idx);
        }
    }
    if instr_indices.is_empty() {
        return Vec::new();
    }
    let first = instr_indices[0];
    let mut all_ranges: Vec<(usize, usize)> = Vec::new();
    let expand_end = |start_idx: usize| -> (usize, usize) {
        let mut end_idx = start_idx;
        let mut idx = start_idx;
        while idx < lines.len() {
            let (_, _, text) = lines[idx];
            let trimmed = text.trim_end();
            if trimmed.ends_with('\\') && idx + 1 < lines.len() {
                end_idx = idx + 1;
                idx += 1;
            } else {
                end_idx = idx;
                break;
            }
        }
        let start_byte = lines[start_idx].0;
        let end_byte = lines[end_idx].1;
        (start_byte, end_byte)
    };
    let mut preamble_has_content = false;
    for (_, _, text) in lines.iter().take(first) {
        let trimmed = text.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            preamble_has_content = true;
            break;
        }
    }
    for &idx in &instr_indices {
        let (s, e) = expand_end(idx);
        all_ranges.push((s, e));
    }
    let mut merged: Vec<(usize, usize)> = all_ranges;
    if preamble_has_content && !merged.is_empty() && merged[0].0 > 0 {
        merged[0].0 = 0;
    }
    merged
}
