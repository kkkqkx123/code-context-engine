use anyhow::Context;

use crate::bench_data::ChunkData;
use crate::bench_gen::{
    GeneratedChunks, bm25_doc_records, chunk_data_from_result, load_fixture_files,
    validate_unique_chunks,
};
use crate::direct_chunker::{EntityChunker, get_chunk_text};

pub async fn gen_entity_based_chunks(
    target_spec: crate::FixtureSpec,
    distractor_spec: crate::FixtureSpec,
) -> anyhow::Result<GeneratedChunks> {
    let target_files =
        load_fixture_files(target_spec).context("Failed to load target source files")?;
    let dist_files =
        load_fixture_files(distractor_spec).context("Failed to load distractor source files")?;

    let chunker = EntityChunker::new();

    let target_chunks = chunker.chunk_files(&target_files);
    let dist_chunks = chunker.chunk_files(&dist_files);

    let mut embedding_chunks_raw = Vec::new();
    let mut bm25_chunks_raw = Vec::new();

    for cr in target_chunks.iter().chain(dist_chunks.iter()) {
        if cr.text.is_empty() {
            continue;
        }
        match cr.path {
            cce_parser::ast_to_nl::chunker::ChunkPath::Embedding => embedding_chunks_raw.push(cr),
            cce_parser::ast_to_nl::chunker::ChunkPath::Bm25 => bm25_chunks_raw.push(cr),
        }
    }

    eprintln!(
        "  Embedding chunks: {}, BM25 chunks: {}",
        embedding_chunks_raw.len(),
        bm25_chunks_raw.len()
    );

    let embedding_chunks: Vec<ChunkData> = embedding_chunks_raw
        .iter()
        .map(|chunk| chunk_data_from_result(chunk))
        .collect();
    let bm25_chunks: Vec<ChunkData> = bm25_chunks_raw
        .iter()
        .map(|chunk| chunk_data_from_result(chunk))
        .collect();
    let embedding_texts: Vec<String> = embedding_chunks_raw
        .iter()
        .map(|cr| get_chunk_text(cr).to_string())
        .collect();
    let bm25_texts: Vec<String> = bm25_chunks_raw.iter().map(|cr| cr.text.clone()).collect();
    let owned_bm25: Vec<cce_parser::ast_to_nl::chunker::ChunkedResult> =
        bm25_chunks_raw.iter().map(|cr| (*cr).clone()).collect();
    let bm25_documents = bm25_doc_records(&owned_bm25);
    validate_unique_chunks(&embedding_chunks, "entity_based embedding");
    validate_unique_chunks(&bm25_chunks, "entity_based bm25");

    Ok(GeneratedChunks {
        embedding_chunks,
        embedding_texts,
        bm25_chunks,
        bm25_texts,
        bm25_documents,
        call_edges: Vec::new(),
    })
}
