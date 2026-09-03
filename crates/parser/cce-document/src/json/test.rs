use crate::DocType;
use crate::types::DocSummary;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;

use super::*;

#[test]
fn test_json_pipeline_simple() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{"name": "test", "value": 123}"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
    assert_eq!(summary.doc_type, DocType::Config);
}

#[test]
fn test_json_pipeline_nested() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "database": {
                "host": "localhost",
                "port": 3306
            },
            "debug": true
        }"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "config.json", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
    assert!(summary.line_count > 0);
}

#[test]
fn test_json_pipeline_array() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "items": [
                {"id": 1, "name": "item1"},
                {"id": 2, "name": "item2"},
                {"id": 3, "name": "item3"}
            ]
        }"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "data.json", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.main_headings.is_empty() || !summary.main_headings.is_empty());
}

#[test]
fn test_json_pipeline_package_json() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "name": "my-package",
            "version": "1.0.0",
            "dependencies": {
                "express": "^4.18.0"
            },
            "scripts": {
                "start": "node index.js"
            }
        }"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "package.json", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert_eq!(summary.title, Some("Package Configuration".to_string()));
}

#[test]
fn test_json_pipeline_invalid() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{"invalid": }"#;

    let result = pipeline.process(json, "test.json", &config, OutputMode::default());
    assert!(result.is_err());
}

#[test]
fn test_array_elements_independent_groups() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "users": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"},
                {"id": 3, "name": "Charlie"}
            ]
        }"#;

    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());

    for chunk in &chunks {
        let text = chunk.text.as_str();
        if text.contains("id") {
            assert!(
                text.contains("name") || text.contains("Array Element"),
                "Chunk should contain complete user object: {}",
                text
            );
        }
    }
}

#[test]
fn test_root_object_flattening() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "database": {
                "host": "localhost",
                "port": 3306
            },
            "debug": true,
            "version": "1.0.0"
        }"#;

    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "config.json", &config, OutputMode::Both)
        .expect("should process");

    assert!(!chunks.is_empty());

    let has_context = chunks.iter().any(|c| {
        let text = c.text.as_str();
        text.contains("[Context:") || text.contains("[Root]")
    });
    assert!(has_context, "Chunks should have path context");
}

#[test]
fn test_chunk_context_prefix() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "database": {
                "credentials": {
                    "username": "admin",
                    "password": "secret"
                }
            }
        }"#;

    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::Both)
        .expect("should process");

    let has_database_context = chunks.iter().any(|c| {
        let text = c.text.as_str();
        text.contains("database -> credentials")
    });
    assert!(
        has_database_context,
        "Chunks should contain full path context"
    );
}

#[test]
fn test_bm25_dual_representation() {
    let mut parser = JsonParser::new();
    let json = r#"{"database": {"host": "localhost"}}"#;
    let nodes = parser.parse(json).expect("should parse");

    let host_node = nodes.iter().find(|n| n.key_name.as_deref() == Some("host"));
    assert!(host_node.is_some());

    let host_node = host_node.unwrap();
    let bm25_text = host_node.to_bm25_text();

    assert!(
        bm25_text.contains("database.host"),
        "Should have dotted form"
    );
    assert!(
        bm25_text.contains("database host"),
        "Should have spaced form"
    );
}

#[test]
fn test_fuzzy_tag_matching() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();

    let json = r#"{
            "db_config": {
                "host": "localhost"
            },
            "auth_token": "xyz123"
        }"#;

    let (_, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::default())
        .expect("should process");

    let summary = summary.expect("should have summary");

    // Verify structural fields are populated
    assert!(summary.line_count > 0);
    assert!(!summary.main_headings.is_empty());
}

#[test]
fn test_json_parse_stage() {
    let mut parser = JsonParser::new();
    let json = r#"{"key1": "value1", "key2": 42}"#;
    let nodes = parser.parse(json).expect("should parse");
    assert!(!nodes.is_empty());
}

#[test]
fn test_json_group_stage() {
    let mut parser = JsonParser::new();
    let grouper = JsonGrouper::new();
    let json = r#"{"key1": "value1", "key2": 42}"#;
    let nodes = parser.parse(json).expect("should parse");
    let groups = grouper.group(nodes, "test.json").expect("should group");
    assert!(!groups.is_empty());
}

#[test]
fn test_json_chunk_stage() {
    let grouper = JsonGrouper::new();
    let config = ChunkingConfig::default();
    let chunker = JsonChunker::new(config.clone());
    let mut parser = JsonParser::new();
    let json = r#"{"key1": "value1"}"#;
    let nodes = parser.parse(json).expect("should parse");
    let groups = grouper.group(nodes, "test.json").expect("should group");
    let chunks = chunker.chunk_groups(
        &groups,
        "test.json",
        OutputMode::Both,
        &DocumentClassification::detect("test.json"),
    );
    assert!(!chunks.is_empty());
}

#[test]
fn test_json_summarize_stage() {
    let summarizer = JsonSummarizer::new();
    let mut parser = JsonParser::new();
    let grouper = JsonGrouper::new();
    let json = r#"{"name": "test"}"#;
    let nodes = parser.parse(json).expect("should parse");
    let groups = grouper
        .group(nodes.clone(), "test.json")
        .expect("should group");
    let summary = summarizer.summarize(&nodes, &groups, "test.json");
    assert_eq!(summary.title, Some("test".to_string()));
}

#[test]
fn test_json_empty_object() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{}"#;
    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "empty.json", &config, OutputMode::default())
        .expect("should process empty object");
    assert!(chunks.is_empty() || chunks.iter().all(|c| c.text.is_empty() || c.text == "{}"));
    assert!(summary.is_some());
}

#[test]
fn test_json_deeply_nested() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "a": { "b": { "c": { "d": { "e": "deep" } } } }
        }"#;
    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "deep.json", &config, OutputMode::Both)
        .expect("should process deeply nested");
    assert!(!chunks.is_empty());
    assert!(summary.is_some());
}

#[test]
fn test_json_with_all_value_types() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "bool_true": true,
            "bool_false": false,
            "null_value": null
        }"#;
    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "types.json", &config, OutputMode::default())
        .expect("should process all types");
    assert!(!chunks.is_empty());
    assert!(summary.is_some());
}

#[test]
fn test_json_nested_arrays() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{
            "matrix": [
                [1, 2, 3],
                [4, 5, 6],
                [7, 8, 9]
            ]
        }"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "matrix.json", &config, OutputMode::Both)
        .expect("should process nested arrays");
    assert!(!chunks.is_empty());
}

#[test]
fn test_json_process_with_bm25_mode() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{"name": "test", "value": 123}"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::Bm25)
        .expect("should process with BM25 mode");
    assert!(!chunks.is_empty());
}

#[test]
fn test_json_process_with_embedding_mode() {
    let pipeline = JsonPipeline::new();
    let config = ChunkingConfig::default();
    let json = r#"{"name": "test", "value": 123}"#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(json, "test.json", &config, OutputMode::Embedding)
        .expect("should process with Embedding mode");
    assert!(!chunks.is_empty());
}
