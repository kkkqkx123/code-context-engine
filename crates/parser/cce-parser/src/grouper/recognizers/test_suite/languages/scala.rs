//! Scala test detection
//!
//! Detects Scala test entities using:
//! - `@Test` (JUnit) annotation adjacency
//! - `*Test` / `*Tests` / `*Spec` class naming conventions

use cce_types::entity::{Entity, EntityKind};
use cce_types::test_info::TestInfo;

/// Detect Scala test annotations from the annotation nodes directly preceding
/// the entity (confidence `High`). JUnit `@Test` is the primary marker;
/// ScalaTest/MUnit conventions are covered by the class naming convention.
pub fn detect_from_annotations(adjacent_annotations: &[String]) -> Option<TestInfo> {
    for annotation in adjacent_annotations {
        if annotation == "Test" {
            return Some(TestInfo::test_ast());
        }
    }
    None
}

/// Detect Scala test classes by the conventional `*Test`/`*Tests`/`*Spec`
/// suffix (confidence `High`, per-language convention).
///
/// Covers ScalaTest/MUnit `Spec`-style classes and JUnit-style `*Test` classes
/// (including `object` singletons, which map to `Class`).
pub fn detect_conventional(entity: &Entity) -> Option<TestInfo> {
    if !matches!(entity.kind, EntityKind::Class | EntityKind::Trait) {
        return None;
    }
    let name = entity.name.as_str();
    if name.ends_with("Test") || name.ends_with("Tests") || name.ends_with("Spec") {
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
    fn test_detect_test_annotation() {
        assert!(detect_from_annotations(&["Test".to_string()]).is_some());
        assert!(detect_from_annotations(&["tailrec".to_string()]).is_none());
        assert!(detect_from_annotations(&["Override".to_string()]).is_none());
    }

    #[test]
    fn test_detect_test_class_convention() {
        assert!(detect_conventional(&class("CalculatorSpec")).is_some());
        assert!(detect_conventional(&class("CalculatorTest")).is_some());
        assert!(detect_conventional(&class("CalculatorTests")).is_some());
        // `TestRunner` / snake_case `contest_spec` style names never match
        assert!(detect_conventional(&class("TestRunner")).is_none());
        assert!(detect_conventional(&class("contest_spec")).is_none());
        assert!(detect_conventional(&class("Contest")).is_none());
    }

    #[test]
    fn test_method_not_class() {
        let method = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "shouldReturnUser".to_string(),
            Span::default(),
        );
        // Naming convention only applies to classes; methods need annotations
        assert!(detect_conventional(&method).is_none());
    }
}
