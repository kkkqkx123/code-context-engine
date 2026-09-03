//! Java and Kotlin test detection
//!
//! Detects Java/Kotlin test entities using:
//! - `@Test` / `@ParameterizedTest` annotation adjacency
//! - `*Test` / `*Tests` class naming conventions (Java and Kotlin)
//! - `*Spec` class naming convention (Kotlin only, Kotest BDD classes such
//!   as `FunSpec`-derived `CalculatorSpec`; Java keeps the stricter `*Test`
//!   set since `*Spec` has no Java counterpart)

use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

/// Detect Java/Kotlin test annotations from the annotation nodes directly
/// preceding the entity (confidence `High`).
pub fn detect_from_annotations(adjacent_annotations: &[String]) -> Option<TestInfo> {
    for annotation in adjacent_annotations {
        match annotation.as_str() {
            "Test" | "ParameterizedTest" => return Some(TestInfo::test_ast()),
            _ => {}
        }
    }
    None
}

/// Detect Java/Kotlin test classes by the conventional `*Test`/`*Tests`
/// suffix (plus `*Spec` for Kotlin) (confidence `High`, per-language
/// convention).
pub fn detect_conventional(entity: &Entity, language: &Language) -> Option<TestInfo> {
    if !matches!(entity.kind, EntityKind::Class | EntityKind::Interface) {
        return None;
    }
    let name = entity.name.as_str();
    let kotlin_spec = matches!(language, Language::Kotlin) && name.ends_with("Spec");
    if name.ends_with("Test") || name.ends_with("Tests") || kotlin_spec {
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
        assert!(detect_from_annotations(&["ParameterizedTest".to_string()]).is_some());
        assert!(detect_from_annotations(&["Override".to_string()]).is_none());
    }

    #[test]
    fn test_detect_test_class_convention() {
        assert!(detect_conventional(&class("UserServiceTest"), &Language::Java).is_some());
        assert!(detect_conventional(&class("UserServiceTests"), &Language::Java).is_some());
        assert!(detect_conventional(&class("TestRunner"), &Language::Java).is_none());
        assert!(detect_conventional(&class("Contest"), &Language::Java).is_none());
    }

    #[test]
    fn test_detect_spec_class_kotlin_only() {
        // Kotest `*Spec` classes are Kotlin-only: Kotlin matches, Java does
        // not (Java has no `*Spec` convention).
        assert!(detect_conventional(&class("CalculatorSpec"), &Language::Kotlin).is_some());
        assert!(detect_conventional(&class("UserServiceSpec"), &Language::Kotlin).is_some());
        assert!(detect_conventional(&class("CalculatorSpec"), &Language::Java).is_none());
        // snake_case `contest_spec`-style names must never match
        assert!(detect_conventional(&class("contest_spec"), &Language::Kotlin).is_none());
        // `Test`/`Tests` suffixes still apply to Kotlin
        assert!(detect_conventional(&class("UserServiceTest"), &Language::Kotlin).is_some());
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
        assert!(detect_conventional(&method, &Language::Kotlin).is_none());
    }
}
