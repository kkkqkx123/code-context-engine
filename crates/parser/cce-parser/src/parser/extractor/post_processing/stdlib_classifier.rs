//! Post-processing: plugin-declared external entities
//!
//! The single writer of [`Entity::is_stdlib`], and it only ever copies a
//! plugin's own declaration.
//!
//! # Design Note
//!
//! `is_stdlib` answers one question: *is this definition something other than
//! the project's own code?* Only the plugin knows. A name lookup cannot answer
//! it — `struct Command` written by the project is not `std::process::Command`
//! — and that is how a name match used to mark project types stdlib and, from
//! there, compress their fields, methods, parameters and return types out of
//! the natural language text.
//!
//! Import *references* are therefore not marked here at all. What an import
//! points at is a property of the edge, carried by
//! `Relation::stdlib_category` and by `ImportClassifier`'s per-import class;
//! storing it on the import entity only produced two answers for one fact (see
//! `docs/plan/import-classification-and-stdlib-semantics.md`).

use cce_types::entity::Entity;

use crate::grouper::types::StdlibCategory;

/// Copy a plugin's explicit stdlib declaration onto the entity.
///
/// Custom-language plugins set `metadata["is_stdlib"] = "true"` (optionally
/// with `metadata["stdlib_category"]`) to say that an entity they emitted is
/// external code rather than project code. Built-in languages never call this
/// with a marker, so for them the flag keeps its default `false`.
///
/// The category defaults to `Other` so consumers can rely on `is_stdlib` and
/// `stdlib_category` being consistent: a marked entity always has a category.
pub fn mark_plugin_declared_external(entity: &mut Entity) {
    if let Some("true") = entity.metadata.get("is_stdlib").map(String::as_str) {
        entity.is_stdlib = true;
        entity.stdlib_category = Some(
            entity
                .metadata
                .get("stdlib_category")
                .and_then(|c| parse_category(c))
                .unwrap_or(StdlibCategory::Other),
        );
    }
}

/// Parse a stdlib category string from plugin metadata.
///
/// Accepts the serde variant names (`"Collection"`, `"Io"`, …) and the
/// lowercase display labels (`"collections"`, `"io"`, `"utilities"`, …).
fn parse_category(value: &str) -> Option<StdlibCategory> {
    if let Ok(c) = serde_json::from_str::<StdlibCategory>(value) {
        return Some(c);
    }
    let lower = value.to_lowercase();
    match lower.as_str() {
        "collections" | "collection" | "data_structure" => Some(StdlibCategory::Collection),
        "io" => Some(StdlibCategory::Io),
        "concurrency" | "threading" => Some(StdlibCategory::Concurrency),
        "utilities" | "utility" => Some(StdlibCategory::Utility),
        "strings" | "string" => Some(StdlibCategory::String),
        "numerics" | "numeric" | "numbers" | "math" => Some(StdlibCategory::Numeric),
        "errors" | "error" => Some(StdlibCategory::Error),
        "macros" | "macro" => Some(StdlibCategory::Macro),
        "traits" | "trait" | "interfaces" => Some(StdlibCategory::Trait),
        // Matches the display label of `StdlibCategory::Other`, so a category
        // read back from metadata round-trips instead of being dropped.
        "other_stdlib" | "other" => Some(StdlibCategory::Other),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::EntityKind;

    #[test]
    fn test_parse_category_serde_variant() {
        assert_eq!(
            parse_category("Collection"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(parse_category("Io"), Some(StdlibCategory::Io));
    }

    #[test]
    fn test_parse_category_display_labels() {
        assert_eq!(
            parse_category("collections"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(parse_category("utilities"), Some(StdlibCategory::Utility));
        assert_eq!(parse_category("other_stdlib"), Some(StdlibCategory::Other));
    }

    #[test]
    fn test_plugin_marker_is_copied() {
        let mut entity = Entity {
            name: "my_stdlib_fn".to_string(),
            ..Default::default()
        };
        entity
            .metadata
            .insert("is_stdlib".to_string(), "true".to_string());
        entity
            .metadata
            .insert("stdlib_category".to_string(), "utility".to_string());
        mark_plugin_declared_external(&mut entity);
        assert!(entity.is_stdlib);
        assert_eq!(entity.stdlib_category, Some(StdlibCategory::Utility));
    }

    #[test]
    fn test_plugin_marker_defaults_to_other() {
        let mut entity = Entity {
            name: "fn".to_string(),
            ..Default::default()
        };
        entity
            .metadata
            .insert("is_stdlib".to_string(), "true".to_string());
        mark_plugin_declared_external(&mut entity);
        assert!(entity.is_stdlib);
        assert_eq!(entity.stdlib_category, Some(StdlibCategory::Other));
    }

    #[test]
    fn test_builtin_languages_never_mark() {
        // A definition that shares a name with a stdlib symbol is project
        // code; so is an import statement, which is this project's own line
        // and only *points at* something external.
        for (name, kind) in [
            ("Command", EntityKind::Struct),
            ("Error", EntityKind::Enum),
            ("String", EntityKind::Class),
            ("println", EntityKind::Function),
            ("Vec", EntityKind::TypeAlias),
            ("MAX", EntityKind::Constant),
            ("std::collections::HashMap", EntityKind::Import),
            ("json", EntityKind::Require),
            ("stdio.h", EntityKind::Include),
        ] {
            let mut entity = Entity {
                name: name.to_string(),
                kind,
                ..Default::default()
            };
            mark_plugin_declared_external(&mut entity);
            assert!(
                !entity.is_stdlib,
                "{kind} {name} must not be marked by the built-in path"
            );
            assert_eq!(entity.stdlib_category, None);
        }
    }

    #[test]
    fn test_plugin_marker_wins_for_any_kind() {
        // A plugin may declare a definition external; that is the only way the
        // flag is ever set.
        for kind in [EntityKind::Struct, EntityKind::Function, EntityKind::Import] {
            let mut entity = Entity {
                name: "sdk_widget".to_string(),
                kind,
                ..Default::default()
            };
            entity
                .metadata
                .insert("is_stdlib".to_string(), "true".to_string());
            mark_plugin_declared_external(&mut entity);
            assert!(entity.is_stdlib, "{kind} declared external by a plugin");
        }
    }
}
