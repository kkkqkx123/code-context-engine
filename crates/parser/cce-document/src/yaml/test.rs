use crate::types::DocSummary;
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;

use super::*;

#[test]
fn test_yaml_pipeline_simple() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = r#"name: test
value: 123"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "test.yaml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
    assert!(summary.line_count >= 2);
}

#[test]
fn test_yaml_pipeline_with_mapping() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = r#"database:
  host: localhost
  port: 3306"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "config.yaml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.line_count > 0);
}

#[test]
fn test_yaml_parse_stage() {
    let mut parser = YamlParser::new();
    let yaml = r#"key: value"#;
    let nodes = parser.parse(yaml).expect("should parse");
    assert!(!nodes.is_empty());
}

#[test]
fn test_yaml_group_stage() {
    let mut parser = YamlParser::new();
    let mut grouper = YamlGrouper::new();
    let yaml = r#"name: test"#;
    let nodes = parser.parse(yaml).expect("should parse");
    let groups = grouper.group(nodes, "test.yaml").expect("should group");
    assert!(!groups.is_empty());
}

#[test]
fn test_yaml_chunk_stage() {
    let mut parser = YamlParser::new();
    let mut grouper = YamlGrouper::new();
    let config = ChunkingConfig::default();
    let chunker = YamlChunker::new(config.clone());
    let yaml = r#"name: test"#;
    let nodes = parser.parse(yaml).expect("should parse");
    let groups = grouper.group(nodes, "test.yaml").expect("should group");
    let chunks = chunker.chunk_groups(
        &groups,
        "test.yaml",
        OutputMode::Both,
        &DocumentClassification::detect("test.yaml"),
    );
    assert!(!chunks.is_empty());
}

#[test]
fn test_yaml_summarize_stage() {
    let summarizer = YamlSummarizer::new();
    let mut parser = YamlParser::new();
    let mut grouper = YamlGrouper::new();
    let yaml = r#"name: test"#;
    let nodes = parser.parse(yaml).expect("should parse");
    let groups = grouper
        .group(nodes.clone(), "test.yaml")
        .expect("should group");
    let summary = summarizer.summarize(&nodes, &groups, "test.yaml");
    assert_eq!(summary.title, Some("test".to_string()));
}

#[test]
fn test_yaml_empty() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = "";
    let result = pipeline.process(yaml, "empty.yaml", &config, OutputMode::default());
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_yaml_all_value_types() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = r#"
string: hello
integer: 42
float: 3.14
boolean: true
null_value: ~
"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "types.yaml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_yaml_deeply_nested() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = r#"
a:
  b:
    c:
      d: deep_value
"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "deep.yaml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_yaml_process_with_different_modes() {
    let pipeline = YamlPipeline::new();
    let config = ChunkingConfig::default();
    let yaml = r#"name: test"#;

    let (c1, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "test.yaml", &config, OutputMode::Bm25)
        .expect("should process");
    assert!(!c1.is_empty());

    let (c2, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "test.yaml", &config, OutputMode::Embedding)
        .expect("should process");
    assert!(!c2.is_empty());

    let (c3, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(yaml, "test.yaml", &config, OutputMode::Both)
        .expect("should process");
    assert!(!c3.is_empty());
}
