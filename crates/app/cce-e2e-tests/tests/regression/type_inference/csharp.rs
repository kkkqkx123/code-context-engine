//! C# type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_csharp_generics_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::csharp_type_inference_generics()
        .expect("Failed to load csharp generics fixture");
    let orchestrator = run_index(fixture, vec!["cs".to_string()]).await;

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
        "Should have extracted function entities from C# generics"
    );
}

#[tokio::test]
async fn test_csharp_control_flow_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::csharp_type_inference_control_flow()
        .expect("Failed to load csharp control flow fixture");
    let orchestrator = run_index(fixture, vec!["cs".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
}

#[tokio::test]
async fn test_csharp_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::csharp_type_inference_cross_file()
        .expect("Failed to load csharp cross_file fixture");
    let orchestrator = run_index(fixture, vec!["cs".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[tokio::test]
async fn test_csharp_visibility_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::csharp_type_inference_visibility()
        .expect("Failed to load csharp visibility fixture");
    let orchestrator = run_index(fixture, vec!["cs".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_csharp_generics_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_variable_has_type;

    init_minimal_logging();
    // NB: method return metadata is not captured by the C# extractor yet,
    // so the observable surface is generic property/parameter bindings.
    let fixture = TestFixture::csharp_type_inference_generics()
        .expect("Failed to load csharp generics fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "Value", "T");
    assert_variable_has_type(&bindings, "First", "A");
    assert_variable_has_type(&bindings, "Second", "B");
}

#[test]
fn test_csharp_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_narrowed_has_type;

    init_minimal_logging();
    let fixture = TestFixture::csharp_type_inference_control_flow()
        .expect("Failed to load csharp control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "e", "FormatException");
    assert_narrowed_has_type(&bindings, "e", "InvalidOperationException");
    assert_narrowed_has_type(&bindings, "ex", "ArgumentException");
}

#[test]
fn test_csharp_discriminated_union_snapshot() {
    use cce_e2e_tests::type_inference_assert::{
        assert_narrowed_has_type, assert_variable_has_type,
    };

    init_minimal_logging();
    // NB: method returns are not captured by the C# extractor yet; the
    // observable surface is property type bindings.
    let fixture = TestFixture::csharp_type_inference_discriminated_union()
        .expect("Failed to load csharp discriminated union fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "Radius", "double");
    assert_variable_has_type(&bindings, "Width", "double");
    assert_variable_has_type(&bindings, "Kind", "string");
    // Switch-expression arms bind their designations.
    assert_narrowed_has_type(&bindings, "c", "Circle");
    assert_narrowed_has_type(&bindings, "r", "Rectangle");
    // `shape.Kind == "Circle"` names the Circle case, so only it is kept.
    assert_narrowed_has_type(&bindings, "shape", "Circle");
}

#[test]
fn test_csharp_is_not_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_narrowed_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::csharp_type_inference_is_not().expect("Failed to load csharp is_not fixture");
    let bindings = snapshot_for(&fixture);

    // `value is not null` narrows the declared `string` parameter.
    assert_narrowed_has_type(&bindings, "value", "string");
    // The `obj is not string` guard leaves `obj: string` behind.
    assert_narrowed_has_type(&bindings, "obj", "string");
}
