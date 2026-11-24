//! Lua type inference tests.

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::snapshot_for;

#[test]
fn test_lua_variables_snapshot() {
    use cce_e2e_tests::type_inference_assert::{assert_return_has_type, assert_variable_has_type};

    init_minimal_logging();
    let fixture = TestFixture::lua_type_inference_variables().expect("Failed to load lua fixture");
    let bindings = snapshot_for(&fixture);

    assert_variable_has_type(&bindings, "app_name", "string");
    assert_variable_has_type(&bindings, "max_retries", "number");
    assert_variable_has_type(&bindings, "verbose", "boolean");
    assert_variable_has_type(&bindings, "counter", "number");

    assert_return_has_type(&bindings, "log_message", "string");
    assert_variable_has_type(&bindings, "greeting", "string");
}
