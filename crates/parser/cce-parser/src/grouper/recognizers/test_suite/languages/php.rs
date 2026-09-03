//! PHP test detection
//!
//! Detects PHP test entities using:
//! - `#[Test]` attribute (PHP 8+, PHPUnit 10+) adjacency
//! - `@test` docblock marker (PHPUnit 4-9)
//! - `*Test` class naming conventions

use cce_types::entity::{Entity, EntityKind};
use cce_types::test_info::TestInfo;

/// Detect PHP test markers from the attribute nodes directly preceding the
/// entity and the attached doc comment (confidence `High`).
///
/// - `#[Test]` (PHPUnit 10+ attribute) on a method/class
/// - `@test` docblock (PHPUnit 4-9): a docblock line whose trimmed content is
///   exactly `@test` or starts with `@test `, so `@testdox`/`@testWith` never
///   match.
pub fn detect_from_annotations(
    entity: &Entity,
    adjacent_annotations: &[String],
) -> Option<TestInfo> {
    for annotation in adjacent_annotations {
        if annotation == "Test" {
            return Some(TestInfo::test_ast());
        }
    }
    if let Some(doc) = entity.doc_comment.as_deref() {
        if docblock_marks_test(doc) {
            return Some(TestInfo::test_ast());
        }
    }
    None
}

/// Whether a doc comment contains a `@test` marker line.
///
/// Handles both raw docblocks (` * @test` with the `*` prefix) and cleaned
/// doc comments (prefix stripped by the comment processor). `@testdox`/
/// `@testWith` never match: the marker must be `@test` alone or followed by
/// whitespace.
fn docblock_marks_test(doc: &str) -> bool {
    doc.lines().any(|line| {
        let trimmed = line.trim_start();
        let stripped = trimmed.strip_prefix('*').unwrap_or(trimmed).trim_start();
        stripped == "@test"
            || stripped
                .strip_prefix("@test ")
                .is_some_and(|rest| !rest.is_empty())
    })
}

/// Detect PHP test classes by the conventional `*Test` suffix (confidence
/// `High`, per-language convention).
pub fn detect_conventional(entity: &Entity) -> Option<TestInfo> {
    if !matches!(entity.kind, EntityKind::Class) {
        return None;
    }
    if entity.name.ends_with("Test") {
        return Some(TestInfo::test_ast_block());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;

    fn class(name: &str) -> Entity {
        Entity::new(
            EntityId(0),
            EntityKind::Class,
            name.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_detect_test_attribute() {
        assert!(detect_from_annotations(&class("Foo"), &["Test".to_string()]).is_some());
        assert!(detect_from_annotations(&class("Foo"), &["Deprecated".to_string()]).is_none());
    }

    #[test]
    fn test_detect_test_docblock() {
        let mut entity = class("Foo");
        entity.doc_comment = Some("/**\n * @test\n * Long description.\n */".to_string());
        assert!(detect_from_annotations(&entity, &[]).is_some());
    }

    #[test]
    fn test_docblock_testdox_not_test() {
        let mut entity = class("Foo");
        // `@testdox` / `@testWith` share the `@test` prefix but are not markers
        entity.doc_comment = Some("/** @testdox The test description. */".to_string());
        assert!(detect_from_annotations(&entity, &[]).is_none());
        entity.doc_comment = Some("/** @testWith(\"data\") */".to_string());
        assert!(detect_from_annotations(&entity, &[]).is_none());
    }

    #[test]
    fn test_detect_test_class_convention() {
        assert!(detect_conventional(&class("CalculatorTest")).is_some());
        // `TestRunner` / `Contest` style names must never match
        assert!(detect_conventional(&class("TestRunner")).is_none());
        assert!(detect_conventional(&class("Contest")).is_none());
    }
}
