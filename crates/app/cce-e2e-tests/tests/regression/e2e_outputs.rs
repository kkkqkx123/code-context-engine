//! E2E output helper tests
//!
//! This file runs output helper tests with complete output and summary.

use crate::helper::{
    OutputCategory, OutputConfig, OutputManager, OutputReporter, OutputSerializer,
    SerializableIndexResult,
};

/// Test output manager functionality
#[test]
fn test_output_manager_basic() {
    // Create output manager
    let manager = OutputManager::builder()
        .category(OutputCategory::Index)
        .scenario("test_basic")
        .build();

    // Get output directory
    let output_dir = manager.output_dir();

    // Verify path structure (handle both forward and back slashes)
    let path_str = output_dir.to_string_lossy();
    assert!(
        path_str.contains("outputs"),
        "Path should contain outputs, got: {}",
        path_str
    );
    assert!(
        path_str.contains("index"),
        "Path should contain 'index', got: {}",
        path_str
    );
    assert!(
        path_str.contains("test_basic"),
        "Path should contain 'test_basic', got: {}",
        path_str
    );

    // Write test file
    let result = manager.write("test.txt", "Hello, World!");
    assert!(result.is_ok(), "Failed to write test file");

    let path = result.expect("Failed to get path");
    assert!(path.exists(), "Output file does not exist");

    // Read and verify content
    let content = std::fs::read_to_string(&path).expect("Failed to read file");
    assert_eq!(content, "Hello, World!");
}

/// Test output reporter functionality
#[test]
fn test_output_reporter_basic() {
    // Create reporter
    let reporter = OutputReporter::new(
        OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .scenario("test_reporter")
            .build(),
        OutputSerializer::json(),
    );

    // Get output directory
    let output_dir = reporter.manager.output_dir();

    // Verify path structure (handle both forward and back slashes)
    let path_str = output_dir.to_string_lossy();
    assert!(
        path_str.contains("outputs"),
        "Path should contain outputs, got: {}",
        path_str
    );
    assert!(
        path_str.contains("scenarios"),
        "Path should contain 'scenarios', got: {}",
        path_str
    );
    assert!(
        path_str.contains("test_reporter"),
        "Path should contain 'test_reporter', got: {}",
        path_str
    );

    // Write custom file
    let result = reporter.write_custom("custom.txt", "Custom content");
    assert!(result.is_ok(), "Failed to write custom file");

    let path = result.expect("Failed to get path");
    assert!(path.exists(), "Custom file does not exist");
}

/// Test output configuration
#[test]
fn test_output_config() {
    // Test default config
    let config = OutputConfig::default();
    let path_str = config.base_dir.to_string_lossy();
    assert!(
        path_str.contains("outputs"),
        "Path should contain outputs, got: {}",
        path_str
    );
    assert!(!config.use_timestamp);
    assert!(config.overwrite);

    // Test custom config
    let custom_config = OutputConfig {
        overwrite: false,
        ..Default::default()
    };

    assert!(!custom_config.overwrite);
}

/// Test JSON serialization
#[test]
fn test_json_serialization() {
    use std::collections::HashMap;

    // Create serializable result
    let mut result = SerializableIndexResult {
        test_name: "test_json".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        total_files: 3,
        indexed_files: 3,
        failed_files: 0,
        total_entities: 5,
        total_relations: 2,
        total_vectors: 5,
        total_tokens: 100,
        errors: vec![],
        elapsed_ms: 100,
        success_rate: 1.0,
        metadata: HashMap::new(),
    };

    result.metadata.insert(
        "description".to_string(),
        "Test JSON serialization".to_string(),
    );
    result
        .metadata
        .insert("category".to_string(), "test".to_string());

    // Serialize to JSON
    let serializer = OutputSerializer::json();
    let json = serializer
        .serialize_index_result(&result)
        .expect("Failed to serialize");

    // Verify JSON contains expected fields
    assert!(json.contains("test_json"), "JSON should contain test name");
    assert!(
        json.contains("total_files"),
        "JSON should contain total_files"
    );
    assert!(
        json.contains("total_entities"),
        "JSON should contain total_entities"
    );
}

/// Test Markdown serialization
#[test]
fn test_markdown_serialization() {
    use std::collections::HashMap;

    // Create serializable result
    let mut result = SerializableIndexResult {
        test_name: "test_markdown".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        total_files: 3,
        indexed_files: 3,
        failed_files: 0,
        total_entities: 5,
        total_relations: 2,
        total_vectors: 5,
        total_tokens: 100,
        errors: vec![],
        elapsed_ms: 100,
        success_rate: 1.0,
        metadata: HashMap::new(),
    };

    result.metadata.insert(
        "description".to_string(),
        "Test Markdown serialization".to_string(),
    );

    // Serialize to Markdown
    let serializer = OutputSerializer::markdown();
    let md = serializer
        .serialize_index_result(&result)
        .expect("Failed to serialize");

    // Verify Markdown contains expected sections
    assert!(
        md.contains("test_markdown"),
        "Markdown should contain test name"
    );
    assert!(
        md.contains("Files"),
        "Markdown should contain Files section"
    );
    assert!(
        md.contains("Entities"),
        "Markdown should contain Entities section"
    );
}
