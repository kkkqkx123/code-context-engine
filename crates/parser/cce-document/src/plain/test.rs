use crate::types::DocSummary;
use cce_config::modules::ChunkingConfig;
use cce_types::ast_to_nl::options::OutputMode;

use super::*;
use crate::types::DocumentClassification;

#[test]
fn test_pipeline_text() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let text = "Hello world\n\nThis is a test.";

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(text, "test.txt", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());
    let summary = summary.unwrap();
    assert_eq!(summary.line_count, 3);
}

#[test]
fn test_pipeline_log() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let log = "2024-01-01 INFO Starting\n2024-01-01 DEBUG Running";

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(log, "app.log", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    let summary = summary.unwrap();
    assert_eq!(summary.doc_type, DocType::PlainText);
}

#[test]
fn test_pipeline_ini() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let ini = "[section]\nkey=value";

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(ini, "config.ini", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    let summary = summary.unwrap();
    assert_eq!(summary.doc_type, DocType::Config);
}

#[test]
fn test_pipeline_csv() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let csv = "name,age,city\nAlice,30,NYC\nBob,25,SF";

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(csv, "data.csv", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    let summary = summary.unwrap();
    assert!(summary.line_count >= 3);
}

#[test]
fn test_pipeline_empty() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process("", "empty.txt", &config, OutputMode::default())
        .expect("should process");

    assert!(chunks.is_empty());
    assert!(summary.is_some());
}

#[test]
fn test_pipeline_large_file() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();

    let text = (0..100)
        .map(|i| format!("Line {}: This is a test line with some content.", i))
        .collect::<Vec<_>>()
        .join("\n");

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(&text, "large.txt", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());
}

#[test]
fn test_pipeline_with_both_modes() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let text = "Test content for both modes.";

    let (chunks_bm25, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(text, "test.txt", &config, OutputMode::Bm25)
        .expect("should process with BM25");
    assert!(!chunks_bm25.is_empty());

    let (chunks_emb, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(text, "test.txt", &config, OutputMode::Embedding)
        .expect("should process with Embedding");
    assert!(!chunks_emb.is_empty());

    let (chunks_both, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(text, "test.txt", &config, OutputMode::Both)
        .expect("should process with Both");
    assert!(!chunks_both.is_empty());
}

#[test]
fn test_plaintext_kind_detection() {
    assert_eq!(PlainTextKind::from_extension("txt"), PlainTextKind::Text);
    assert_eq!(PlainTextKind::from_extension("log"), PlainTextKind::Log);
    assert_eq!(PlainTextKind::from_extension("ini"), PlainTextKind::Ini);
    assert_eq!(PlainTextKind::from_extension("csv"), PlainTextKind::Csv);
    assert_eq!(
        PlainTextKind::from_extension("unknown"),
        PlainTextKind::Text
    );
}

#[test]
fn test_pipeline_multi_paragraph() {
    let pipeline = PlainTextPipeline::new();
    let config = ChunkingConfig::default();
    let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(text, "test.txt", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    let summary = summary.unwrap();
    assert!(summary.line_count > 0);
}

#[test]
fn test_chunker_direct() {
    let config = ChunkingConfig::default();
    let chunker = PlainTextChunker::new(config);
    let content = "Line 1\nLine 2\nLine 3";
    let results = chunker.chunk(
        content,
        "test.txt",
        PlainTextKind::Text,
        &DocumentClassification::detect("test.txt"),
    );
    assert!(!results.is_empty());
}
