//! Python type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_python_type_hints_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::python_type_inference_type_hints()
        .expect("Failed to load python type hints fixture");
    let orchestrator = run_index(fixture, vec!["py".to_string()]).await;

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
        "Should have extracted function entities from Python type hints"
    );
}

#[tokio::test]
async fn test_python_control_flow_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::python_type_inference_control_flow()
        .expect("Failed to load python control flow fixture");
    let orchestrator = run_index(fixture, vec!["py".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
}

#[test]
fn test_python_type_hints_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_type_hints()
        .expect("Failed to load python type hints fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "result", "Dict[str, int]");
    assert_variable_has_type(&bindings, "container", "Container");
    assert_return_has_type(&bindings, "process_items", "Dict[str, int]");
    assert_return_has_type(&bindings, "safe_divide", "Optional[float]");
    assert_return_has_type(&bindings, "wrap_value", "List[str]");
    assert_return_has_type(&bindings, "process_pair", "str");
}

#[test]
fn test_python_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_control_flow()
        .expect("Failed to load python control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "str");
    assert_return_has_type(&bindings, "handle_isinstance", "str");
    assert_return_has_type(&bindings, "handle_optional", "str");
}

#[test]
fn test_python_visibility_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_visibility()
        .expect("Failed to load python visibility fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "obj", "PublicClass");

    assert_return_has_type(&bindings, "public_func", "str");
    assert_return_has_type(&bindings, "_private_func", "str");
    assert_return_has_type(&bindings, "__dunder_func__", "str");
    assert_return_has_type(&bindings, "public_method", "str");
    assert_return_has_type(&bindings, "_private_method", "str");
}

#[test]
fn test_python_lambda_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_no_binding, assert_return_has_type};

    init_minimal_logging();
    let fixture =
        TestFixture::python_type_inference_lambda().expect("Failed to load python lambda fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "apply_twice", "int");
    // Unannotated lambdas stay conservative: no binding is inferred.
    assert_no_binding(&bindings, "formatter");
}

#[test]
fn test_python_discriminated_union_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_discriminated_union()
        .expect("Failed to load python discriminated union fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "shape", "Circle");
    // Post-guard code observes the complement; the vacuous tuple check
    // (`Union[Circle, Rectangle]` itself) emits no identity row.
    assert_narrowed_has_type(&bindings, "shape", "Rectangle");
    assert_return_has_type(&bindings, "area", "float");
    assert_return_has_type(&bindings, "describe", "str");
}

#[test]
fn test_python_destructuring_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_destructuring()
        .expect("Failed to load python destructuring fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "split_pair", "str");
    assert_return_has_type(&bindings, "lookup_user", "str");
    assert_return_has_type(&bindings, "handle_point", "str");
}

#[test]
fn test_python_negated_checks_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_negated_checks()
        .expect("Failed to load python negated checks fixture");
    let bindings = snapshot_for(&fixture);

    // Negated guards narrow against the declared annotation.
    // `not isinstance(value, str)` on `Union[str, int]` leaves `int`;
    // `value is not None` on `Optional[str]` leaves `str`;
    // `not value` on `Optional[str]` leaves `None`.
    assert_narrowed_has_type(&bindings, "value", "int");
    assert_narrowed_has_type(&bindings, "value", "str");
    assert_narrowed_has_type(&bindings, "value", "None");
    assert_return_has_type(&bindings, "handle_not_str", "str");
    assert_return_has_type(&bindings, "handle_is_not_none", "str");
}

#[test]
fn test_python_cross_file_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::python_type_inference_cross_file().expect("Failed to load cross-file fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "load_user", "User");
    assert_return_has_type(&bindings, "render_greeting", "str");
}

#[tokio::test]
async fn test_python_visibility_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::python_type_inference_visibility()
        .expect("Failed to load python visibility fixture");
    let orchestrator = run_index(fixture, vec!["py".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}
