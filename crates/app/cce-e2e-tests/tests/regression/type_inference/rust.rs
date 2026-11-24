//! Rust type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_rust_generics_type_inference() {
    init_minimal_logging();

    let fixture =
        TestFixture::rust_type_inference_generics().expect("Failed to load rust generics fixture");
    let orchestrator = run_index(fixture, vec!["rs".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
    let _relation_count = relation_index.resolved_relation_count();

    let function_ids = relation_index.function_index();
    assert!(
        !function_ids.is_empty(),
        "Should have extracted function entities"
    );
}

#[tokio::test]
async fn test_rust_control_flow_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::rust_type_inference_control_flow()
        .expect("Failed to load rust control flow fixture");
    let orchestrator = run_index(fixture, vec!["rs".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );

    let function_ids = relation_index.function_index();
    assert!(
        !function_ids.is_empty(),
        "Should have extracted function entities from control flow fixture"
    );
}

#[test]
fn test_rust_generics_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture =
        TestFixture::rust_type_inference_generics().expect("Failed to load rust generics fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "print_pair", "String");
    assert_return_has_type(&bindings, "nested_generic", "Vec<Option<i32>>");
    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrap_in_vec", "Vec<T>");
    assert_return_has_type(&bindings, "swap", "Pair<B, A>");
    assert_variable_has_type(&bindings, "x", "T");
    assert_variable_has_type(&bindings, "a", "T");
}

#[test]
fn test_rust_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::rust_type_inference_control_flow()
        .expect("Failed to load rust control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "handle_option", "String");
    assert_return_has_type(&bindings, "handle_result", "String");
    // Narrowing resolves to payload types, not variant names.
    assert_narrowed_has_type(&bindings, "value", "String");
    assert_narrowed_has_type(&bindings, "val", "i32");
    assert_narrowed_has_type(&bindings, "e", "String");
    assert_narrowed_has_type(&bindings, "name", "String");
    assert_narrowed_has_type(&bindings, "age", "i32");
}

#[test]
fn test_rust_closure_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::rust_type_inference_closure().expect("Failed to load rust closure fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "apply_twice", "i32");
    assert_return_has_type(&bindings, "run", "i32");
}

#[test]
fn test_rust_destructuring_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::rust_type_inference_destructuring()
        .expect("Failed to load rust destructuring fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "destructure_tuple", "String");
    assert_return_has_type(&bindings, "destructure_struct", "i32");
    assert_variable_has_type(&bindings, "pair", "(i32");
    assert_variable_has_type(&bindings, "p", "Point");
    assert_variable_has_type(&bindings, "x", "i32");
    assert_variable_has_type(&bindings, "num", "i32");
    assert_variable_has_type(&bindings, "text", "String");
}

#[test]
fn test_rust_lifetime_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture =
        TestFixture::rust_type_inference_lifetime().expect("Failed to load rust lifetime fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "first", "str");
    assert_return_has_type(&bindings, "collect_refs", "Vec");
    assert_return_has_type(&bindings, "owned_or_borrowed", "String");
    assert_variable_has_type(&bindings, "items", "String");
}

#[test]
fn test_rust_impl_self_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::rust_type_inference_impl_self()
        .expect("Failed to load rust impl_self fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "new", "Self");
    assert_return_has_type(&bindings, "with_count", "Self");
    assert_return_has_type(&bindings, "increment", "Self");
    assert_return_has_type(&bindings, "sum", "Self::Item");
}

#[test]
fn test_rust_reference_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::rust_type_inference_reference()
        .expect("Failed to load rust reference fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "len_of", "usize");
    assert_return_has_type(&bindings, "bump", "i32");
    assert_return_has_type(&bindings, "reborrow", "str");
    assert_variable_has_type(&bindings, "text", "String");
}

#[tokio::test]
async fn test_rust_wildcard_type_inference() {
    init_minimal_logging();
    let fixture =
        TestFixture::rust_type_inference_wildcard().expect("Failed to load rust wildcard fixture");
    let orchestrator = run_index(fixture, vec!["rs".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}
