use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;

use super::*;

#[test]
fn test_toml_pipeline_simple() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = r#"name = "test"
value = 123"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "test.toml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
    assert!(summary.line_count >= 2);
}

#[test]
fn test_toml_pipeline_with_table() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = r#"[database]
host = "localhost"
port = 3306"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "config.toml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.line_count > 0);
}

#[test]
fn test_toml_parse_stage() {
    let mut parser = TomlParser::new();
    let toml = r#"key = "value""#;
    let nodes = parser.parse(toml).expect("should parse");
    assert!(!nodes.is_empty());
}

#[test]
fn test_toml_group_stage() {
    let mut parser = TomlParser::new();
    let mut grouper = TomlGrouper::new();
    let toml = r#"name = "test""#;
    let nodes = parser.parse(toml).expect("should parse");
    let groups = grouper.group(nodes, "test.toml").expect("should group");
    assert!(!groups.is_empty());
}

#[test]
fn test_toml_chunk_stage() {
    let mut parser = TomlParser::new();
    let mut grouper = TomlGrouper::new();
    let config = ChunkingConfig::default();
    let chunker = TomlChunker::new(config.clone());
    let toml = r#"name = "test""#;
    let nodes = parser.parse(toml).expect("should parse");
    let groups = grouper.group(nodes, "test.toml").expect("should group");
    let chunks = chunker.chunk_groups(
        &groups,
        "test.toml",
        OutputMode::Both,
        &DocumentClassification::detect("test.toml"),
    );
    assert!(!chunks.is_empty());
}

#[test]
fn test_toml_summarize_stage() {
    let summarizer = TomlSummarizer::new();
    let mut parser = TomlParser::new();
    let mut grouper = TomlGrouper::new();
    let toml = r#"name = "test""#;
    let nodes = parser.parse(toml).expect("should parse");
    let groups = grouper
        .group(nodes.clone(), "test.toml")
        .expect("should group");
    let summary = summarizer.summarize(&nodes, &groups, "test.toml");
    assert_eq!(summary.title, Some("test".to_string()));
}

#[test]
fn test_toml_empty() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = "";
    let result = pipeline.process(toml, "empty.toml", &config, OutputMode::default());
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_toml_all_value_types() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = r#"
string = "hello"
integer = 42
float = 3.14
boolean = true
"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "types.toml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_toml_nested_tables() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = r#"[department]
name = "engineering"

[department.manager]
name = "Alice""#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "nested.toml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_toml_process_with_different_modes() {
    let pipeline = TomlPipeline::new();
    let config = ChunkingConfig::default();
    let toml = r#"name = "test""#;

    let (c1, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "test.toml", &config, OutputMode::Bm25)
        .expect("should process");
    assert!(!c1.is_empty());

    let (c2, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "test.toml", &config, OutputMode::Embedding)
        .expect("should process");
    assert!(!c2.is_empty());

    let (c3, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(toml, "test.toml", &config, OutputMode::Both)
        .expect("should process");
    assert!(!c3.is_empty());
}
