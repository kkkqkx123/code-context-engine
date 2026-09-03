//! Annotation handling for entity extraction
//!
//! This module handles:
//! - Language-specific annotation/attribute semantics detection
//! - Test attribute identification
//! - Rust attribute filtering

use cce_types::Entity;
use cce_types::language::Language;

/// Whether the language's annotation/attribute nodes modify the entity that
/// follows them in source, and are therefore buffered as pending annotations.
pub fn language_has_annotation_semantics(language: &Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Java
            | Language::Kotlin
            | Language::CSharp
            | Language::Scala
            | Language::Php
            | Language::JavaScript
            | Language::TypeScript
            | Language::Jsx
            | Language::Tsx
    )
}

/// Whether an annotation name marks its entity as a test case.
///
/// Covers Rust `#[test]` / `#[tokio::test]` / `#[test(...)]`,
/// Java/Kotlin `@Test` / `@ParameterizedTest`, Scala `@Test`, PHP `#[Test]`,
/// and C# `[Test]` / `[TestCase]` / `[Fact]` / `[Theory]`.
pub fn is_test_attribute(attr: &str) -> bool {
    attr == "test"
        || attr.starts_with("test(")
        || attr.ends_with("::test")
        || attr == "Test"
        || attr == "ParameterizedTest"
        || attr == "TestCase"
        || attr == "Fact"
        || attr == "Theory"
}

/// Whether a `cfg(...)` annotation targets the `test` configuration.
///
/// Tokenizes on non-alphanumeric characters and checks for an exact `test`
/// token, so `cfg(feature = "contest")` is never misjudged.
pub fn cfg_attribute_targets_test(attr: &str) -> bool {
    let Some(payload) = attr
        .strip_prefix("cfg(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    payload
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == "test")
}

/// Check if a Rust attribute entity should be skipped entirely
pub fn should_skip_rust_attr(entity: &Entity) -> bool {
    let exact_rust_attrs = [
        "inline",
        "cold",
        "must_use",
        "no_mangle",
        "non_exhaustive",
        "deprecated",
        "automatically_derived",
    ];
    if exact_rust_attrs.contains(&entity.name.as_str()) {
        return true;
    }

    let internal_param_attrs = ["path ", "allow(", "deny(", "warn(", "forbid("];
    if internal_param_attrs
        .iter()
        .any(|prefix| entity.name.starts_with(prefix))
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, EntityKind, Span};

    #[test]
    fn test_language_has_annotation_semantics() {
        assert!(language_has_annotation_semantics(&Language::Rust));
        assert!(language_has_annotation_semantics(&Language::Java));
        assert!(language_has_annotation_semantics(&Language::Kotlin));
        assert!(language_has_annotation_semantics(&Language::CSharp));
        assert!(language_has_annotation_semantics(&Language::Scala));
        assert!(language_has_annotation_semantics(&Language::Php));
        assert!(language_has_annotation_semantics(&Language::JavaScript));
        assert!(language_has_annotation_semantics(&Language::TypeScript));
        assert!(!language_has_annotation_semantics(&Language::Python));
        assert!(!language_has_annotation_semantics(&Language::Go));
    }

    #[test]
    fn test_is_test_attribute() {
        assert!(is_test_attribute("test"));
        assert!(is_test_attribute("test(tokio::test)"));
        assert!(is_test_attribute("tokio::test"));
        assert!(is_test_attribute("Test"));
        assert!(is_test_attribute("ParameterizedTest"));
        assert!(is_test_attribute("TestCase"));
        assert!(is_test_attribute("Fact"));
        assert!(is_test_attribute("Theory"));
        assert!(!is_test_attribute("bench"));
        assert!(!is_test_attribute("ignore"));
    }

    #[test]
    fn test_cfg_attribute_targets_test() {
        assert!(cfg_attribute_targets_test("cfg(test)"));
        assert!(cfg_attribute_targets_test("cfg(not(test))"));
        assert!(cfg_attribute_targets_test(
            "cfg(all(test, feature = \"x\"))"
        ));
        assert!(cfg_attribute_targets_test("cfg(feature = \"test\")"));
        assert!(!cfg_attribute_targets_test("cfg(feature = \"contest\")"));
        assert!(!cfg_attribute_targets_test("not_a_cfg"));
    }

    #[test]
    fn test_should_skip_rust_attr() {
        let entity = Entity::new(
            EntityId(0),
            EntityKind::Annotation,
            "inline".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        assert!(should_skip_rust_attr(&entity));

        let entity = Entity::new(
            EntityId(1),
            EntityKind::Annotation,
            "allow(dead_code)".to_string(),
            Span::new(0, 20, 0, 0, 0, 20),
        );
        assert!(should_skip_rust_attr(&entity));

        let entity = Entity::new(
            EntityId(2),
            EntityKind::Annotation,
            "test".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        assert!(!should_skip_rust_attr(&entity));
    }
}
