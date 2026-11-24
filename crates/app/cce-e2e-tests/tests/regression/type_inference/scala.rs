//! Scala type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[test]
fn test_scala_control_flow_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_narrowed_has_type;

    init_minimal_logging();
    // NB: `val`/return metadata is not captured by the Scala extractor yet,
    // so `declarations.scala` is index-smoke only. `match`/`isInstanceOf`
    // narrowing works.
    let fixture = TestFixture::scala_type_inference_control_flow()
        .expect("Failed to load scala control flow fixture");
    let bindings = snapshot_for(&fixture);
    assert_narrowed_has_type(&bindings, "c", "Circle");
}

#[tokio::test]
async fn test_scala_basic_resolves_calls() {
    init_minimal_logging();

    let fixture = TestFixture::scala_basic().expect("Failed to load scala basic fixture");
    let orchestrator = run_index(fixture, vec!["scala".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 2,
        "scala basic should index Models and Main"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "scala basic should extract function entities"
    );
}

#[tokio::test]
async fn test_scala_cross_file_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::scala_type_inference_cross_file()
        .expect("Failed to load scala cross_file fixture");
    let orchestrator = run_index(fixture, vec!["scala".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[tokio::test]
async fn test_scala_overloads_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::scala_type_inference_overloads()
        .expect("Failed to load scala overloads fixture");
    let orchestrator = run_index(fixture, vec!["scala".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[tokio::test]
async fn test_scala_visibility_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::scala_type_inference_visibility()
        .expect("Failed to load scala visibility fixture");
    let orchestrator = run_index(fixture, vec!["scala".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
}

#[test]
fn test_scala_declarations_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::scala_type_inference_declarations()
        .expect("Failed to load scala declarations fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "identity", "T");
    assert_return_has_type(&bindings, "wrapInList", "List[T]");
    assert_return_has_type(&bindings, "toMap", "Map[K, V]");
    assert_variable_has_type(&bindings, "id", "String");
    assert_variable_has_type(&bindings, "biggest", "Int");
}

#[test]
fn test_scala_for_comprehension_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::scala_type_inference_for_comprehension()
        .expect("Failed to load scala for_comprehension fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "usernames", "List[String]");
    assert_return_has_type(&bindings, "pairs", "List[(Int, String)]");
    assert_variable_has_type(&bindings, "users", "List");

    assert_variable_has_type(&bindings, "user", "User");
    assert_variable_has_type(&bindings, "n", "Int");
    assert_variable_has_type(&bindings, "label", "String");
}
