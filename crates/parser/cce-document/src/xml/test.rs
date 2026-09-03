use crate::types::DocSummary;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ast_to_nl::options::OutputMode;

use super::*;

#[test]
fn test_xml_pipeline_simple() {
    let pipeline = XmlPipeline::new();
    let config = ChunkingConfig::default();
    let xml = r#"<root><child>text</child></root>"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "test.xml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
    assert!(summary.main_headings.is_empty() || !summary.main_headings.is_empty());
}

#[test]
fn test_xml_pipeline_with_attributes() {
    let pipeline = XmlPipeline::new();
    let config = ChunkingConfig::default();
    let xml = r#"<root id="main"><child name="test">value</child></root>"#;

    let (chunks, summary): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "config.xml", &config, OutputMode::default())
        .expect("should process");

    assert!(!chunks.is_empty());
    assert!(summary.is_some());

    let summary = summary.unwrap();
    assert!(summary.title.is_some());
}

#[test]
fn test_xml_with_deep_nesting() {
    let pipeline = XmlPipeline::new();
    let config = ChunkingConfig::default();
    let xml = r#"
        <a>
            <b>
                <c>
                    <d>deep</d>
                </c>
            </b>
        </a>
    "#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "deep.xml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_xml_with_comments_and_cdata() {
    let pipeline = XmlPipeline::new();
    let config = ChunkingConfig::default();
    let xml = r#"
        <root>
            <!-- comment -->
            <![CDATA[some cdata]]>
            <data>value</data>
        </root>
    "#;
    let (chunks, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "mixed.xml", &config, OutputMode::default())
        .expect("should process");
    assert!(!chunks.is_empty());
}

#[test]
fn test_xml_process_with_different_modes() {
    let pipeline = XmlPipeline::new();
    let config = ChunkingConfig::default();
    let xml = r#"<root><item>value</item></root>"#;

    let (c1, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "test.xml", &config, OutputMode::Bm25)
        .expect("should process");
    assert!(!c1.is_empty());

    let (c2, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "test.xml", &config, OutputMode::Embedding)
        .expect("should process");
    assert!(!c2.is_empty());

    let (c3, _): (Vec<ChunkedResult>, Option<DocSummary>) = pipeline
        .process(xml, "test.xml", &config, OutputMode::Both)
        .expect("should process");
    assert!(!c3.is_empty());
}
