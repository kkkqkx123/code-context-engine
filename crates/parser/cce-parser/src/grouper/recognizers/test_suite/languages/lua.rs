//! Lua test detection
//!
//! Detects Lua test entities using:
//! - luaunit `test_` prefix functions, `test*` class methods and `test*`
//!   table fields, constrained to test files (the per-language path rule
//!   must hit first, so `test_foo` helpers in regular source files are
//!   never misjudged).
//!
//! Degradation: busted `describe`/`it`/`context` calls parse as
//! `function_call` nodes that the entity queries do not capture as entities
//! (verified with `tools/parse_test_detection.rs`), and their signals are
//! file-constrained anyway (`*_spec.lua` is the dominant busted convention)
//! — only path rules are implemented for them, matching the Dart/Ruby
//! precedent.

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

/// Detect luaunit test functions/methods/fields inside test files
/// (confidence `High`, requires the per-language path rule).
pub fn detect_conventional(entity: &Entity, file_path: &str) -> Option<TestInfo> {
    let in_test_file = TestInfo::from_path(Some(&Language::Lua), file_path).is_test();
    if !in_test_file {
        return None;
    }
    let name = entity.name.as_str();
    let is_luaunit_callable = matches!(
        entity.kind,
        EntityKind::Function | EntityKind::Method | EntityKind::Field
    );
    if is_luaunit_callable && is_luaunit_test_name(name) {
        return Some(TestInfo::test_ast());
    }
    None
}

/// Whether a name follows the luaunit test naming: `test_*` (snake_case,
/// top-level functions and class methods) or `test` + digit (`test1`-style
/// fields, the canonical luaunit class example). camelCase `testFoo` is
/// deliberately rejected: `testMode`-style production names share the same
/// shape. `testify` and exact `test` never match.
fn is_luaunit_test_name(name: &str) -> bool {
    if name.starts_with("test_") {
        return true;
    }
    name.as_bytes().get(4).is_some_and(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;

    fn entity(kind: EntityKind, name: &str) -> Entity {
        Entity::new(EntityId(0), kind, name.to_string(), Span::default())
    }

    #[test]
    fn test_detect_luaunit_top_level_function_in_test_file() {
        let e = entity(EntityKind::Function, "test_addition");
        assert!(detect_conventional(&e, "tests/test_calc.lua").is_some());
        assert!(detect_conventional(&e, "test_calc.lua").is_some());
        assert!(detect_conventional(&e, "spec/test_calc.lua").is_some());
    }

    #[test]
    fn test_detect_luaunit_class_methods_in_test_file() {
        let snake = entity(EntityKind::Method, "test_add");
        assert!(detect_conventional(&snake, "tests/test_calc.lua").is_some());
        let field = entity(EntityKind::Field, "test1");
        assert!(detect_conventional(&field, "tests/test_calc.lua").is_some());
    }

    #[test]
    fn test_detect_requires_test_file() {
        let e = entity(EntityKind::Function, "test_addition");
        assert!(detect_conventional(&e, "src/math.lua").is_none());
        assert!(detect_conventional(&e, "lib/testMode.lua").is_none());
    }

    #[test]
    fn test_luaunit_name_precision() {
        // `testify` / `testMode` (camelCase) / exact `test` / `TestRunner`-
        // style classes must never match; `latest` / `contest` are out of
        // scope by default.
        assert!(!is_luaunit_test_name("testify"));
        assert!(!is_luaunit_test_name("testMode"));
        assert!(!is_luaunit_test_name("testSubtract"));
        assert!(!is_luaunit_test_name("test"));
        assert!(!is_luaunit_test_name("TestRunner"));
        assert!(!is_luaunit_test_name("contest"));
        assert!(!is_luaunit_test_name("latest"));
    }

    #[test]
    fn test_non_callable_entities_not_detected() {
        let table = entity(EntityKind::Class, "TestCalculator");
        assert!(detect_conventional(&table, "tests/test_calc.lua").is_none());
    }
}
