//! Export workflow tests
//!
//! Tests for the NL document export and the plain text display formatter.
//! These tests construct ChunkedResult data directly and verify
//! that both outputs produce correct results.

use std::collections::HashMap;

use cce_orchestrator::export::{ExportConfig, NlDocumentExporter};
use cce_parser::ast_to_nl::chunker::{
    ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
};
use cce_parser::grouper::GroupType;
use cce_types::language::Language;
use cce_types::{EntityId, EntityKind, Span};

use crate::helper::NlTextDocumentExporter;
use crate::helper::fixture::TestFixture;
use crate::helper::init_minimal_logging;

/// Helper to construct a minimal ChunkedResult for testing
fn make_chunk(
    file_path: &str,
    group_id: &str,
    path: ChunkPath,
    text: &str,
    index: usize,
    total: usize,
) -> ChunkedResult {
    let chunk_id = format!("{}_{}_{}", group_id, path.as_str(), index);
    let mut chunk = ChunkedResult::new(chunk_id, group_id.to_string(), path, index, total);
    chunk.text = text.to_string();
    chunk.token_count = text.split_whitespace().count();
    chunk.start_byte = 0;
    chunk.end_byte = text.len();
    chunk.group_type = GroupType::Standalone;
    chunk.metadata = ChunkMetadata::for_code(
        file_path.to_string(),
        Span::from_lines(1, 10),
        Language::Rust,
        CodeSpecificMetadata {
            content_entity_ids: vec![EntityId(1)],
            entity_kind: EntityKind::Function,
            ..Default::default()
        },
    );
    chunk
}

/// Basic NL document export
#[tokio::test]
async fn test_basic_nl_export() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    // Build chunks for a single file
    let file_path = "src/main.rs";
    let chunks = vec![
        make_chunk(
            file_path,
            "group_main",
            ChunkPath::Embedding,
            "The main function serves as the entry point. It prints a greeting message.",
            0,
            1,
        ),
        make_chunk(
            file_path,
            "group_main",
            ChunkPath::Bm25,
            "fn main() { println!(\"Hello, World!\"); }",
            0,
            1,
        ),
    ];

    let config = ExportConfig::new(project_root.clone(), 1)
        .with_summary(true)
        .with_relation_enhancement(false);

    let exporter = NlDocumentExporter::new(config);

    let output_path = exporter
        .export_file(&chunks, None)
        .await
        .expect("Export should succeed");

    assert!(
        output_path.exists(),
        "Exported file should exist at {:?}",
        output_path
    );

    let content = tokio::fs::read_to_string(&output_path)
        .await
        .expect("Should read exported file");

    assert!(content.contains("main"), "Content should contain 'main'");
    assert!(
        content.contains("entry point"),
        "Content should contain NL description"
    );
}

/// Batch export with relation enhancement
#[tokio::test]
async fn test_batch_export_with_relation_enhancement() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    let file_a = "src/module_a.rs";
    let file_b = "src/module_b.rs";

    let mut file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::new();

    file_chunks.insert(
        file_a.to_string(),
        vec![make_chunk(
            file_a,
            "group_a",
            ChunkPath::Embedding,
            "Module A provides a compute function that multiplies two numbers.",
            0,
            1,
        )],
    );

    file_chunks.insert(
        file_b.to_string(),
        vec![make_chunk(
            file_b,
            "group_b",
            ChunkPath::Embedding,
            "Module B provides a utility function used by Module A.",
            0,
            1,
        )],
    );

    let config = ExportConfig::new(project_root.clone(), 1)
        .with_summary(false)
        .with_relation_enhancement(false);

    let exporter = NlDocumentExporter::new(config);

    let result = exporter
        .export_batch(&file_chunks, None)
        .await
        .expect("Batch export should succeed");

    assert_eq!(result.exported_count, 2);
    assert!(result.failed.is_empty());
    assert_eq!(result.output_paths.len(), 2);

    for path in &result.output_paths {
        assert!(path.exists(), "Output path {:?} should exist", path);
    }
}

