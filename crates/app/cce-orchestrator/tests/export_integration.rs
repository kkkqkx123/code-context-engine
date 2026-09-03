//! Integration tests for the export module
//!
//! Tests the full pipeline: config → aggregator → formatter → exporter
//! using realistic mock chunk data.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cce_parser::ast_to_nl::chunker::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
};
use cce_parser::grouper::GroupType;
use cce_parser::summary::FileSummary;
use cce_types::Span;
use cce_types::entity::EntityKind;
use cce_types::language::Language;

use cce_orchestrator::export::{
    EntityNlDocument, ExportConfig, ExportError, ExportResult, ExportSummaryView, FileAggregator,
    FileNlDocument, MarkdownFormatter, NlDocumentExporter, RelatedEntity, RelationEnhancerConfig,
    paths_match, relative_source_path,
};

// ---------------------------------------------------------------------------
// Helper: build a mock ChunkedResult for testing
// ---------------------------------------------------------------------------

/// Builder for constructing test chunks
struct ChunkBuilder {
    group_id: String,
    path: ChunkPath,
    entity_name: String,
    kind: EntityKind,
    text: String,
    token_count: usize,
    file_path: String,
    start_row: usize,
    end_row: usize,
}

impl ChunkBuilder {
    fn new(group_id: &str) -> Self {
        Self {
            group_id: group_id.to_string(),
            path: ChunkPath::Bm25,
            entity_name: String::new(),
            kind: EntityKind::Function,
            text: String::new(),
            token_count: 0,
            file_path: "test.rs".to_string(),
            start_row: 1,
            end_row: 1,
        }
    }

    fn path(mut self, path: ChunkPath) -> Self {
        self.path = path;
        self
    }

    fn entity_name(mut self, name: &str) -> Self {
        self.entity_name = name.to_string();
        self
    }

    fn kind(mut self, kind: EntityKind) -> Self {
        self.kind = kind;
        self
    }

    fn text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }

    fn token_count(mut self, count: usize) -> Self {
        self.token_count = count;
        self
    }

    fn file_path(mut self, path: &str) -> Self {
        self.file_path = path.to_string();
        self
    }

    fn row_range(mut self, start: usize, end: usize) -> Self {
        self.start_row = start;
        self.end_row = end;
        self
    }

    fn build(self) -> ChunkedResult {
        let span = Span::from_lines(self.start_row, self.end_row);
        let meta = ChunkMetadata::for_code(
            self.file_path,
            span,
            Language::Rust,
            CodeSpecificMetadata {
                entity_kind: self.kind,
                ..Default::default()
            },
        );

        ChunkedResult {
            chunk_id: format!("{}_{}", self.group_id, self.path.as_str()),
            source_group_id: self.group_id,
            path: self.path,
            group_type: if self.kind == EntityKind::Function {
                GroupType::Standalone
            } else if self.kind == EntityKind::Struct {
                GroupType::ClassWithMethods
            } else {
                GroupType::Standalone
            },
            chunk_index: 0,
            total_chunks: 1,
            end_byte: self.text.len(),
            text: self.text,
            bm25_title: Some(self.entity_name),
            bm25_keywords: vec![],
            token_count: self.token_count,
            start_byte: 0,
            prev_overlap: None,
            next_overlap: None,
            related_groups: vec![],
            self_contained: false,
            metadata: meta,
        }
    }
}

/// Helper to create an embedding chunk quickly (backward compat)
#[allow(clippy::too_many_arguments)]
fn make_embedding_chunk(
    group_id: &str,
    entity_name: &str,
    kind: EntityKind,
    description: &str,
    token_count: usize,
    file_path: &str,
    start_row: usize,
    end_row: usize,
) -> ChunkedResult {
    ChunkBuilder::new(group_id)
        .entity_name(entity_name)
        .kind(kind)
        .text(description)
        .token_count(token_count)
        .file_path(file_path)
        .row_range(start_row, end_row)
        .path(ChunkPath::Embedding)
        .build()
}

// ---------------------------------------------------------------------------
// Tests for ExportConfig
// ---------------------------------------------------------------------------

