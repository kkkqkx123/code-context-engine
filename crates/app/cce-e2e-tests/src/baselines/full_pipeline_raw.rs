use std::collections::HashMap;

use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::grouper::GroupType;

use crate::bench_data::ChunkData;
use crate::bench_gen::{
    FullPipelineParse, GeneratedChunks, bm25_doc_records, chunk_data_from_result,
    collect_full_pipeline_parse, normalize_file_path, validate_unique_chunks,
};

/// Derive raw-source chunks from one shared parse.
///
/// Shares `collect_full_pipeline_parse` with the NL `full_pipeline` baseline
/// so large fixtures are parsed once; derivation only clones the NL chunks
/// and replaces `text` by slicing the shared decoded source map by
/// `source_ranges` (see `baselines/full_pipeline.rs` for the single-parse
/// alignment rationale).
pub fn derive_full_pipeline_raw_source_chunks(
    parse: &FullPipelineParse,
) -> anyhow::Result<GeneratedChunks> {
    // Chunks whose source coverage resolves to no (non-blank) source lines
    // cannot contribute a raw-source baseline entry; drop them rather than
    // substituting NL text or panicking. This keeps the raw-source baseline
    // aligned with what the production chunker actually owns.
    let emb_results: Vec<ChunkedResult> = parse
        .embedding_nl
        .iter()
        .filter(|chunk| chunk.group_type != GroupType::FileDocumentation)
        .filter_map(|chunk| {
            let mut chunk = chunk.clone();
            let raw = try_extract_raw_source(&chunk, &parse.source_map)?;
            chunk.text = raw;
            Some(chunk)
        })
        .collect();
    let bm25_results: Vec<ChunkedResult> = parse
        .bm25_nl
        .iter()
        .filter(|chunk| chunk.group_type != GroupType::FileDocumentation)
        .filter_map(|chunk| {
            let mut chunk = chunk.clone();
            let raw = try_extract_raw_source(&chunk, &parse.source_map)?;
            chunk.text = raw;
            Some(chunk)
        })
        .collect();

    eprintln!(
        "=== FULL PIPELINE RAW SOURCE: {} embedding chunks, {} bm25 chunks ===",
        emb_results.len(),
        bm25_results.len(),
    );

    let embedding_chunks: Vec<ChunkData> = emb_results.iter().map(chunk_data_from_result).collect();
    let bm25_chunks: Vec<ChunkData> = bm25_results.iter().map(chunk_data_from_result).collect();
    let embedding_texts: Vec<String> = emb_results.iter().map(|cr| cr.text.clone()).collect();
    let bm25_texts: Vec<String> = bm25_results.iter().map(|cr| cr.text.clone()).collect();
    let bm25_documents = bm25_doc_records(&bm25_results);

    validate_unique_chunks(&embedding_chunks, "full_pipeline_raw_source embedding");
    validate_unique_chunks(&bm25_chunks, "full_pipeline_raw_source bm25");

    Ok(GeneratedChunks {
        embedding_chunks,
        embedding_texts,
        bm25_chunks,
        bm25_texts,
        bm25_documents,
        call_edges: Vec::new(),
    })
}

pub async fn gen_full_pipeline_raw_source_chunks(
    target_spec: crate::FixtureSpec,
    distractor_spec: crate::FixtureSpec,
) -> anyhow::Result<GeneratedChunks> {
    let parse = collect_full_pipeline_parse(target_spec, distractor_spec).await?;
    derive_full_pipeline_raw_source_chunks(&parse)
}