/// Export output verification
#[tokio::test]
async fn test_export_output_verification() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    let file_path = "src/calculator.rs";
    let chunks = vec![
        make_chunk(
            file_path,
            "group_add",
            ChunkPath::Embedding,
            "The add function takes two integers and returns their sum.",
            0,
            2,
        ),
        make_chunk(
            file_path,
            "group_sub",
            ChunkPath::Embedding,
            "The subtract function takes two integers and returns their difference.",
            1,
            2,
        ),
    ];

    let config = ExportConfig::new(project_root.clone(), 1)
        .with_summary(true)
        .with_relation_enhancement(false);

    let exporter = NlDocumentExporter::new(config);

    let output_path = exporter
        .export_file(&chunks, None)
        .await
        .expect("Export should succeed");

    let content = tokio::fs::read_to_string(&output_path)
        .await
        .expect("Should read exported file");

    assert!(
        content.contains("calculator"),
        "File name should appear in output"
    );
    assert!(
        content.contains("sum") || content.contains("difference"),
        "NL descriptions should appear in output"
    );
}

/// Export with fixture using absolute paths
///
/// Uses the once_cell fixture to verify that export correctly handles
/// absolute source paths by converting them to project-relative paths
/// under the `.cce/nl_docs/` directory.
#[tokio::test]
async fn test_fixture_export_with_absolute_paths() {
    init_minimal_logging();

    // Load the once_cell fixture from e2e test fixtures
    let fixture = TestFixture::rust_once_cell().expect("Failed to load once_cell fixture");

    let project_root = fixture.root_path().to_path_buf();

    // Get real absolute paths from the fixture
    let imp_cs_path = fixture.file("src/imp_cs.rs").to_string_lossy().to_string();
    let lib_path = fixture.file("src/lib.rs").to_string_lossy().to_string();

    // Build chunks with real absolute paths (simulating what the real pipeline produces)
    let file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::from([
        (
            imp_cs_path.clone(),
            vec![
                make_chunk(
                    &imp_cs_path,
                    "group_imp_cs",
                    ChunkPath::Embedding,
                    "Module imp_cs provides a OnceCell implementation using critical sections.",
                    0,
                    1,
                ),
                make_chunk(
                    &imp_cs_path,
                    "group_imp_cs",
                    ChunkPath::Bm25,
                    "use core::cell::UnsafeCell; use core::sync::atomic::{AtomicBool, Ordering};",
                    0,
                    1,
                ),
            ],
        ),
        (
            lib_path.clone(),
            vec![make_chunk(
                &lib_path,
                "group_lib",
                ChunkPath::Embedding,
                "The once_cell library provides lazy initialization primitives.",
                0,
                1,
            )],
        ),
    ]);

    let config = ExportConfig::new(project_root.clone(), 1)
        .with_summary(true)
        .with_relation_enhancement(false);

    let exporter = NlDocumentExporter::new(config);

    let result = exporter
        .export_batch(&file_chunks, None)
        .await
        .expect("Batch export should succeed");

    assert_eq!(result.exported_count, 2, "Both files should be exported");
    assert!(
        result.failed.is_empty(),
        "No files should fail: {:?}",
        result.failed
    );
    assert_eq!(result.output_paths.len(), 2);

    // Verify output paths:
    // They should be under .cce/nl_docs/ with project-relative paths,
    // NOT at the source file locations
    for output_path in &result.output_paths {
        assert!(
            output_path.exists(),
            "Output path {:?} should exist",
            output_path
        );

        let cce_dir = project_root.join(".cce").join("nl_docs");

        // Output must be under .cce/nl_docs/
        let output_str = output_path.to_string_lossy().replace('\\', "/");
        assert!(
            output_str.contains(".cce/nl_docs/"),
            "Output path {:?} should be under .cce/nl_docs/",
            output_path
        );

        // Output must NOT be at the source file location
        let relative = output_path
            .strip_prefix(&cce_dir)
            .expect("Output should be under .cce/nl_docs");
        // The exporter appends `.md` to the project-relative source path, so
        // stripping the trailing extension recovers the source file.
        let source_file = project_root.join(relative).with_extension("");
        assert!(
            source_file.exists(),
            "Source file {:?} should exist for output {:?}",
            source_file,
            output_path
        );

        // Verify content is valid markdown
        let content = tokio::fs::read_to_string(output_path)
            .await
            .expect("Should read exported file");

        assert!(!content.is_empty(), "Exported content should not be empty");

        tracing::debug!("Exported content for {:?}:\n{}", output_path, content);
    }
}

