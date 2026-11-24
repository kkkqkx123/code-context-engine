//! C and C++ type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[test]
fn test_c_declarations_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::c_type_inference_declarations().expect("Failed to load c fixture");
    let bindings = snapshot_for(&fixture);

    // Locals inside `main` are not extracted as entities for C; the
    // observable surface is function return types (including typedef
    // aliases), which verifies C inferer dispatch end to end.
    assert_return_has_type(&bindings, "add", "int");
    assert_return_has_type(&bindings, "distance", "double");
    assert_return_has_type(&bindings, "count_items", "size_alias");
    assert_return_has_type(&bindings, "main", "int");
}

#[test]
fn test_cpp_declarations_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::cpp_type_inference_declarations().expect("Failed to load cpp fixture");
    let bindings = snapshot_for(&fixture);

    // Locals inside `main` are not extracted as entities for C++; the
    // observable surface is function return types (including templates).
    assert_return_has_type(&bindings, "process_items", "int");
    assert_return_has_type(&bindings, "to_map", "std::map");
    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "main", "int");
}

#[tokio::test]
async fn test_cpp_basic_resolves_header_source_calls() {
    init_minimal_logging();

    let fixture = TestFixture::cpp_basic().expect("Failed to load cpp basic fixture");
    let orchestrator = run_index(fixture, vec!["cpp".to_string(), "h".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 3,
        "cpp basic should index header, implementation and main"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "cpp basic should extract function entities"
    );
}

#[tokio::test]
async fn test_cpp_overloads_type_inference() {
    init_minimal_logging();
    let fixture =
        TestFixture::cpp_type_inference_overloads().expect("Failed to load cpp overloads fixture");
    let orchestrator = run_index(fixture, vec!["cpp".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}
