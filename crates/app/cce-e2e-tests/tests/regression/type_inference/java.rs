//! Java type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_java_generics_type_inference() {
    init_minimal_logging();

    let fixture =
        TestFixture::java_type_inference_generics().expect("Failed to load java generics fixture");
    let orchestrator = run_index(fixture, vec!["java".to_string()]).await;

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
        "Should have extracted function entities from Java generics"
    );
}

#[tokio::test]
async fn test_java_control_flow_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::java_type_inference_control_flow()
        .expect("Failed to load java control flow fixture");
    let orchestrator = run_index(fixture, vec!["java".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
}

#[tokio::test]
async fn test_java_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_cross_file()
        .expect("Failed to load java cross_file fixture");
    let orchestrator = run_index(fixture, vec!["java".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[test]
fn test_java_generics_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::java_type_inference_generics().expect("Failed to load java generics fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "printPair", "String");
    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrapInList", "List<T>");
    assert_return_has_type(&bindings, "toMap", "Map<K, V>");
    assert_return_has_type(&bindings, "swap", "Pair<B, A>");
    assert_return_has_type(&bindings, "max", "T");
}

#[test]
fn test_java_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_control_flow()
        .expect("Failed to load java control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "e", "NumberFormatException");
    assert_return_has_type(&bindings, "handleInstanceof", "String");
    assert_return_has_type(&bindings, "handleTryCatch", "String");
}

#[test]
fn test_java_overloads_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_overloads()
        .expect("Failed to load java overloads fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "combine", "int");
    assert_return_has_type(&bindings, "combine", "String");
    assert_return_has_type(&bindings, "run", "String");
}

#[test]
fn test_java_lambda_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::java_type_inference_lambda().expect("Failed to load java lambda fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "applyTwice", "int");
}

#[test]
fn test_java_discriminated_union_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_discriminated_union()
        .expect("Failed to load java discriminated union fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "area", "double");
    assert_return_has_type(&bindings, "describe", "String");
}

#[test]
fn test_java_pattern_matching_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_pattern_matching()
        .expect("Failed to load java pattern matching fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "describe", "String");
    assert_return_has_type(&bindings, "matchShape", "String");
}

#[test]
fn test_java_negated_checks_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_negated_checks()
        .expect("Failed to load java negated checks fixture");
    let bindings = snapshot_for(&fixture);

    // `value != null` narrows the declared `String` parameter.
    assert_narrowed_has_type(&bindings, "value", "String");
    // The `!(obj instanceof String)` guard leaves `obj: String` behind.
    assert_narrowed_has_type(&bindings, "obj", "String");
    assert_return_has_type(&bindings, "handleNotInstance", "String");
    assert_return_has_type(&bindings, "handleNull", "String");
}

#[test]
fn test_java_control_positions_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::java_type_inference_control_positions()
        .expect("Failed to load java control positions fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "handleWhile", "String");
    assert_return_has_type(&bindings, "handleElseIf", "String");
}

#[test]
fn test_java_var_inference_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_variable_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::java_type_inference_var_inference().expect("Failed to load java var fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "names", "ArrayList<String>");
    assert_variable_has_type(&bindings, "scores", "HashMap<String, Integer>");
    // `names.get(0)` resolves to the element type, not the receiver.
    assert_variable_has_type(&bindings, "first", "String");
    // `first + first` follows string-concatenation semantics.
    assert_variable_has_type(&bindings, "doubled", "String");
}