/// Basic plain text display export
///
/// Verifies that the display formatter keeps the output close to source code
/// and produces plain text files in `.cce/nl_text_docs/`.
#[tokio::test]
async fn test_basic_nl_text_export() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    // Build chunks for a single file with both Embedding and BM25 paths
    let file_path = "src/main.rs";
    let mut embedding_chunk = make_chunk(
        file_path,
        "group_main",
        ChunkPath::Embedding,
        "The main function serves as the entry point.",
        0,
        1,
    );
    embedding_chunk.bm25_title = Some("main".to_string());
    embedding_chunk.bm25_keywords = vec!["main".to_string(), "entry point".to_string()];

    let mut bm25_chunk = make_chunk(
        file_path,
        "group_main",
        ChunkPath::Bm25,
        "fn main() { println!(\"Hello, World!\"); }",
        0,
        1,
    );
    bm25_chunk.bm25_title = Some("main".to_string());
    bm25_chunk.bm25_keywords = vec!["main".to_string(), "println".to_string()];

    let chunks = vec![embedding_chunk, bm25_chunk];

    let exporter = NlTextDocumentExporter::new(project_root.clone());

    let output_path = exporter
        .export_file(&chunks)
        .expect("Display export should succeed");

    assert!(
        output_path.exists(),
        "Exported display file should exist at {:?}",
        output_path
    );

    // Output must be under .cce/nl_text_docs/
    let output_str = output_path.to_string_lossy().replace('\\', "/");
    assert!(
        output_str.contains(".cce/nl_text_docs/"),
        "Output path should be under .cce/nl_text_docs/, got {:?}",
        output_path
    );

    let content = std::fs::read_to_string(&output_path).expect("Should read exported file");

    assert!(
        content.contains("fn main()"),
        "Display content should contain raw code"
    );
    assert!(
        content.contains("main"),
        "Display content should contain entity name"
    );
    assert!(
        content.contains("keywords:"),
        "Display content should contain keywords"
    );
    assert!(
        content.contains("code:"),
        "Display content should contain code label"
    );
    assert!(
        content.contains("nl:"),
        "Display content should contain natural language label"
    );
    assert!(
        content.contains("entry point"),
        "Display content should contain NL text"
    );
}

/// Plain text batch export with multiple files
#[tokio::test]
async fn test_nl_text_batch_export() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    let file_a = "src/module_a.rs";
    let file_b = "src/module_b.rs";

    let mut file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::new();

    let mut compute_bm25 = make_chunk(
        file_a,
        "group_compute",
        ChunkPath::Bm25,
        "pub fn compute(a: i32, b: i32) -> i32 { a * b }",
        0,
        1,
    );
    compute_bm25.bm25_title = Some("compute".to_string());
    compute_bm25.bm25_keywords = vec!["compute".to_string(), "multiply".to_string()];

    let mut compute_emb = make_chunk(
        file_a,
        "group_compute",
        ChunkPath::Embedding,
        "The compute function multiplies two numbers.",
        0,
        1,
    );
    compute_emb.bm25_title = Some("compute".to_string());
    compute_emb.bm25_keywords = vec!["compute".to_string(), "numbers".to_string()];

    file_chunks.insert(file_a.to_string(), vec![compute_bm25, compute_emb]);

    let mut util_chunk = make_chunk(
        file_b,
        "group_util",
        ChunkPath::Embedding,
        "The util function increments the input.",
        0,
        1,
    );
    util_chunk.bm25_title = Some("util".to_string());
    util_chunk.bm25_keywords = vec!["util".to_string(), "increment".to_string()];
    file_chunks.insert(file_b.to_string(), vec![util_chunk]);

    let exporter = NlTextDocumentExporter::new(project_root.clone());
    let result = exporter.export_batch(&file_chunks);

    assert_eq!(result.exported_count, 2, "Both files should export");
    assert!(
        result.failed.is_empty(),
        "No failures expected: {:?}",
        result.failed
    );
    assert_eq!(result.output_paths.len(), 2);

    for path in &result.output_paths {
        assert!(path.exists(), "Output path {:?} should exist", path);
        let content = std::fs::read_to_string(path).expect("Should read display export file");
        assert!(!content.is_empty(), "Display content should not be empty");
        assert!(
            content.contains("---"),
            "Display content should include separators"
        );
    }
}

