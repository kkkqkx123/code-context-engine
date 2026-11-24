use cce_codegraph::IndexBuilder;
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::grouper::GroupType;
use cce_types::ParsedFile;

use crate::FixtureSpec;
use crate::bench_data::{CallEdgeData, ChunkData};
use crate::bench_gen::{
    FullPipelineParse, GeneratedChunks, bm25_doc_records, chunk_data_from_result,
    collect_call_edges, collect_full_pipeline_parse, fixture_source_path, validate_unique_chunks,
};

/// Derive full-pipeline (NL text) chunks from one shared parse.
///
/// The production index parses each file **once** and generates both paths
/// from the same parse (see `IndexOrchestrator::decide_output_mode`), so both
/// paths share the same `entity_ids` and `segment_id`s — the cross-path
/// alignment contract documented in `chunker.rs` / `hybrid-fusion-design.md`.
/// Parsing once per baseline would waste a full tree-sitter pass over large
/// fixtures, so `collect_full_pipeline_parse` runs the parse and both this
/// and the raw-source baseline derive from it without reparsing.
pub fn derive_full_pipeline_chunks(
    parse: &FullPipelineParse,
    target_spec: FixtureSpec,
) -> anyhow::Result<GeneratedChunks> {
    let mut emb_results = parse.embedding_nl.clone();
    let mut bm25_results = parse.bm25_nl.clone();

    exclude_file_documentation(&mut emb_results, "full-pipeline embedding");
    exclude_file_documentation(&mut bm25_results, "full-pipeline BM25");

    eprintln!(
        "=== FULL PIPELINE: {} embedding chunks, {} bm25 chunks ===",
        emb_results.len(),
        bm25_results.len(),
    );

    let embedding_chunks: Vec<ChunkData> = emb_results.iter().map(chunk_data_from_result).collect();
    let bm25_chunks: Vec<ChunkData> = bm25_results.iter().map(chunk_data_from_result).collect();
    let embedding_texts: Vec<String> = emb_results.iter().map(|cr| cr.text.clone()).collect();
    let bm25_texts: Vec<String> = bm25_results.iter().map(|cr| cr.text.clone()).collect();
    let bm25_documents = bm25_doc_records(&bm25_results);

    validate_unique_chunks(&embedding_chunks, "full_pipeline embedding");
    validate_unique_chunks(&bm25_chunks, "full_pipeline bm25");

    Ok(GeneratedChunks {
        embedding_chunks,
        embedding_texts,
        bm25_chunks,
        bm25_texts,
        bm25_documents,
        call_edges: build_call_edges(target_spec, &parse.target_parsed),
    })
}

pub async fn gen_full_pipeline_chunks(
    target_spec: FixtureSpec,
    distractor_spec: FixtureSpec,
) -> anyhow::Result<GeneratedChunks> {
    let parse = collect_full_pipeline_parse(target_spec.clone(), distractor_spec).await?;
    derive_full_pipeline_chunks(&parse, target_spec)
}

/// Build the relation index from the same parses that produced the chunks and
/// extract the in-project call-edge sidecar for offline annotation review.
fn build_call_edges(target_spec: FixtureSpec, parsed_files: &[ParsedFile]) -> Vec<CallEdgeData> {
    if parsed_files.is_empty() {
        return Vec::new();
    }
    let builder = IndexBuilder::new();
    let project_symbols = builder.create_project_symbol_table(fixture_source_path(target_spec));
    for pf in parsed_files {
        builder.add_file_symbols(pf, &project_symbols);
    }
    for pf in parsed_files {
        builder.register_file_entities(pf);
    }
    for pf in parsed_files {
        builder.resolve_file_relations(pf, &project_symbols);
    }
    let index = builder.build();
    let edges = collect_call_edges(&index);
    eprintln!(
        "=== FULL PIPELINE: {} in-project call edges over {} parsed files ===",
        edges.len(),
        parsed_files.len()
    );
    edges
}

pub fn exclude_file_documentation(chunks: &mut Vec<ChunkedResult>, label: &str) {
    let before = chunks.len();
    chunks.retain(|chunk| chunk.group_type != GroupType::FileDocumentation);
    let removed = before.saturating_sub(chunks.len());
    if removed > 0 {
        eprintln!("  Excluded {removed} file-documentation chunks from {label}");
    }
}
