//! JavaScript type inference tests.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::{run_index, snapshot_for};

#[test]
fn test_javascript_narrowing_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::javascript_type_inference_narrowing().expect("Failed to load js fixture");
    let bindings = snapshot_for(&fixture);

    // Return expressions are normalized to type vocabulary (`'empty'` /
    // `'unknown'` -> `string`, `{...}` -> `object`); `typeof`/`instanceof`
    // narrowing does not produce bindings for this fixture yet.
    assert_return_has_type(&bindings, "handleResult", "string");
    assert_return_has_type(&bindings, "process", "string");
    assert_return_has_type(&bindings, "createUser", "object");
}

#[tokio::test]
async fn test_javascript_basic_resolves_cross_file_calls() {
    init_minimal_logging();

    let fixture = TestFixture::javascript_basic().expect("Failed to load javascript basic fixture");
    let orchestrator = run_index(fixture, vec!["js".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 3,
        "javascript basic should index calculator, stringUtils and main"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "javascript basic should extract function entities"
    );
}

#[tokio::test]
async fn test_javascript_cross_file_resolves_calls() {
    init_minimal_logging();

    let fixture = TestFixture::javascript_type_inference_cross_file()
        .expect("Failed to load javascript cross-file fixture");
    let orchestrator = run_index(fixture, vec!["js".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");

    assert!(
        relation_index.file_count() >= 2,
        "javascript cross-file should index models and service"
    );
    assert!(
        !relation_index.function_index().is_empty(),
        "javascript cross-file should extract function entities"
    );
    assert!(
        relation_index.resolved_relation_count() >= 1,
        "javascript cross-file should resolve at least one call edge"
    );
}

#[test]
fn test_javascript_cross_file_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::javascript_type_inference_cross_file()
        .expect("Failed to load javascript cross-file fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "loadUser", "User");
    assert_variable_has_type(&bindings, "user", "User");
}

#[tokio::test]
async fn test_javascript_wildcard_type_inference() {
    init_minimal_logging();
    let fixture = TestFixture::javascript_type_inference_wildcard()
        .expect("Failed to load javascript wildcard fixture");
    let orchestrator = run_index(fixture, vec!["js".to_string()]).await;
    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available");
    assert!(relation_index.file_count() >= 1);
    assert!(relation_index.resolved_relation_count() >= 1);
}

#[test]
fn test_javascript_wildcard_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_return_has_type;

    init_minimal_logging();
    let fixture = TestFixture::javascript_type_inference_wildcard()
        .expect("Failed to load javascript wildcard fixture");
    let bindings = snapshot_for(&fixture);

    assert_return_has_type(&bindings, "alpha", "number");
    assert_return_has_type(&bindings, "beta", "number");

    // sum's return body is `utils.alpha() + utils.beta()` — an
    // arithmetic expression over two known numeric calls.  The extractor
    // intentionally drops compound expressions so the inferer falls back
    // to unknown.
}
