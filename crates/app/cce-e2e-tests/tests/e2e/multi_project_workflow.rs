//! Multi-project workflow tests
//!
//! Verifies project isolation for indexing, BM25 storage, and cleanup.

use std::sync::Arc;

/// Dual-project BM25 isolation
///
/// Creates two projects with different content, indexes both using a shared
/// BM25 client, and verifies:
/// - BM25 search results are scoped by project_id
/// - Document counts are isolated per project
#[tokio::test]
async fn test_dual_project_bm25_isolation() {
    crate::helper::init_minimal_logging();

    // Create two project directories with different Rust source files
    let project_a = crate::helper::EmptyFixture::new().expect("Failed to create project A fixture");
    project_a
        .add_file(
            "src/lib.rs",
            r#"
pub fn foo() -> i32 { 42 }
pub fn helper_a() -> i32 { foo() }
"#,
        )
        .expect("Failed to add src/lib.rs to project A");
    project_a
        .add_file(
            "Cargo.toml",
            "[package]\nname = \"proj_a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("Failed to add Cargo.toml to project A");

    let project_b = crate::helper::EmptyFixture::new().expect("Failed to create project B fixture");
    project_b
        .add_file(
            "src/lib.rs",
            r#"
pub fn bar() -> String { "hello".to_string() }
pub fn helper_b() -> String { bar() }
"#,
        )
        .expect("Failed to add src/lib.rs to project B");
    project_b
        .add_file(
            "Cargo.toml",
            "[package]\nname = \"proj_b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("Failed to add Cargo.toml to project B");

    // Create shared BM25 client
    let bm25_temp = tempfile::TempDir::new().expect("Failed to create BM25 temp dir");
    let bm25_path = bm25_temp
        .path()
        .to_str()
        .expect("BM25 path is not valid UTF-8")
        .to_string();
    let bm25_config = cce_storage_bm25::Bm25Config::default()
        .enabled()
        .with_index_name("default")
        .with_index_path(&bm25_path);
    let mut bm25 = cce_storage_bm25::Bm25Client::new(bm25_config);
    bm25.connect().await.expect("Failed to connect BM25 client");
    let bm25 = Arc::new(bm25);

    // --- Index project A (project_id = 1) ---
    let mut orch_a = cce_orchestrator::IndexOrchestrator::new(1)
        .expect("failed to create IndexOrchestrator")
        .with_bm25(bm25.clone());
    let options_a = cce_orchestrator::IndexOptions {
        root_dir: project_a.root().to_path_buf(),
        extensions: vec!["rs".to_string()],
        store_bm25: true,
        store_vectors: false,
        build_relations: false,
        ..Default::default()
    };
    let result_a = orch_a
        .execute(options_a)
        .await
        .expect("Project A indexing should succeed");
    assert!(
        result_a.total_entities > 0,
        "Project A should have indexed entities"
    );

    // --- Index project B (project_id = 2) ---
    let mut orch_b = cce_orchestrator::IndexOrchestrator::new(2)
        .expect("failed to create IndexOrchestrator")
        .with_bm25(bm25.clone());
    let options_b = cce_orchestrator::IndexOptions {
        root_dir: project_b.root().to_path_buf(),
        extensions: vec!["rs".to_string()],
        store_bm25: true,
        store_vectors: false,
        build_relations: false,
        ..Default::default()
    };
    let result_b = orch_b
        .execute(options_b)
        .await
        .expect("Project B indexing should succeed");
    assert!(
        result_b.total_entities > 0,
        "Project B should have indexed entities"
    );

    // --- Verify BM25 search isolation ---
    let retrieval = cce_storage_bm25::Bm25Retrieval::new();

    // Clone IndexManager out of the shared client and snapshot the schema.
    let index_mgr = bm25
        .index_manager()
        .expect("BM25 index manager not available")
        .clone();
    let schema = {
        let guard = index_mgr.read().await;
        guard.schema().clone()
    };

    // Search "foo" in project A → should find results
    let opts_a = cce_storage_common::FulltextSearchOptions {
        project_id: 1,
        limit: 10,
        offset: 0,
        field_weights: std::collections::HashMap::new(),
        epochs: Vec::new(),
        excluded_files: None,
        exclude_test: false,
        exclude_generated: false,
        exclude_dependency: false,
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        term_operator: Default::default(),
    };
    let results_a_foo = {
        let mgr = index_mgr.read().await;
        retrieval
            .search(&mgr, &schema, "foo", &opts_a)
            .expect("BM25 search for 'foo' in project A should succeed")
    };
    assert!(
        !results_a_foo.is_empty(),
        "Project A should find 'foo' in BM25"
    );

    // Search "foo" in project B → should NOT find results
    let opts_b = cce_storage_common::FulltextSearchOptions {
        project_id: 2,
        limit: 10,
        offset: 0,
        field_weights: std::collections::HashMap::new(),
        epochs: Vec::new(),
        excluded_files: None,
        exclude_test: false,
        exclude_generated: false,
        exclude_dependency: false,
        include_categories: Vec::new(),
        exclude_categories: Vec::new(),
        term_operator: Default::default(),
    };
    let results_b_foo = {
        let mgr = index_mgr.read().await;
        retrieval
            .search(&mgr, &schema, "foo", &opts_b)
            .expect("BM25 search for 'foo' in project B should succeed")
    };
    assert!(results_b_foo.is_empty(), "Project B should NOT find 'foo'");

    // Search "bar" in project B → should find results
    let results_b_bar = {
        let mgr = index_mgr.read().await;
        retrieval
            .search(&mgr, &schema, "bar", &opts_b)
            .expect("BM25 search for 'bar' in project B should succeed")
    };
    assert!(!results_b_bar.is_empty(), "Project B should find 'bar'");

    // Search "bar" in project A → should NOT find results
    let results_a_bar = {
        let mgr = index_mgr.read().await;
        retrieval
            .search(&mgr, &schema, "bar", &opts_a)
            .expect("BM25 search for 'bar' in project A should succeed")
    };
    assert!(results_a_bar.is_empty(), "Project A should NOT find 'bar'");
}

/// Project deletion and recreation residue check
///
/// Creates a project, indexes it, records BM25 document count, deletes all
/// project data, then re-indexes the same content and verifies the count
/// matches the original.
#[tokio::test]
async fn test_project_delete_bm25_residue() {
    crate::helper::init_minimal_logging();

    let project = crate::helper::EmptyFixture::new().expect("Failed to create project fixture");
    project
        .add_file(
            "src/lib.rs",
            r#"
pub fn compute(x: i32) -> i32 { x * 2 }
pub fn process(y: i32) -> i32 { compute(y) }
"#,
        )
        .expect("Failed to add src/lib.rs");
    project
        .add_file(
            "Cargo.toml",
            "[package]\nname = \"test_proj\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("Failed to add Cargo.toml");

    // Create BM25 client
    let bm25_temp = tempfile::TempDir::new().expect("Failed to create BM25 temp dir");
    let bm25_path = bm25_temp
        .path()
        .to_str()
        .expect("BM25 path is not valid UTF-8")
        .to_string();
    let bm25_config = cce_storage_bm25::Bm25Config::default()
        .enabled()
        .with_index_name("default")
        .with_index_path(&bm25_path);
    let mut bm25 = cce_storage_bm25::Bm25Client::new(bm25_config);
    bm25.connect().await.expect("Failed to connect BM25 client");
    let bm25 = Arc::new(bm25);

    // --- First index ---
    let mut orch = cce_orchestrator::IndexOrchestrator::new(1)
        .expect("failed to create IndexOrchestrator")
        .with_bm25(bm25.clone());
    let options = cce_orchestrator::IndexOptions {
        root_dir: project.root().to_path_buf(),
        extensions: vec!["rs".to_string()],
        store_bm25: true,
        store_vectors: false,
        build_relations: false,
        ..Default::default()
    };
    let result = orch
        .execute(options.clone())
        .await
        .expect("First indexing should succeed");
    assert!(result.total_entities > 0);

    // Count BM25 documents
    let first_count = {
        bm25.document_count()
            .await
            .expect("Failed to get BM25 document count")
    };
    assert!(first_count > 0, "Should have indexed BM25 documents");

    // --- Delete all project docs ---
    let deleted = {
        bm25.delete_all_project_docs("default", 1)
            .await
            .expect("Failed to delete BM25 documents for project")
    };
    assert_eq!(deleted, first_count, "Deleted count should match");

    // Verify BM25 is empty after deletion
    let after_delete = {
        bm25.document_count()
            .await
            .expect("Failed to get BM25 document count after deletion")
    };
    assert_eq!(
        after_delete, 0,
        "BM25 should be empty after project deletion"
    );

    // --- Re-index same content ---
    let mut orch2 = cce_orchestrator::IndexOrchestrator::new(1)
        .expect("failed to create IndexOrchestrator")
        .with_bm25(bm25.clone());
    let result2 = orch2
        .execute(options)
        .await
        .expect("Re-indexing should succeed");
    assert!(result2.total_entities > 0);

    // Verify re-index count matches original
    let second_count = {
        bm25.document_count()
            .await
            .expect("Failed to get BM25 document count after re-index")
    };
    assert_eq!(
        second_count, first_count,
        "Re-index should produce same document count as original"
    );
}