fn try_extract_raw_source(
    chunk: &ChunkedResult,
    source_map: &HashMap<String, String>,
) -> Option<String> {
    const MAX_TOKENS: usize = 7200;
    // Proportional truncation is shared with the dead-letter truncate-retry
    // path via `cce_utils::token_estimation::truncate_to_token_budget`:
    // when the estimate exceeds the limit, cut to 80% of the current length
    // (at a line boundary) and re-check, with an absolute split as the final
    // fallback. The estimator can underestimate code density, so the
    // conservative ratio keeps the result under the provider cap.

    let file_path = normalize_file_path(&chunk.metadata.file_path);
    let source = source_map.get(&file_path).unwrap_or_else(|| {
        panic!(
            "raw-source extraction: file not in source map: {} (chunk_id={})",
            file_path, chunk.chunk_id
        )
    });
    let lines: Vec<&str> = source.lines().collect();
    // GroupFallback chunks carry no *exact* entity ranges, but the merge
    // pass still records the concrete source spans it combined (see
    // `merge_source_coverage`). Raw-source extraction needs bytes, not
    // precision, so those recorded spans are used directly here; only when
    // nothing was recorded do we fall back to the enclosing span.
    let ranges: &[cce_types::Span] = if !chunk.metadata.source_ranges.is_empty() {
        &chunk.metadata.source_ranges
    } else {
        std::slice::from_ref(&chunk.metadata.source_span)
    };
    let mut parts: Vec<String> = Vec::new();
    for span in ranges {
        let (start, end) = match span.line_range_opt() {
            Some(r) => r,
            None => continue,
        };
        if start == 0 || end < start || start > lines.len() {
            continue;
        }
        let end = end.min(lines.len());
        parts.push(lines[start - 1..end].join("\n"));
    }
    let raw = parts.join("\n");

    if raw.trim().is_empty() {
        return None;
    }

    let result = cce_utils::token_estimation::truncate_to_token_budget(&raw, MAX_TOKENS);
    if result.truncated {
        eprintln!(
            "  raw-source: chunk {} truncated from {} to {} bytes (estimate {} > {} tokens)",
            chunk.chunk_id,
            result.original_len,
            result.final_len,
            result.original_estimate,
            MAX_TOKENS
        );
    }
    Some(result.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_gen::split_full_pipeline_chunks;
    use cce_orchestrator::index::FileProcessor;
    use cce_types::OutputMode;

    fn gbk_c_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"#include <stdio.h>\n");
        bytes.extend_from_slice(
            b"// a \xa1\xae1\xa1\xaf bit indicates \xa1\xaenot in this track\xa1\xaf\n",
        );
        bytes.extend_from_slice(b"int add(int a, int b) {\n    return a + b;\n}\n");
        bytes
    }

    #[tokio::test]
    async fn raw_source_map_covers_non_utf8_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("mov-chnl.c");
        let raw = gbk_c_bytes();
        std::fs::write(&path, &raw).expect("write gbk file");

        assert!(
            std::fs::read_to_string(&path).is_err(),
            "fixture must reproduce the non-UTF8 case plain read_to_string skips"
        );
        let decoded = cce_utils::file::read_file_to_utf8(&path).expect("production decoding");
        assert!(decoded.contains("int add"));

        let metadata = std::fs::metadata(&path).expect("stat file");
        let entry = cce_scanner::FileEntry::new(
            path.clone(),
            std::path::PathBuf::from("libmov/source/mov-chnl.c"),
            metadata.len(),
            metadata.modified().expect("mtime").into(),
        )
        .with_hash(cce_utils::hash::calculate_hash(&raw))
        .with_language_info(cce_types::LanguageInfo::detect_from_path(
            "libmov/source/mov-chnl.c",
        ));

        let mut processor = FileProcessor::new();
        let result = processor
            .process_file_complete(&entry, OutputMode::Both)
            .await
            .expect("non-UTF8 C file must process via production decoding");

        let mut source_map: HashMap<String, String> = HashMap::new();
        let decoded_from_result = result.parsed_file.source.to_string();
        assert_eq!(
            decoded_from_result, decoded,
            "map content must equal the indexed source so line numbers align"
        );
        source_map
            .entry(normalize_file_path(&entry.relative_path.to_string_lossy()))
            .or_insert_with(|| decoded_from_result.clone());
        source_map
            .entry(normalize_file_path(&result.parsed_file.path))
            .or_insert(decoded_from_result);

        let (emb, bm25) = split_full_pipeline_chunks(result.chunks);
        assert!(
            !emb.is_empty() || !bm25.is_empty(),
            "gbk fixture must yield chunks"
        );
        for chunk in emb
            .iter()
            .chain(bm25.iter())
            .filter(|chunk| chunk.group_type != cce_parser::grouper::GroupType::FileDocumentation)
        {
            let raw_text = try_extract_raw_source(chunk, &source_map)
                .expect("raw-source extraction: chunk has no source coverage");
            assert!(
                !raw_text.trim().is_empty(),
                "chunk {} must extract raw source",
                chunk.chunk_id
            );
        }
    }
}
