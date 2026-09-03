//! C++ test detection
//!
//! Detects C++ test entities using:
//! - `TEST(...)` / `TEST_F(...)` GoogleTest macro invocations, validated by
//!   parameter shape (exactly two bare-identifier arguments)
//! - `*_test.cpp` / `*_test.cc` path rules (in `TestInfo::from_path`)
//!
//! GoogleTest macros are legal in any translation unit, so the macro name
//! alone is never trusted: the entity must be a `Constructor`-shaped
//! definition (macro calls parse as `function_definition` with an identifier
//! declarator and no return type) AND the argument list between the name and
//! the body must contain exactly two comma-separated identifiers.
//!
//! Catch2 `TEST_CASE(...)` invocations parse as plain `call_expression` nodes
//! and produce no entities, so they are not detected at entity level (path
//! rules still cover `*_test.cpp`/`*_test.cc` files).

use cce_types::entity::Entity;
use cce_types::entity::EntityKind;
use cce_types::test_info::TestInfo;

/// Detect GoogleTest `TEST`/`TEST_F` macro invocations by shape (confidence
/// `High`).
///
/// The entity must be constructor-shaped (macro invocation without return
/// type) and the source between the macro name and the body must be a
/// two-identifier argument list: `TEST(SuiteName, CaseName)`.
pub fn detect_conventional(entity: &Entity, source: &str) -> Option<TestInfo> {
    if entity.kind != EntityKind::Constructor {
        return None;
    }
    let name = entity.name.as_str();
    if name != "TEST" && name != "TEST_F" {
        return None;
    }
    if macro_args_are_two_identifiers(source, entity) {
        return Some(TestInfo::test_ast());
    }
    None
}

/// Whether the macro invocation starting at the entity span carries exactly
/// two bare-identifier arguments (`TEST(A, B)`), and never a typed parameter
/// list (`TEST(int a, int b)`) or a different arity.
fn macro_args_are_two_identifiers(source: &str, entity: &Entity) -> bool {
    let Some(rest) = source.get(entity.span.start_byte..) else {
        return false;
    };
    let Some(name_start) = rest.find(entity.name.as_str()) else {
        return false;
    };
    let after_name = &rest[name_start + entity.name.len()..];
    let Some(open) = after_name.find('(') else {
        return false;
    };

    let mut depth = 0usize;
    for (i, c) in after_name[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let args = &after_name[open + 1..open + i];
                    return split_exactly_two_identifiers(args);
                }
            }
            _ => {}
        }
    }
    false
}

/// Split a comma-separated argument list and require exactly two arguments,
/// each a single C/C++ identifier token.
fn split_exactly_two_identifiers(args: &str) -> bool {
    let parts: Vec<&str> = args.split(',').collect();
    if parts.len() != 2 {
        return false;
    }
    parts.iter().all(|part| is_identifier(part.trim()))
}

/// Whether a token is a single C/C++ identifier (`[A-Za-z_][A-Za-z0-9_]*`).
fn is_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;

    fn constructor(name: &str, start_byte: usize, end_byte: usize) -> Entity {
        Entity::new(
            EntityId(0),
            EntityKind::Constructor,
            name.to_string(),
            Span::new(start_byte, end_byte, 0, 0, 0, 0),
        )
    }

    #[test]
    fn test_detect_test_macro() {
        let source = "TEST(MathTest, Add) {\n  EXPECT_EQ(4, 4);\n}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_some());
    }

    #[test]
    fn test_detect_test_f_macro() {
        let source = "TEST_F(Fixture, Subtract) {\n  EXPECT_EQ(2, 2);\n}";
        let entity = constructor("TEST_F", 0, source.len());
        assert!(detect_conventional(&entity, source).is_some());
    }

    #[test]
    fn test_comment_between_name_and_args() {
        let source = "TEST /* suite */ (MathTest, Add) {}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_some());
    }

    #[test]
    fn test_single_argument_never_matches() {
        let source = "TEST(MathTest) {}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_three_arguments_never_match() {
        let source = "TEST(A, B, C) {}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_empty_arguments_never_match() {
        let source = "TEST() {}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_typed_parameters_never_match() {
        // A real function named TEST with typed parameters
        let source = "int TEST(int a, int b) { return a + b; }";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_non_identifier_argument_never_matches() {
        let source = "TEST(MathTest, 42) {}";
        let entity = constructor("TEST", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_non_constructor_kind_never_matches() {
        let source = "TEST(MathTest, Add) {}";
        let mut entity = constructor("TEST", 0, source.len());
        entity.kind = EntityKind::Function;
        assert!(detect_conventional(&entity, source).is_none());
    }

    #[test]
    fn test_other_names_never_match() {
        // `latest` / `test_runner` style names never match
        let source = "int latest(int x) { return x; }";
        let entity = constructor("latest", 0, source.len());
        assert!(detect_conventional(&entity, source).is_none());
    }
}
