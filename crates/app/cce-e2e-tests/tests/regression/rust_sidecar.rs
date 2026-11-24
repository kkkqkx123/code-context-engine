//! Assertion test for Rust NL text display export with sidecar data.
//!
//! The `index_sidecar` fixture exercises the internal sidecar paths we care
//! about: `behavior` and `control_flow`, while keeping stdlib classification
//! as metadata.

use cce_orchestrator::index::FileProcessor;
use cce_parser::ast_to_nl::chunker::ChunkPath;
use cce_scanner::{FSScanner, ScanOptions};
use cce_types::OutputMode;

use crate::helper::{NlTextDocumentExporter, TestFixture, init_minimal_logging};

/// Verify plain text display output for the index_sidecar fixture.
///
/// This fixture exercises the internal sidecar paths we care about:
/// `behavior` and `control_flow`, while keeping stdlib classification as metadata.
#[tokio::test]
async fn test_nl_text_export_index_sidecar() {
    init_minimal_logging();

    let fixture = TestFixture::rust_index_sidecar().expect("Failed to load index_sidecar fixture");

    let project_root = fixture.root_path().to_path_buf();

    let mut scanner = FSScanner::new();
    let scan_opts = ScanOptions {
        root_path: project_root.to_string_lossy().to_string(),
        include_patterns: vec!["*.rs".to_string()],
        ..Default::default()
    };
    let file_entries = scanner
        .scan(&scan_opts)
        .expect("Failed to scan index_sidecar fixture");

    assert_eq!(
        file_entries.len(),
        1,
        "Fixture should contain one Rust file"
    );

    let entry = &file_entries[0];
    let mut file_processor = FileProcessor::new();
    let emb_result = file_processor
        .process_file_complete(entry, OutputMode::Embedding)
        .await
        .expect("Embedding processing should succeed");
    let bm25_result = file_processor
        .process_file_complete(entry, OutputMode::Bm25)
        .await
        .expect("BM25 processing should succeed");

    let parsed = emb_result.parsed_file;
    let processing_result = emb_result
        .processing_result
        .expect("Processing result should be present");

    assert!(
        !parsed.behavior.is_empty(),
        "Parsed file should carry behavior sidecar data"
    );
    assert!(
        !parsed.control_flow.is_empty(),
        "Parsed file should carry control-flow sidecar data"
    );
    assert!(
        !processing_result.behavior.is_empty(),
        "Processing result should carry behavior sidecar data"
    );
    assert!(
        !processing_result.control_flow.is_empty(),
        "Processing result should carry control-flow sidecar data"
    );

    let demo_entity = parsed
        .entities
        .iter()
        .find(|entity| entity.name == "process_values" && entity.kind.is_function_like())
        .expect("process_values entity should be extracted");
    assert!(
        parsed.behavior.get(demo_entity.id).is_some(),
        "process_values entity should have behavior facts"
    );
    assert!(
        parsed.control_flow.get(demo_entity.id).is_some(),
        "process_values entity should have control-flow facts"
    );

    assert!(
        !emb_result.chunks.is_empty(),
        "Embedding conversion should produce chunks"
    );
    assert!(
        !bm25_result.chunks.is_empty(),
        "BM25 conversion should produce chunks"
    );

    let emb_exporter = NlTextDocumentExporter::embedding_only(project_root.clone());
    let bm25_exporter = NlTextDocumentExporter::bm25_only(project_root.clone());

    let emb_output = emb_exporter
        .export_file_with_path(&emb_result.chunks, ChunkPath::Embedding, None)
        .expect("Embedding export should succeed");
    let bm25_output = bm25_exporter
        .export_file_with_path(&bm25_result.chunks, ChunkPath::Bm25, None)
        .expect("BM25 export should succeed");

    let emb_content =
        std::fs::read_to_string(&emb_output).expect("Should read Embedding display export");
    let bm25_content =
        std::fs::read_to_string(&bm25_output).expect("Should read BM25 display export");

    let emb_lower = emb_content.to_lowercase();
    assert!(
        emb_lower.contains("if let some(items) = values {"),
        "Control-flow source fragment should be present in embedding display text: {}",
        emb_content
    );
    assert!(
        emb_lower.contains("let mut buffer") || emb_lower.contains("buffer.push"),
        "Behavior facts should be present in embedding display text: {}",
        emb_content
    );
    assert!(
        emb_lower.contains("} else {"),
        "Control-flow should preserve else branch in embedding display text: {}",
        emb_content
    );
    assert!(
        emb_lower.contains("let mut buffer"),
        "Behavior facts should include raw source fragments: {}",
        emb_content
    );
    assert!(
        !emb_content.contains("keywords:"),
        "Embedding display should not show BM25 keywords: {}",
        emb_content
    );
    assert!(
        !emb_content.contains("nl:"),
        "Embedding display should not add an NL label: {}",
        emb_content
    );
    assert!(
        !emb_content.contains("code:"),
        "Embedding display should not include code labels: {}",
        emb_content
    );
    assert!(
        !emb_content.to_lowercase().contains("standard library"),
        "Embedding display should not add stdlib semantic expansion: {}",
        emb_content
    );

    let bm25_lower = bm25_content.to_lowercase();
    assert!(
        bm25_lower.contains("if let some(items) = values"),
        "Control-flow source fragment should be present in BM25 display text: {}",
        bm25_content
    );
    assert!(
        bm25_lower.contains("let mut buffer") || bm25_lower.contains("buffer.push"),
        "Behavior facts should be present in BM25 display text: {}",
        bm25_content
    );
    assert!(
        bm25_content.contains("keywords:"),
        "BM25 display should keep keywords: {}",
        bm25_content
    );
    assert!(
        bm25_content.contains("code:"),
        "BM25 display should keep code labels: {}",
        bm25_content
    );
    assert!(
        !bm25_content.contains("nl:"),
        "BM25-only display should not add an NL label: {}",
        bm25_content
    );
}