#[test]
fn test_export_config_default() {
    let config = ExportConfig::new(PathBuf::from("/test/project"), 1);
    assert_eq!(config.project_root, PathBuf::from("/test/project"));
    assert!(config.include_summary);
    assert!(!config.enable_relation_enhancement);
}

#[test]
fn test_export_config_output_dir() {
    let config = ExportConfig::new(PathBuf::from("/test/project"), 1);
    let output = config.output_dir();
    assert_eq!(output, PathBuf::from("/test/project/.cce/nl_docs"));
}

#[test]
fn test_export_config_builders() {
    let config = ExportConfig::new(PathBuf::from("/p"), 1)
        .with_summary(false)
        .with_relation_enhancement(true);
    assert!(!config.include_summary);
    assert!(config.enable_relation_enhancement);
}

#[test]
fn test_export_config_from_module_config() {
    let module_config = cce_config::modules::ExportModuleConfig::new()
        .with_summary(false)
        .with_relation_enhancement(true);
    let config =
        ExportConfig::from_module_config(&module_config, PathBuf::from("/test/project"), 1);
    assert!(!config.include_summary);
    assert!(config.enable_relation_enhancement);
    assert_eq!(config.project_root, PathBuf::from("/test/project"));
}

// ---------------------------------------------------------------------------
// Tests for RelationEnhancerConfig
// ---------------------------------------------------------------------------

#[test]
fn test_relation_enhancer_config_default() {
    let config = RelationEnhancerConfig::new();
    assert_eq!(config.max_related_entities, 10);
    assert!(config.include_cross_file);
    assert!(!config.include_stdlib);
}

#[test]
fn test_relation_enhancer_config_builders() {
    let config = RelationEnhancerConfig::new()
        .with_max_related(5)
        .with_cross_file(false)
        .with_stdlib(true);
    assert_eq!(config.max_related_entities, 5);
    assert!(!config.include_cross_file);
    assert!(config.include_stdlib);
}

// ---------------------------------------------------------------------------
// Tests for ExportResult
// ---------------------------------------------------------------------------

#[test]
fn test_export_result_new() {
    let result = ExportResult::new();
    assert_eq!(result.exported_count, 0);
    assert_eq!(result.removed_count, 0);
    assert!(result.failed.is_empty());
    assert!(result.output_paths.is_empty());
}

#[test]
fn test_export_result_is_success() {
    let mut result = ExportResult::new();
    assert!(result.is_success());

    result.failed.push((PathBuf::from("a.rs"), "error".into()));
    assert!(!result.is_success());
}

#[test]
fn test_export_result_total_processed() {
    let mut result = ExportResult::new();
    result.exported_count = 10;
    result.removed_count = 3;
    assert_eq!(result.total_processed(), 13);
}

// ---------------------------------------------------------------------------
// Tests for path_utils
// ---------------------------------------------------------------------------

