//! Dart type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_dart_basic_resolves_calls() {
    init_minimal_logging();

    let fixture = TestFixture::dart_basic().expect("Failed to load dart basic fixture");
    let orchestrator = run_index(fixture, vec!["dart".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 2,
        "dart basic should index lib and bin entrypoint"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "dart basic should extract function entities"
    );
}

#[tokio::test]
async fn test_dart_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_cross_file()
        .expect("Failed to load dart cross_file fixture");
    let orchestrator = run_index(fixture, vec!["dart".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[tokio::test]
async fn test_dart_overloads_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_overloads()
        .expect("Failed to load dart overloads fixture");
    let orchestrator = run_index(fixture, vec!["dart".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_dart_declarations_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_declarations()
        .expect("Failed to load dart declarations fixture");
    let bindings = snapshot_for(&fixture);

    // `var count = 42` infers Dart `int` (the `num` supertype was an
    // artifact of the language-agnostic `number` vocabulary).
    assert_variable_has_type(&bindings, "count", "int");
    assert_variable_has_type(&bindings, "explicit", "String");
    assert_variable_has_type(&bindings, "name", "String");
    assert_return_has_type(&bindings, "greet", "String");
    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrapInList", "List<T>");
}

#[test]
fn test_dart_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_control_flow()
        .expect("Failed to load dart control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "String");
    assert_narrowed_has_type(&bindings, "value", "int");
    assert_narrowed_has_type(&bindings, "result", "Success");
    assert_return_has_type(&bindings, "handleIs", "String");
}

#[test]
fn test_dart_discriminated_union_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_discriminated_union()
        .expect("Failed to load dart discriminated union fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "result", "Success");
    assert_narrowed_has_type(&bindings, "result", "Failure");
    assert_return_has_type(&bindings, "describe", "String");
}

#[test]
fn test_dart_is_negated_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_is_negated()
        .expect("Failed to load dart is_negated fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "handleIsNegated", "String");
    assert_return_has_type(&bindings, "handleNotNull", "String");
    assert_return_has_type(&bindings, "handleNullCheck", "String");
    // `value != null` narrows the declared `String?` parameter (non-null
    // branch). `is!` on `String?` leaves `Null` on the then side and
    // `String` past the guard; `is!` on plain `Object` leaves the positive
    // type past the guard.
    assert_narrowed_has_type(&bindings, "value", "String");
    assert_narrowed_has_type(&bindings, "value", "Null");
}

#[test]
fn test_dart_control_positions_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::dart_type_inference_control_positions()
        .expect("Failed to load dart control positions fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "String");
    assert_narrowed_has_type(&bindings, "value", "int");
    assert_return_has_type(&bindings, "handleWhile", "String");
    assert_return_has_type(&bindings, "handleElseIf", "String");
}