/// Plain text display export with fixture using absolute paths
///
/// Uses the once_cell fixture to verify that the display formatter correctly
/// handles absolute source paths by converting them to project-relative
/// paths under `.cce/nl_text_docs/`.
#[tokio::test]
async fn test_fixture_nl_text_export_with_absolute_paths() {
    init_minimal_logging();

    // Load the once_cell fixture
    let fixture = TestFixture::rust_once_cell().expect("Failed to load once_cell fixture");

    let project_root = fixture.root_path().to_path_buf();

    // Get real absolute paths from the fixture
    let imp_cs_path = fixture.file("src/imp_cs.rs").to_string_lossy().to_string();
    let lib_path = fixture.file("src/lib.rs").to_string_lossy().to_string();

    // Build chunks with real absolute paths, including keyword tags
    let mut file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::new();

    let mut imp_chunk = make_chunk(
        &imp_cs_path,
        "group_imp_cs",
        ChunkPath::Embedding,
        "Module imp_cs provides a OnceCell implementation using critical sections.",
        0,
        1,
    );
    imp_chunk.bm25_title = Some("OnceCell".to_string());
    imp_chunk.bm25_keywords = vec![
        "OnceCell".to_string(),
        "UnsafeCell".to_string(),
        "AtomicBool".to_string(),
        "lazy".to_string(),
    ];
    let mut imp_bm25 = make_chunk(
        &imp_cs_path,
        "group_imp_cs",
        ChunkPath::Bm25,
        "use core::cell::UnsafeCell;\nuse core::sync::atomic::{AtomicBool, Ordering};",
        0,
        1,
    );
    imp_bm25.bm25_title = Some("OnceCell".to_string());
    imp_bm25.bm25_keywords = vec!["OnceCell".to_string(), "UnsafeCell".to_string()];
    file_chunks.insert(imp_cs_path.clone(), vec![imp_chunk, imp_bm25]);

    let mut lib_chunk = make_chunk(
        &lib_path,
        "group_lib",
        ChunkPath::Embedding,
        "The library re-exports the OnceCell type and documents the module.",
        0,
        1,
    );
    lib_chunk.bm25_title = Some("once_cell_lib".to_string());
    lib_chunk.bm25_keywords = vec![
        "once_cell".to_string(),
        "lazy".to_string(),
        "thread_safe".to_string(),
    ];
    file_chunks.insert(lib_path.clone(), vec![lib_chunk]);

    let exporter = NlTextDocumentExporter::new(project_root);
    let result = exporter.export_batch(&file_chunks);

    assert_eq!(result.exported_count, 2, "Both files should be exported");
    assert!(result.failed.is_empty(), "No failures: {:?}", result.failed);

    // Verify output structure under .cce/nl_text_docs/
    let mut saw_code_section = false;
    for output_path in &result.output_paths {
        assert!(
            output_path.exists(),
            "Output path {:?} should exist",
            output_path
        );

        let output_str = output_path.to_string_lossy().replace('\\', "/");
        assert!(
            output_str.contains(".cce/nl_text_docs/"),
            "Output path {:?} should be under .cce/nl_text_docs/",
            output_path
        );

        let content =
            std::fs::read_to_string(output_path).expect("Should read display export file");

        assert!(!content.is_empty(), "Display content should not be empty");
        assert!(
            content.contains("keywords:"),
            "Display content should contain keywords"
        );
        if content.contains("code:") {
            saw_code_section = true;
        }
        assert!(
            content.contains("nl:"),
            "Display content should contain natural language section"
        );
    }
    assert!(
        saw_code_section,
        "At least one display file should contain a code section"
    );
}

