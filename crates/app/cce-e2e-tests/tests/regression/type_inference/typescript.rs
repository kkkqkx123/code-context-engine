//! TypeScript type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[tokio::test]
async fn test_typescript_generics_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::typescript_type_inference_generics()
        .expect("Failed to load typescript generics fixture");
    let orchestrator = run_index(fixture, vec!["ts".to_string()]).await;

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
        "Should have extracted function entities from TypeScript generics"
    );
}

#[tokio::test]
async fn test_typescript_unions_type_inference() {
    init_minimal_logging();

    let fixture = TestFixture::typescript_type_inference_unions()
        .expect("Failed to load typescript unions fixture");
    let orchestrator = run_index(fixture, vec!["ts".to_string()]).await;

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 1,
        "Should have at least one file indexed"
    );
}

#[test]
fn test_typescript_generics_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_generics()
        .expect("Failed to load typescript generics fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrapInArray", "T[]");
    assert_return_has_type(&bindings, "makePair", "Pair<A, B>");
    assert_return_has_type(&bindings, "swap", "Pair<B, A>");
    assert_return_has_type(&bindings, "collectToMap", "Map<K, V>");
    // Call-site generic substitution.
    assert_variable_has_type(&bindings, "pair", "Pair<number, string>");
    assert_variable_has_type(&bindings, "swapped", "Pair<string, number>");
    assert_variable_has_type(&bindings, "wrapped", "number[]");
    assert_variable_has_type(&bindings, "map", "Map<string, number>");
    // Explicit annotations must not be downgraded by generic machinery.
    assert_variable_has_type(&bindings, "container", "Container<string>");
}

#[test]
fn test_typescript_unions_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_unions()
        .expect("Failed to load typescript unions fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "string");
    assert_narrowed_has_type(&bindings, "x", "string");
    assert_narrowed_has_type(&bindings, "x", "number");
    assert_narrowed_has_type(&bindings, "x", "Array");
    assert_return_has_type(&bindings, "handleResult", "string");
    assert_return_has_type(&bindings, "processValue", "string");
}

#[test]
fn test_typescript_overloads_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_overloads()
        .expect("Failed to load typescript overloads fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "combine", "number | string");
    assert_return_has_type(&bindings, "run", "string");
}

#[test]
fn test_typescript_visibility_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_visibility()
        .expect("Failed to load typescript visibility fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "consumePublic", "string");
    assert_return_has_type(&bindings, "consumeDescribe", "string");
    assert_variable_has_type(&bindings, "pubField", "string");
}

#[test]
fn test_typescript_lambda_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_lambda()
        .expect("Failed to load typescript lambda fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "applyTwice", "number");
    assert_return_has_type(&bindings, "fetchGreeting", "Promise<string>");
}

#[test]
fn test_typescript_destructuring_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_destructuring()
        .expect("Failed to load typescript destructuring fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "name", "string");
    assert_variable_has_type(&bindings, "age", "number");
    assert_variable_has_type(&bindings, "first", "string");
    assert_variable_has_type(&bindings, "second", "string");
    assert_return_has_type(&bindings, "greet", "string");
}

#[test]
fn test_typescript_negated_checks_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_negated_checks()
        .expect("Failed to load typescript negated checks fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "undefined");
    assert_return_has_type(&bindings, "handleNotNull", "string");
    assert_return_has_type(&bindings, "handleNegated", "string");
}

#[test]
fn test_typescript_control_positions_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_narrowed_has_type, assert_return_has_type};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_control_positions()
        .expect("Failed to load typescript control positions fixture");
    let bindings = snapshot_for(&fixture);

    assert_narrowed_has_type(&bindings, "value", "string");
    assert_narrowed_has_type(&bindings, "value", "number");
    assert_narrowed_has_type(&bindings, "value", "Array");
    assert_return_has_type(&bindings, "handleWhile", "string");
    assert_return_has_type(&bindings, "handleElseIf", "string");
    assert_return_has_type(&bindings, "earlyReturn", "string");
}

#[tokio::test]
async fn test_typescript_basic_resolves_cross_file_calls() {
    init_minimal_logging();

    let fixture = TestFixture::typescript_basic().expect("Failed to load typescript basic fixture");
    let orchestrator = run_index(fixture, vec!["ts".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 3,
        "typescript basic should index calculator, string_utils and main"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "typescript basic should extract function entities"
    );
}

#[test]
fn test_typescript_cross_file_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_cross_file()
        .expect("Failed to load typescript cross-file fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "loadUser", "User");
    assert_return_has_type(&bindings, "renderGreeting", "string");
}

#[tokio::test]
async fn test_typescript_cross_file_resolves_calls() {
    init_minimal_logging();

    let fixture = TestFixture::typescript_type_inference_cross_file()
        .expect("Failed to load typescript cross-file fixture");
    let orchestrator = run_index(fixture, vec!["ts".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 2,
        "typescript cross-file should index models and service"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "typescript cross-file should extract function entities"
    );
    assert!(
        relation_index.resolved_relation_count() >= 1,
        "typescript cross-file should resolve at least one call edge"
    );
}

#[tokio::test]
async fn test_typescript_visibility_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_visibility()
        .expect("Failed to load typescript visibility fixture");
    let orchestrator = run_index(fixture, vec!["ts".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_typescript_overloads_resolve_per_callsite() {
    use cce_e2e_tests::type_inference_assert::{TypeBindingKind, find_bindings};

    init_minimal_logging();
    let fixture = TestFixture::typescript_type_inference_overloads()
        .expect("Failed to load ts overloads fixture");
    let bindings = snapshot_for(&fixture);

    // Each call site binds exactly to its matching overload signature,
    // not the implementation's union return.
    for (var, expected) in [("ints", "number"), ("strs", "string"), ("mixed", "string")] {
        let hits = find_bindings(&bindings, var, TypeBindingKind::Variable);
        assert_eq!(
            hits.len(),
            1,
            "expected one binding for '{var}', got {hits:#?}"
        );
        assert_eq!(hits[0].inferred_type, expected);
    }
}