#[test]
fn test_relative_source_path_unix() {
    assert_eq!(
        relative_source_path("src/main.rs", Path::new("/project")),
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn test_relative_source_path_windows() {
    assert_eq!(
        relative_source_path("src\\main.rs", Path::new("/project")),
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn test_paths_match_exact() {
    assert!(paths_match("src/main.rs", "src/main.rs"));
}

#[test]
fn test_paths_match_windows_sep() {
    assert!(paths_match("src/main.rs", "src\\main.rs"));
}

#[test]
fn test_paths_match_different() {
    assert!(!paths_match("src/main.rs", "src/lib.rs"));
}

// ---------------------------------------------------------------------------
// Tests for EntityNlDocument
// ---------------------------------------------------------------------------

#[test]
fn test_entity_nl_document_new() {
    let span = Span::from_lines(10, 30);
    let entity = EntityNlDocument::new(
        "hello".into(),
        EntityKind::Function,
        Vec::new(),
        "This is a greeting function".into(),
        span,
        GroupType::Standalone,
    );
    assert_eq!(entity.name, "hello");
    assert_eq!(entity.kind, EntityKind::Function);
    assert!(entity.related_entities.is_empty());
}

#[test]
fn test_entity_nl_document_add_related() {
    let mut entity = EntityNlDocument::new(
        "foo".into(),
        EntityKind::Function,
        Vec::new(),
        "does something".into(),
        Span::from_lines(1, 5),
        GroupType::Standalone,
    );

    let related = RelatedEntity {
        name: "bar".into(),
        relation_type: "calls".into(),
        file_path: Some("other.rs".into()),
        location: Some(Span::from_lines(3, 3)),
    };
    entity.add_related(related);

    assert_eq!(entity.related_entities.len(), 1);
    assert_eq!(entity.related_entities[0].name, "bar");
    assert_eq!(entity.related_entities[0].relation_type, "calls");
}

// ---------------------------------------------------------------------------
// Tests for FileNlDocument
// ---------------------------------------------------------------------------

#[test]
fn test_file_nl_document_new() {
    let doc = FileNlDocument::new("src/main.rs".into(), Language::Rust);
    assert_eq!(doc.source_path, "src/main.rs");
    assert_eq!(doc.language, Language::Rust);
    assert!(doc.entities.is_empty());
    assert!(doc.imports.is_empty());
    assert!(doc.exports.is_empty());
    assert_eq!(doc.total_tokens, 0);
    assert_eq!(doc.entity_count(), 0);
}

#[test]
fn test_file_nl_document_with_summary() {
    let summary = ExportSummaryView::from(
        FileSummary::new("src/main.rs")
            .with_summary("Main entry point")
            .with_imports(vec!["std::io".into()])
            .with_line_count(50),
    );

    let doc = FileNlDocument::new("src/main.rs".into(), Language::Rust).with_summary(summary);

    assert!(doc.summary.is_some());
    assert_eq!(
        doc.summary.as_ref().unwrap().summary_text,
        "Main entry point"
    );
}

#[test]
fn test_file_nl_document_add_entity() {
    let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);
    let entity = EntityNlDocument::new(
        "helper".into(),
        EntityKind::Function,
        Vec::new(),
        "helper function".into(),
        Span::from_lines(1, 10),
        GroupType::Standalone,
    );
    doc.add_entity(entity);
    assert_eq!(doc.entity_count(), 1);
}

#[test]
fn test_file_nl_document_set_imports_exports() {
    let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);
    doc.set_imports(vec!["std::fmt".into(), "serde::Serialize".into()]);
    doc.set_exports(vec!["run".into(), "Config".into()]);
    assert_eq!(doc.imports.len(), 2);
    assert_eq!(doc.exports.len(), 2);
}

// ---------------------------------------------------------------------------
// Tests for FileAggregator
// ---------------------------------------------------------------------------

#[test]
fn test_file_aggregator_empty_chunks() {
    let aggregator = FileAggregator::new();
    let result = aggregator.aggregate(&[], None);
    assert!(result.is_err());
    match result {
        Err(ExportError::NoChunks) => {}
        _ => panic!("Expected NoChunks error"),
    }
}

#[test]
fn test_file_aggregator_single_entity() {
    let aggregator = FileAggregator::new();
    let chunks = vec![
        ChunkBuilder::new("group_1")
            .entity_name("greet")
            .kind(EntityKind::Function)
            .text("Greets the user with a message.")
            .token_count(15)
            .file_path("src/main.rs")
            .row_range(0, 5)
            .path(ChunkPath::Embedding)
            .build(),
    ];

    let doc = aggregator.aggregate(&chunks, None).unwrap();
    assert_eq!(doc.source_path, "src/main.rs");
    assert_eq!(doc.language, Language::Rust);
    assert_eq!(doc.entities.len(), 1);
    assert_eq!(doc.entities[0].name, "greet");
    assert_eq!(doc.entities[0].kind, EntityKind::Function);
    assert_eq!(doc.total_tokens, 15);
}

#[test]
fn test_file_aggregator_multiple_entities() {
    let aggregator = FileAggregator::new();
    let chunks = vec![
        make_embedding_chunk(
            "group_1",
            "add",
            EntityKind::Function,
            "Adds two numbers.",
            10,
            "src/math.rs",
            0,
            3,
        ),
        make_embedding_chunk(
            "group_2",
            "Calculator",
            EntityKind::Struct,
            "A simple calculator struct.",
            20,
            "src/math.rs",
            5,
            30,
        ),
    ];

    let doc = aggregator.aggregate(&chunks, None).unwrap();
    assert_eq!(doc.entities.len(), 2);
    assert_eq!(doc.entities[0].name, "add");
    assert_eq!(doc.entities[1].name, "Calculator");
    assert_eq!(doc.total_tokens, 30);
}

#[test]
fn test_file_aggregator_with_summary() {
    let aggregator = FileAggregator::new();
    let chunks = vec![make_embedding_chunk(
        "group_1",
        "run",
        EntityKind::Function,
        "Entry point function.",
        8,
        "src/main.rs",
        0,
        10,
    )];

    let summary = ExportSummaryView::from(
        FileSummary::new("src/main.rs")
            .with_summary("Application entry point")
            .with_imports(vec!["std::env".into()])
            .with_line_count(100),
    );

    let doc = aggregator.aggregate(&chunks, Some(summary)).unwrap();
    assert!(doc.summary.is_some());
    assert_eq!(doc.imports.len(), 1);
    assert_eq!(doc.imports[0], "std::env");
}

// ---------------------------------------------------------------------------
// Tests for MarkdownFormatter
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_formatter_basic() {
    let formatter = MarkdownFormatter::new();
    let doc = FileNlDocument::new("src/main.rs".into(), Language::Rust);
    let output = formatter.format(&doc).unwrap();

    assert!(output.contains("# src/main.rs"));
}

#[test]
fn test_markdown_formatter_with_entities() {
    let formatter = MarkdownFormatter::new();
    let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);

    let entity = EntityNlDocument::new(
        "compute".into(),
        EntityKind::Function,
        Vec::new(),
        "Performs a complex computation.".into(),
        Span::from_lines(5, 15),
        GroupType::Standalone,
    );
    doc.add_entity(entity);

    let output = formatter.format(&doc).unwrap();
    assert!(output.contains("function compute"));
    assert!(output.contains("Performs a complex computation."));
}

#[test]
fn test_markdown_formatter_with_imports_exports() {
    let formatter = MarkdownFormatter::new();
    let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);
    doc.set_imports(vec!["std::collections::HashMap".into()]);
    doc.set_exports(vec!["run".into(), "Config".into()]);

    let output = formatter.format(&doc).unwrap();
    assert!(output.contains("imports"));
    assert!(output.contains("std::collections::HashMap"));
    assert!(output.contains("exports"));
    assert!(output.contains("run"));
    assert!(output.contains("Config"));
}

