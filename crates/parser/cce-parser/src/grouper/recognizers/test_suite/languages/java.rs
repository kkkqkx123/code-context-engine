//! Java, Kotlin and Groovy test detection
//!
//! Detects Java/Kotlin/Groovy test entities using:
//! - `@Test` / `@ParameterizedTest` annotation adjacency
//! - `*Test` / `*Tests` class naming conventions (all three languages)
//! - `*Spec` class naming convention (Kotlin and Groovy: Kotest BDD classes
//!   such as `FunSpec`-derived `CalculatorSpec`, Spock specifications;
//!   Java keeps the stricter `*Test` set since `*Spec` has no Java counterpart)

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

/// Detect Java/Kotlin/Groovy test classes by the conventional `*Test`/`*Tests`
/// suffix (plus `*Spec` for Kotlin and Groovy) (confidence `High`,
/// per-language convention).
pub fn detect_conventional(entity: &Entity, language: &Language) -> Option<TestInfo> {
    if !matches!(entity.kind, EntityKind::Class | EntityKind::Interface) {
        return None;
    }
    let name = entity.name.as_str();
    let spec_convention =
        matches!(language, Language::Kotlin | Language::Groovy) && name.ends_with("Spec");
    if name.ends_with("Test") || name.ends_with("Tests") || spec_convention {
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
    fn test_detect_spec_class_kotlin_and_groovy() {
        // Kotest `*Spec` classes match Kotlin, Spock specifications match
        // Groovy; Java has no `*Spec` convention and stays strict.
        assert!(detect_conventional(&class("CalculatorSpec"), &Language::Kotlin).is_some());
        assert!(detect_conventional(&class("UserServiceSpec"), &Language::Groovy).is_some());
        assert!(detect_conventional(&class("CalculatorSpec"), &Language::Java).is_none());
        // snake_case `contest_spec`-style names must never match
        assert!(detect_conventional(&class("contest_spec"), &Language::Kotlin).is_none());
        assert!(detect_conventional(&class("contest_spec"), &Language::Groovy).is_none());
        // `Test`/`Tests` suffixes still apply to Kotlin and Groovy
        assert!(detect_conventional(&class("UserServiceTest"), &Language::Kotlin).is_some());
        assert!(detect_conventional(&class("UserServiceTest"), &Language::Groovy).is_some());
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
