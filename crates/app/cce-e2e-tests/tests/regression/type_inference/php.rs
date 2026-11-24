//! PHP type inference tests.

use cce_codegraph::index::{FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[test]
fn test_php_phpdoc_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_variable_has_type;

    init_minimal_logging();
    let fixture = TestFixture::php_type_inference_phpdoc().expect("Failed to load php fixture");
    let bindings = snapshot_for(&fixture);

    // Observable today: property annotations and literal types.
    // `new User(...)` constructor metadata is not captured yet.
    assert_variable_has_type(&bindings, "age", "int");
    assert_variable_has_type(&bindings, "greeting", "string");
}

#[tokio::test]
async fn test_php_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::php_type_inference_cross_file()
        .expect("Failed to load php cross_file fixture");
    let orchestrator = run_index(fixture, vec!["php".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[tokio::test]
async fn test_php_overloads_type_inference() {
    init_minimal_logging();
    let fixture =
        TestFixture::php_type_inference_overloads().expect("Failed to load php overloads fixture");
    let orchestrator = run_index(fixture, vec!["php".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}