#[test]
fn test_markdown_formatter_with_related_entities() {
    let formatter = MarkdownFormatter::new();
    let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);

    let mut entity = EntityNlDocument::new(
        "process".into(),
        EntityKind::Function,
        Vec::new(),
        "Processes input data.".into(),
        Span::from_lines(1, 20),
        GroupType::Standalone,
    );
    entity.add_related(RelatedEntity {
        name: "validate".into(),
        relation_type: "calls".into(),
        file_path: Some("src/validate.rs".into()),
        location: Some(Span::from_lines(5, 5)),
    });
    doc.add_entity(entity);

    let output = formatter.format(&doc).unwrap();
    assert!(output.contains("related"));
    assert!(output.contains("validate"));
    assert!(output.contains("calls"));
    assert!(output.contains("src/validate.rs"));
}

// ---------------------------------------------------------------------------
// Tests for NlDocumentExporter
// ---------------------------------------------------------------------------

#[test]
fn test_exporter_config_accessor() {
    let config = ExportConfig::new(PathBuf::from("/test"), 1);
    let exporter = NlDocumentExporter::new(config);
    assert_eq!(exporter.config().project_root, PathBuf::from("/test"));
}

#[tokio::test]
async fn test_exporter_export_file_to_temp() {
    let temp_dir = std::env::temp_dir().join("cce_export_test");
    let config = ExportConfig::new(temp_dir.clone(), 1);
    let exporter = NlDocumentExporter::new(config);

    let chunks = vec![make_embedding_chunk(
        "group_1",
        "main",
        EntityKind::Function,
        "Main entry point.",
        5,
        "src/main.rs",
        0,
        10,
    )];

    let result = exporter.export_file(&chunks, None).await;

    assert!(result.is_ok());
    let output_path = result.unwrap();
    assert!(output_path.exists());

    let content = tokio::fs::read_to_string(&output_path).await.unwrap();
    assert!(content.contains("Main entry point."));

    tokio::fs::remove_dir_all(&temp_dir).await.ok();
}

