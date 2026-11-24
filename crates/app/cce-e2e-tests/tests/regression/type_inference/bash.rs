//! Bash type inference tests.

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::snapshot_for;

#[test]
fn test_bash_variables_snapshot() {
    use cce_e2e_tests::type_inference_assert::assert_variable_has_type;

    init_minimal_logging();
    let fixture =
        TestFixture::bash_type_inference_variables().expect("Failed to load bash fixture");
    let bindings = snapshot_for(&fixture);

    // Shell literals bind with shell vocabulary: quoted words are
    // strings, bare words/numbers are ints.
    assert_variable_has_type(&bindings, "APP_NAME", "string");
    assert_variable_has_type(&bindings, "MAX_RETRIES", "int");
    assert_variable_has_type(&bindings, "retry_count", "int");
}
