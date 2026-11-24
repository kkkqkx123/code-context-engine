//! Go type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_go_interfaces_type_inference() {
    init_minimal_logging();

    let fixture =
        TestFixture::go_type_inference_interfaces().expect("Failed to load go interfaces fixture");
    let orchestrator = run_index(fixture, vec!["go".to_string()]).await;

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
        "Should have extracted function entities from Go interfaces"
    );
}

#[tokio::test]
async fn test_go_control_flow_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::go_type_inference_control_flow()
        .expect("Failed to load go control flow fixture");
    let orchestrator = run_index(fixture, vec!["go".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
}

#[tokio::test]
async fn test_go_cross_file_type_inference() {
    init_minimal_logging();
    let fixture =
        TestFixture::go_type_inference_cross_file().expect("Failed to load go cross_file fixture");
    let orchestrator = run_index(fixture, vec!["go".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[tokio::test]
async fn test_go_visibility_type_inference() {
    init_minimal_logging();
    let fixture =
        TestFixture::go_type_inference_visibility().expect("Failed to load go visibility fixture");
    let orchestrator = run_index(fixture, vec!["go".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_go_interfaces_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture =
        TestFixture::go_type_inference_interfaces().expect("Failed to load go interfaces fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "processStringer", "string");
    assert_return_has_type(&bindings, "processNamed", "string");
    assert_return_has_type(&bindings, "first", "T");
    assert_return_has_type(&bindings, "wrapInSlice", "T[]");
    assert_variable_has_type(&bindings, "s", "Stringer");
    assert_variable_has_type(&bindings, "n", "Named");
    // Call-site generic substitution through composite literals.
    assert_variable_has_type(&bindings, "wrapped", "string[]");
    assert_variable_has_type(&bindings, "firstItem", "int");
}

#[tokio::test]
async fn test_go_interfaces_method_set_satisfaction() {
    use cce_codegraph::index::HierarchyQueryOps;
    use cce_types::entity::EntityKind;

    init_minimal_logging();

    let fixture =
        TestFixture::go_type_inference_interfaces().expect("Failed to load go interfaces fixture");
    let orchestrator = run_index(fixture, vec!["go".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    let entity_id_by_kind = |name: &str, kind: EntityKind| {
        relation_index
            .get_function_ids_by_name(name)
            .into_iter()
            .find(|id| {
                relation_index
                    .get_function_by_entity_id(*id)
                    .is_some_and(|entity| entity.kind == kind)
            })
            .unwrap_or_else(|| panic!("Should extract {kind:?} entity named {name}"))
    };
    let person = entity_id_by_kind("Person", EntityKind::Struct);
    let stringer = entity_id_by_kind("Stringer", EntityKind::Interface);
    let named = entity_id_by_kind("Named", EntityKind::Interface);
    let formatter = entity_id_by_kind("Formatter", EntityKind::Interface);

    // Person implements all three interfaces (Formatter's direct method
    // set is {Format}; the embedded Stringer edge is covered separately).
    for iface in [stringer, named, formatter] {
        assert!(
            relation_index
                .get_implementing_classes(iface)
                .contains(&person),
            "Person should implement interface {iface:?}"
        );
    }
    let implemented = relation_index.get_implemented_interfaces(person);
    for iface in [stringer, named, formatter] {
        assert!(
            implemented.contains(&iface),
            "Person should list interface {iface:?} as implemented"
        );
    }
}

#[test]
fn test_go_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::{
        TypeBindingKind, assert_return_has_type, find_bindings,
    };

    init_minimal_logging();
    let fixture = TestFixture::go_type_inference_control_flow()
        .expect("Failed to load go control flow fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "handleError", "string");
    assert_return_has_type(&bindings, "divide", "int");
    // Guard narrowings never render as duplicated rows: identical
    // (variable, type) entries collapse even when several statements share
    // the entity-span fallback.
    let err_narrowed = find_bindings(&bindings, "err", TypeBindingKind::Narrowed);
    let mut seen = std::collections::HashSet::new();
    for binding in &err_narrowed {
        assert!(
            seen.insert((binding.inferred_type.clone(), binding.shape.clone())),
            "duplicate narrowed row for 'err': {binding:?}"
        );
    }
}

#[test]
fn test_go_type_assertion_snapshot() {
    use cce_e2e_tests::type_inference_assert::{
        assert_narrowed_has_type, assert_return_has_type, assert_variable_has_type,
    };

    init_minimal_logging();
    let fixture = TestFixture::go_type_inference_type_assertion()
        .expect("Failed to load go type assertion fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "describe", "string");
    assert_return_has_type(&bindings, "switchType", "string");
    assert_variable_has_type(&bindings, "Name", "string");
    // Comma-ok assertions bind the assertion target directly.
    assert_narrowed_has_type(&bindings, "s", "string");
    assert_narrowed_has_type(&bindings, "n", "int");
    // Type-switch cases bind the alias per single-type case.
    assert_narrowed_has_type(&bindings, "v", "string");
    assert_narrowed_has_type(&bindings, "v", "Greeter");
}