#[tokio::test]
async fn test_exporter_export_file_with_summary() {
    let temp_dir = std::env::temp_dir().join("cce_export_summary_test");
    let config = ExportConfig::new(temp_dir.clone(), 1);
    let exporter = NlDocumentExporter::new(config);

    let chunks = vec![make_embedding_chunk(
        "group_1",
        "run",
        EntityKind::Function,
        "Runs the application.",
        8,
        "src/main.rs",
        0,
        15,
    )];

    let summary = ExportSummaryView::from(
        FileSummary::new("src/main.rs")
            .with_summary("Application entry point")
            .with_line_count(50),
    );

    let result = exporter.export_file(&chunks, Some(&summary)).await;

    assert!(result.is_ok());
    let output_path = result.unwrap();
    assert!(output_path.exists());

    let content = tokio::fs::read_to_string(&output_path).await.unwrap();
    assert!(content.contains("Runs the application."));

    tokio::fs::remove_dir_all(&temp_dir).await.ok();
}

#[tokio::test]
async fn test_exporter_export_batch() {
    let temp_dir = std::env::temp_dir().join("cce_export_batch_test");
    let config = ExportConfig::new(temp_dir.clone(), 1);
    let exporter = NlDocumentExporter::new(config);

    let mut file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::new();
    file_chunks.insert(
        "src/main.rs".into(),
        vec![make_embedding_chunk(
            "g1",
            "main",
            EntityKind::Function,
            "Entry point.",
            5,
            "src/main.rs",
            0,
            10,
        )],
    );
    file_chunks.insert(
        "src/lib.rs".into(),
        vec![make_embedding_chunk(
            "g2",
            "helper",
            EntityKind::Function,
            "Helper utility.",
            3,
            "src/lib.rs",
            0,
            5,
        )],
    );

    let result = exporter.export_batch(&file_chunks, None).await.unwrap();
    assert_eq!(result.exported_count, 2);
    assert_eq!(result.output_paths.len(), 2);

    for path in &result.output_paths {
        assert!(path.exists());
    }

    tokio::fs::remove_dir_all(&temp_dir).await.ok();
}

#[tokio::test]
async fn test_exporter_remove_file() {
    let temp_dir = std::env::temp_dir().join("cce_export_remove_test");
    let config = ExportConfig::new(temp_dir.clone(), 1);
    let exporter = NlDocumentExporter::new(config);

    let chunks = vec![make_embedding_chunk(
        "g1",
        "main",
        EntityKind::Function,
        "desc",
        3,
        "src/main.rs",
        0,
        10,
    )];

    let output_path = exporter.export_file(&chunks, None).await.unwrap();
    assert!(output_path.exists());

    exporter
        .remove_file(std::path::Path::new("src/main.rs"))
        .await
        .unwrap();
    assert!(!output_path.exists());

    tokio::fs::remove_dir_all(&temp_dir).await.ok();
}

