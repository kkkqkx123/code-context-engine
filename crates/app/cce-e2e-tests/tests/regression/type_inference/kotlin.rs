//! Kotlin type inference tests.

use cce_codegraph::index::{FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[test]
fn test_kotlin_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_narrowed_has_type;

    init_minimal_logging();
    // NB: expression-body functions and `val` metadata are not captured by
    // the Kotlin extractor yet, so `generics.kt` is index-smoke only (see
    // `test_new_language_fixtures_index`). Narrowing via `is`/`when` works.
    let fixture = TestFixture::kotlin_type_inference_control_flow()
        .expect("Failed to load kotlin control flow fixture");
    let bindings = snapshot_for(&fixture);
    assert_narrowed_has_type(&bindings, "value", "String");
    assert_narrowed_has_type(&bindings, "result", "Result.Success");
}

#[tokio::test]
async fn test_kotlin_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_cross_file()
        .expect("Failed to load kotlin cross_file fixture");
    let orchestrator = run_index(fixture, vec!["kt".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[test]
fn test_kotlin_overloads_resolve_by_argument_shapes() {
    use cce_e2e_tests::type_inference_assert::assert_variable_has_type;

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_overloads()
        .expect("Failed to load kotlin overloads fixture");
    let bindings = snapshot_for(&fixture);

    // Same-file overloads dispatch on call-site shapes, not on the
    // most recently declared overload.
    assert_variable_has_type(&bindings, "ints", "Int");
    assert_variable_has_type(&bindings, "strs", "String");
    assert_variable_has_type(&bindings, "mixed", "String");
}

#[tokio::test]
async fn test_kotlin_overloads_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_overloads()
        .expect("Failed to load kotlin overloads fixture");
    let orchestrator = run_index(fixture, vec!["kt".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[tokio::test]
async fn test_kotlin_visibility_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_visibility()
        .expect("Failed to load kotlin visibility fixture");
    let orchestrator = run_index(fixture, vec!["kt".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_kotlin_generics_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_generics()
        .expect("Failed to load kotlin generics fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrapInList", "List<T>");
    assert_return_has_type(&bindings, "toMap", "Map<K, V>");
    assert_return_has_type(&bindings, "maxOf", "T");
    assert_variable_has_type(&bindings, "id", "String");
    assert_variable_has_type(&bindings, "biggest", "Int");
}

#[test]
fn test_kotlin_lambda_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture =
        TestFixture::kotlin_type_inference_lambda().expect("Failed to load kotlin lambda fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "applyTwice", "Int");
    assert_variable_has_type(&bindings, "toLabel", "Int");
}

#[test]
fn test_kotlin_discriminated_union_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_discriminated_union()
        .expect("Failed to load kotlin discriminated union fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "result", "Result.Success");
    assert_return_has_type(&bindings, "describe", "String");
}

#[test]
fn test_kotlin_null_safety_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_null_safety()
        .expect("Failed to load kotlin null safety fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "lengthOrDefault", "Int");
    assert_return_has_type(&bindings, "shout", "String");
    assert_return_has_type(&bindings, "forced", "Int");
}

#[test]
fn test_kotlin_scope_functions_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_scope_functions()
        .expect("Failed to load kotlin scope functions fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "applied", "StringBuilder");
    assert_return_has_type(&bindings, "parseNumber", "Int?");
}

#[test]
fn test_kotlin_negated_is_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::kotlin_type_inference_negated_is()
        .expect("Failed to load kotlin negated is fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "String");
    // Both `when` arms narrow, including the second one.
    assert_narrowed_has_type(&bindings, "value", "Int");
    assert_return_has_type(&bindings, "handleNegated", "String");
    assert_return_has_type(&bindings, "classifyWhen", "String");
}
