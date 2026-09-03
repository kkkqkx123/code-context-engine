//! C# test detection
//!
//! Detects C# test entities using:
//! - `[Test]` / `[TestCase]` / `[Fact]` / `[Theory]` attribute adjacency
//! - `*Test` / `*Tests` class naming conventions

use cce_types::entity::{Entity, EntityKind};
use cce_types::test_info::TestInfo;

/// Detect C# test attributes from the attribute nodes directly preceding the
/// entity (confidence `High`).
///
/// NUnit: `[Test]`, `[TestCase(...)]`; xUnit: `[Fact]`, `[Theory]`.
pub fn detect_from_annotations(adjacent_annotations: &[String]) -> Option<TestInfo> {
    for annotation in adjacent_annotations {
        match annotation.as_str() {
            "Test" | "TestCase" | "Fact" | "Theory" => return Some(TestInfo::test_ast()),
            _ => {}
        }
    }
    None
}

/// Detect C# test classes by the conventional `*Test`/`*Tests` suffix
/// (confidence `High`, per-language convention).
pub fn detect_conventional(entity: &Entity) -> Option<TestInfo> {
    if !matches!(entity.kind, EntityKind::Class) {
        return None;
    }
    let name = entity.name.as_str();
    if name.ends_with("Test") || name.ends_with("Tests") {
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
    fn test_detect_nunit_attributes() {
        assert!(detect_from_annotations(&["Test".to_string()]).is_some());
        assert!(detect_from_annotations(&["TestCase".to_string()]).is_some());
    }

    #[test]
    fn test_detect_xunit_attributes() {
        assert!(detect_from_annotations(&["Fact".to_string()]).is_some());
        assert!(detect_from_annotations(&["Theory".to_string()]).is_some());
    }

    #[test]
    fn test_non_test_attribute() {
        assert!(detect_from_annotations(&["TestFixture".to_string()]).is_none());
        assert!(detect_from_annotations(&["Obsolete".to_string()]).is_none());
    }

    #[test]
    fn test_detect_test_class_convention() {
        assert!(detect_conventional(&class("CalculatorTests")).is_some());
        assert!(detect_conventional(&class("CalculatorTest")).is_some());
        // `TestRunner` / `Contest` style names must never match
        assert!(detect_conventional(&class("TestRunner")).is_none());
        assert!(detect_conventional(&class("Contest")).is_none());
    }

    #[test]
    fn test_method_not_class() {
        let method = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "ShouldReturnUser".to_string(),
            Span::default(),
        );
        // Naming convention only applies to classes; methods need attributes
        assert!(detect_conventional(&method).is_none());
    }
}