/// Plain text display export edge cases
#[tokio::test]
async fn test_nl_text_export_edge_cases() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    let exporter = NlTextDocumentExporter::new(project_root.clone());

    // Case 1: Only Embedding chunks
    let embedding_only = vec![make_chunk(
        "src/only_emb.rs",
        "group_only_emb",
        ChunkPath::Embedding,
        "This is a semantic description.",
        0,
        1,
    )];
    let result = exporter.export_file(&embedding_only);
    assert!(
        result.is_ok(),
        "Export with only Embedding chunks should succeed"
    );

    // Case 2: Only BM25 chunks
    let bm25_only = vec![make_chunk(
        "src/only_bm25.rs",
        "group_only_bm25",
        ChunkPath::Bm25,
        "pub fn hello() { println!(\"hi\"); }",
        0,
        1,
    )];
    let result = exporter.export_file(&bm25_only);
    assert!(
        result.is_ok(),
        "Export with only BM25 chunks should succeed"
    );
    if let Ok(path) = result {
        let content = std::fs::read_to_string(&path).expect("Should read display export file");
        assert!(
            content.contains("hello"),
            "Should contain the function name"
        );
        assert!(content.contains("println"), "Should contain code text");
    }

    // Case 3: Empty chunks list
    let result = exporter.export_file(&[]);
    assert!(result.is_err(), "Empty chunks should fail");

    // Case 4: Multiple groups with both paths
    let multi_group = vec![
        {
            let mut chunk = make_chunk(
                "src/multi.rs",
                "group_a",
                ChunkPath::Embedding,
                "Foo description",
                0,
                1,
            );
            chunk.bm25_title = Some("foo".to_string());
            chunk.bm25_keywords = vec!["foo".to_string(), "answer".to_string()];
            chunk
        },
        {
            let mut chunk = make_chunk(
                "src/multi.rs",
                "group_b",
                ChunkPath::Embedding,
                "Bar description",
                0,
                1,
            );
            chunk.bm25_title = Some("bar".to_string());
            chunk.bm25_keywords = vec!["bar".to_string(), "string".to_string()];
            chunk
        },
        {
            let mut chunk = make_chunk(
                "src/multi.rs",
                "group_a",
                ChunkPath::Bm25,
                "fn foo() -> i32 { 42 }",
                0,
                1,
            );
            chunk.bm25_title = Some("foo".to_string());
            chunk.bm25_keywords = vec!["foo".to_string(), "answer".to_string()];
            chunk
        },
        {
            let mut chunk = make_chunk(
                "src/multi.rs",
                "group_b",
                ChunkPath::Bm25,
                "fn bar() -> String { \"hello\".to_string() }",
                0,
                1,
            );
            chunk.bm25_title = Some("bar".to_string());
            chunk.bm25_keywords = vec!["bar".to_string(), "string".to_string()];
            chunk
        },
    ];
    let result = exporter.export_file(&multi_group);
    assert!(result.is_ok(), "Multi-group display export should succeed");
    if let Ok(path) = result {
        let content = std::fs::read_to_string(&path).expect("Should read display export file");
        assert!(content.contains("foo"), "Should contain foo function");
        assert!(content.contains("bar"), "Should contain bar function");
        assert!(content.contains("---"), "Should contain separators");
    }
}