#[tokio::test]
async fn test_exporter_remove_nonexistent_file() {
    let config = ExportConfig::new(PathBuf::from("/nonexistent"), 1);
    let exporter = NlDocumentExporter::new(config);

    // Removing a non-existent file should succeed silently
    let result = exporter
        .remove_file(std::path::Path::new("src/ghost.rs"))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_exporter_export_empty_chunks() {
    let config = ExportConfig::new(PathBuf::from("/tmp"), 1);
    let exporter = NlDocumentExporter::new(config);

    let result = exporter.export_file(&[], None).await;
    assert!(result.is_err());
    match result.err().unwrap() {
        ExportError::NoChunks => {}
        e => panic!("Expected NoChunks error, got: {}", e),
    }
}

#[tokio::test]
async fn test_exporter_set_clear_relation_enhancement() {
    let config = ExportConfig::new(PathBuf::from("/tmp"), 1);
    let exporter = NlDocumentExporter::new(config);

    // Clearing when no enhancement is configured should be fine
    exporter.clear_relation_enhancement();

    // Setting with a dummy relation index - we can't easily create a RelationIndex
    // here, so we just verify the public API is callable without panicking.
    // A full test would require a properly initialized RelationIndex.
}

// ---------------------------------------------------------------------------
// Tests for ExportError Display
// ---------------------------------------------------------------------------

#[test]
fn test_export_error_display() {
    let err = ExportError::NoChunks;
    assert_eq!(format!("{}", err), "No chunks to export");

    let err = ExportError::Formatter("invalid markdown".into());
    assert_eq!(format!("{}", err), "Formatter error: invalid markdown");

    let err = ExportError::InvalidSourcePath(PathBuf::from("bad.rs"));
    assert_eq!(format!("{}", err), "Invalid source path: bad.rs");
}

// ---------------------------------------------------------------------------
// Tests for MarkdownFormatter with different entity kinds
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_formatter_various_entity_kinds() {
    let formatter = MarkdownFormatter::new();
    let mut doc = FileNlDocument::new("src/types.rs".into(), Language::Rust);

    for (name, kind) in [
        ("MyStruct", EntityKind::Struct),
        ("MyEnum", EntityKind::Enum),
        ("MyTrait", EntityKind::Trait),
        ("do_stuff", EntityKind::Function),
        ("MyClass", EntityKind::Class),
        ("MyInterface", EntityKind::Interface),
    ] {
        // Use empty description to trigger "kind name" format
        let entity = EntityNlDocument::new(
            name.into(),
            kind,
            Vec::new(),
            String::new(),
            Span::from_lines(1, 10),
            GroupType::Standalone,
        );
        doc.add_entity(entity);
    }

    let output = formatter.format(&doc).unwrap();
    assert!(output.contains("struct MyStruct"));
    assert!(output.contains("enum MyEnum"));
    assert!(output.contains("trait MyTrait"));
    assert!(output.contains("function do_stuff"));
    assert!(output.contains("class MyClass"));
    assert!(output.contains("interface MyInterface"));
}

// ---------------------------------------------------------------------------
// Tests for MarkdownFormatter with summary
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_formatter_with_summary() {
    let formatter = MarkdownFormatter::new();
    let summary = ExportSummaryView::from(
        FileSummary::new("src/app.rs")
            .with_summary("Core application module")
            .with_line_count(200),
    );
    let doc = FileNlDocument::new("src/app.rs".into(), Language::Rust).with_summary(summary);

    let output = formatter.format(&doc).unwrap();
    assert!(output.contains("# src/app.rs"));
}

// ---------------------------------------------------------------------------
// Tests for ISO language display coverage
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_formatter_various_languages() {
    let formatter = MarkdownFormatter::new();

    for (lang, _label) in [
        (Language::Rust, "Rust"),
        (Language::Python, "Python"),
        (Language::JavaScript, "JavaScript"),
        (Language::TypeScript, "TypeScript"),
        (Language::Go, "Go"),
        (Language::Java, "Java"),
        (Language::Cpp, "C++"),
        (Language::C, "C"),
        (Language::Kotlin, "Kotlin"),
        (Language::Ruby, "Ruby"),
        (Language::Php, "PHP"),
        (Language::CSharp, "C#"),
        (Language::Scala, "Scala"),
        (Language::Dart, "Dart"),
        (Language::Html, "HTML"),
        (Language::Css, "CSS"),
        (Language::Scss, "SCSS"),
        (Language::Less, "LESS"),
        (Language::Vue, "Vue"),
        (Language::Svelte, "Svelte"),
        (Language::Jsx, "JSX"),
        (Language::Tsx, "TSX"),
        (Language::Json, "JSON"),
        (Language::Yaml, "YAML"),
        (Language::Toml, "TOML"),
        (Language::Xml, "XML"),
        (Language::Unknown, "Unknown"),
    ] {
        let doc = FileNlDocument::new("file.ext".into(), lang);
        let output = formatter.format(&doc).unwrap();
        assert!(
            output.contains("# file.ext"),
            "Expected file path in title for {:?}",
            lang
        );
    }
}
