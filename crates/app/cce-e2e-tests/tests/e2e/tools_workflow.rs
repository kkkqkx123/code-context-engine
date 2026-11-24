//! Tools workflow tests
//!
//! Tests for standalone tools: AST diagnosis, compression retrieval,
//! keyword search, and symbol lookup tools.

use cce_orchestrator::{AstDiagnosis, CompressionRequest, CompressionRetrieval, DiagnosisRequest};
use cce_types::language::Language;

use crate::helper::init_minimal_logging;

// ---------------------------------------------------------------------------
// AST Diagnosis
// ---------------------------------------------------------------------------

/// AST diagnosis standalone tool
#[test]
fn test_ast_diagnosis_valid_code() {
    init_minimal_logging();

    let mut diagnosis = AstDiagnosis::new();

    let request = DiagnosisRequest {
        code: "fn main() {\n    println!(\"Hello\");\n}\n".to_string(),
        language: Some(Language::Rust),
        file_name: None,
        include_ast: false,
    };

    let response = diagnosis
        .diagnose(request)
        .expect("Diagnosis should succeed");

    assert!(
        response.is_valid,
        "Valid Rust code should have no diagnostics"
    );
    assert!(response.diagnostics.is_empty());
    assert_eq!(response.language, "Rust");
}

#[test]
fn test_ast_diagnosis_invalid_code() {
    init_minimal_logging();

    let mut diagnosis = AstDiagnosis::new();

    let request = DiagnosisRequest {
        code: "fn main(\n    println!(\"Hello\");\n}".to_string(),
        language: Some(Language::Rust),
        file_name: None,
        include_ast: false,
    };

    let response = diagnosis
        .diagnose(request)
        .expect("Diagnosis should succeed");

    assert!(
        !response.is_valid,
        "Invalid Rust code should have diagnostics"
    );
    assert!(
        !response.diagnostics.is_empty(),
        "Should have at least one diagnostic"
    );
}

#[test]
fn test_ast_diagnosis_unsupported_language() {
    init_minimal_logging();

    let mut diagnosis = AstDiagnosis::new();

    let request = DiagnosisRequest {
        code: "some code".to_string(),
        language: None,
        file_name: Some("unknown.xyz".to_string()),
        include_ast: false,
    };

    let result = diagnosis.diagnose(request);
    assert!(
        result.is_err(),
        "Unsupported language should return an error"
    );
}

// ---------------------------------------------------------------------------
// Compression Retrieval
// ---------------------------------------------------------------------------

/// Compression retrieval standalone
#[tokio::test]
async fn test_compression_retrieval_standalone() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let file_path = tmp_dir.path().join("test.rs");

    let content = "fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n";
    tokio::fs::write(&file_path, content)
        .await
        .expect("Failed to write test file");

    let compression = CompressionRetrieval::new();

    let request = CompressionRequest {
        file_path: file_path.to_string_lossy().to_string(),
        include_entities: false,
        include_groups: false,
    };

    let response = compression
        .compress(request)
        .await
        .expect("Compression should succeed");

    assert_eq!(response.language, "Rust");
    assert!(!response.from_cache);
    assert!(
        !response.semantic_text.is_empty(),
        "Semantic text should not be empty"
    );
    assert!(
        response.semantic_text.contains("greet"),
        "Semantic text should mention 'greet'"
    );
}

#[tokio::test]
async fn test_compression_retrieval_nonexistent_file() {
    init_minimal_logging();

    let compression = CompressionRetrieval::new();

    let request = CompressionRequest {
        file_path: "/nonexistent/path/file.rs".to_string(),
        include_entities: false,
        include_groups: false,
    };

    let result = compression.compress(request).await;
    assert!(
        result.is_err(),
        "Compression of nonexistent file should fail"
    );
}

#[tokio::test]
async fn test_batch_compression() {
    init_minimal_logging();

    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    let file1 = tmp_dir.path().join("main.rs");
    let file2 = tmp_dir.path().join("lib.rs");

    tokio::fs::write(&file1, "fn hello() {}\n")
        .await
        .expect("write");
    tokio::fs::write(&file2, "fn world() {}\n")
        .await
        .expect("write");

    let compression = CompressionRetrieval::new();

    let request = cce_orchestrator::BatchCompressionRequest {
        file_paths: vec![
            file1.to_string_lossy().to_string(),
            file2.to_string_lossy().to_string(),
        ],
        include_entities: false,
        include_groups: false,
        max_concurrency: 4,
    };

    let response = compression.compress_batch(request).await;

    assert_eq!(response.successes.len(), 2);
    assert!(response.failures.is_empty());
}

// ---------------------------------------------------------------------------
// Symbol Lookup Tools (require RelationIndex)
//
// The FindReferencesTool, GotoDefinitionTool, and GetSymbolsTool all
// require an Arc<RelationIndex> populated with indexed entities.
// These are tested via the full E2E index workflow in query_workflow.rs
// and index_workflow.rs, which exercise the underlying relation index.
//
// Below we provide basic construction tests to verify the tool APIs.
// ---------------------------------------------------------------------------

/// Goto definition tool construction
#[test]
fn test_goto_definition_tool_construction() {
    use cce_codegraph::LayeredSnapshotIndex;
    use cce_orchestrator::GotoDefinitionTool;
    use std::sync::Arc;

    let index = Arc::new(LayeredSnapshotIndex::empty());
    let tool = GotoDefinitionTool::new(index, 1);
    // Construction succeeds; full query requires indexed data
    let _ = tool;
}

/// Find references tool construction
#[test]
fn test_find_references_tool_construction() {
    use cce_codegraph::LayeredSnapshotIndex;
    use cce_orchestrator::FindReferencesTool;
    use std::sync::Arc;

    let index = Arc::new(LayeredSnapshotIndex::empty());
    let tool = FindReferencesTool::new(index, 1);
    let _ = tool;
}

/// Get symbols tool construction
#[test]
fn test_get_symbols_tool_construction() {
    use cce_codegraph::LayeredSnapshotIndex;
    use cce_orchestrator::GetSymbolsTool;
    use std::sync::Arc;

    let index = Arc::new(LayeredSnapshotIndex::empty());
    let tool = GetSymbolsTool::new(index);
    let _ = tool;
}
