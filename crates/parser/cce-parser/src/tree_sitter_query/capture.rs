//! Capture name types and utilities

pub mod behavior;
pub mod call;
pub mod constants;
pub mod control;
mod macros;

// Re-export Domain from parser_types to avoid duplication
pub use crate::tree_sitter_query::parser_types::Domain;
pub use call::{CallCategory, CallSubcategory};

// Re-export all constants for backward compatibility
pub use constants::*;

// =============================================================================
// Helper functions for backward compatibility
// =============================================================================

/// Build a full entity capture name
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::entity_name;
/// assert_eq!(entity_name("function"), "entity.function.name");
/// ```
pub fn entity_name(category: &str) -> String {
    format!("{}{}{}", ENTITY_PREFIX, category, NAME_SUFFIX)
}

/// Build a full entity capture name with category and subcategory
pub fn entity_name_with_subtype(category: &str, subtype: &str) -> String {
    format!("{}{}.{}{}", ENTITY_PREFIX, category, subtype, NAME_SUFFIX)
}

/// All known non-main attribute suffixes used in entity captures.
/// These are sub-captures that provide metadata (name, body, type, etc.) and
/// should NOT be treated as main entity captures.
/// IMPORTANT: When checking, these suffixes are only matched against
/// NON-CATEGORY components of the capture name (i.e., after the first
/// dot following the category), so `entity.type` is correctly treated
/// as a main capture while `entity.variable.typed.type` is not.
const KNOWN_ATTRIBUTE_SUFFIXES: &[&str] = &[
    ".name",
    ".body",
    ".params",
    ".value",
    ".type",
    ".return_type",
    ".selectors",
    ".block",
    ".blocks",
    ".encoding",
    ".path",
    ".url",
    ".keyword",
    ".left",
    ".right",
    ".important",
    ".unit",
    ".args",
    ".alias",
    ".content",
    ".tag_name",
    ".start_tag",
    ".end_tag",
    ".attributes",
    ".attr_name",
    ".attr_value",
    ".quoted_value",
    ".expr_value",
    ".declaration",
    ".func",
    ".modifier",
    ".pattern",
    ".color",
    ".string",
    ".integer",
    ".float",
    ".plain",
    ".visibility",
    ".cls_param",
    ".self_param",
    ".result",
];

/// Extract the category from a call capture name
///
/// Returns `None` if the capture name doesn't start with the call prefix
pub fn extract_call_category(name: &str) -> Option<&str> {
    if !name.starts_with(CALL_PREFIX) {
        return None;
    }

    let rest = &name[CALL_PREFIX.len()..];
    rest.split('.').next()
}

/// Extract the subcategory from a call capture name
///
/// Returns `None` if there is no subcategory or if the second part is a known suffix.
/// For example:
/// - "call.method.static" -> Some("static")
/// - "call.method.static.name" -> Some("static")
/// - "call.function.name" -> None ("name" is a suffix)
pub fn extract_call_subcategory(name: &str) -> Option<&str> {
    if !name.starts_with(CALL_PREFIX) {
        return None;
    }

    let rest = &name[CALL_PREFIX.len()..];
    let parts: Vec<&str> = rest.split('.').collect();

    let subcategory = parts.get(1).copied()?;

    if SUBCATEGORY_SUFFIXES.contains(&subcategory) {
        return None;
    }

    Some(subcategory)
}

/// Check if a capture name is a main entity capture (not a suffix capture)
///
/// A capture name like `entity.type` is a main capture (category-level),
/// while `entity.variable.typed.type` is an attribute capture (the `.type`
/// suffix applies to a subcategory, not the category itself).
pub fn is_main_entity_capture(name: &str) -> bool {
    if !name.starts_with(ENTITY_PREFIX) {
        return false;
    }

    let rest = &name[ENTITY_PREFIX.len()..];
    let first_dot = rest.find('.');
    match first_dot {
        None => {
            // "entity.type" → no subcategory dot → always main capture
            true
        }
        Some(pos) => {
            // Check suffixes only against the portion after the category
            let after_category = &rest[pos..];
            !KNOWN_ATTRIBUTE_SUFFIXES
                .iter()
                .any(|suffix| after_category.ends_with(suffix))
        }
    }
}

/// Check if a capture name is a name capture
pub fn is_name_capture(name: &str) -> bool {
    name.ends_with(NAME_SUFFIX)
}

/// Extract the category from a capture name
///
/// Returns `None` if the capture name doesn't start with the entity prefix
pub fn extract_category(name: &str) -> Option<&str> {
    if !name.starts_with(ENTITY_PREFIX) {
        return None;
    }

    let rest = &name[ENTITY_PREFIX.len()..];
    rest.split('.').next()
}

/// The subcategory suffixes (attribute-like parts that are NOT entity categories).
/// Used by `extract_subcategory` to distinguish between actual subcategories and
/// metadata attributes.
const SUBCATEGORY_SUFFIXES: &[&str] = &[
    "name",
    "body",
    "params",
    "value",
    "type",
    "return_type",
    "selectors",
    "block",
    "blocks",
    "encoding",
    "path",
    "url",
    "keyword",
    "left",
    "right",
    "important",
    "unit",
    "args",
    "alias",
    "content",
    "tag_name",
    "start_tag",
    "end_tag",
    "attributes",
    "attr_name",
    "attr_value",
    "quoted_value",
    "expr_value",
    "declaration",
    "func",
    "modifier",
    "pattern",
    "color",
    "string",
    "integer",
    "float",
    "plain",
    "visibility",
    "cls_param",
    "self_param",
    "result",
];

/// Extract the subcategory from a capture name
///
/// Returns `None` if there is no subcategory or if the second part is a known suffix.
/// For example:
/// - "entity.method.operator" -> Some("operator")
/// - "entity.function.name" -> None ("name" is a suffix)
/// - "entity.class" -> None
pub fn extract_subcategory(name: &str) -> Option<&str> {
    if !name.starts_with(ENTITY_PREFIX) {
        return None;
    }

    let rest = &name[ENTITY_PREFIX.len()..];
    let parts: Vec<&str> = rest.split('.').collect();

    // Get the second part if it exists
    let subcategory = parts.get(1).copied()?;

    // If the second part is a known suffix, return None
    if SUBCATEGORY_SUFFIXES.contains(&subcategory) {
        return None;
    }

    Some(subcategory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_name() {
        assert_eq!(entity_name("function"), "entity.function.name");
        assert_eq!(entity_name("class"), "entity.class.name");
    }

    #[test]
    fn test_entity_name_with_subtype() {
        assert_eq!(
            entity_name_with_subtype("method", "operator"),
            "entity.method.operator.name"
        );
    }

    #[test]
    fn test_is_main_entity_capture() {
        // Main entity captures don't end with known suffixes
        assert!(is_main_entity_capture("entity.function"));
        assert!(is_main_entity_capture("entity.function.generator"));
        assert!(is_main_entity_capture("entity.style_rule"));
        assert!(is_main_entity_capture("entity.style_selector.class"));

        // Suffix captures should NOT be main entities
        assert!(!is_main_entity_capture("entity.function.name"));
        assert!(!is_main_entity_capture("entity.function.body"));
        assert!(!is_main_entity_capture("entity.function.params"));
        assert!(!is_main_entity_capture("entity.function.return_type"));
        assert!(!is_main_entity_capture("entity.style_rule.selectors"));
        assert!(!is_main_entity_capture("entity.style_rule.block"));
        assert!(!is_main_entity_capture("call.function.name"));
        assert!(!is_main_entity_capture("entity.at_rule.keyword"));
        assert!(!is_main_entity_capture("entity.at_rule.url"));
        assert!(!is_main_entity_capture("entity.at_rule.encoding"));
        assert!(!is_main_entity_capture("entity.css_value.important"));
    }

    #[test]
    fn test_is_name_capture() {
        assert!(is_name_capture("entity.function.name"));
        assert!(!is_name_capture("entity.function.body"));
    }

    #[test]
    fn test_extract_category() {
        assert_eq!(extract_category("entity.function.name"), Some("function"));
        assert_eq!(extract_category("entity.class.body"), Some("class"));
        assert_eq!(extract_category("call.function.name"), None);
    }

    #[test]
    fn test_extract_subcategory() {
        assert_eq!(
            extract_subcategory("entity.method.operator.name"),
            Some("operator")
        );
        assert_eq!(extract_subcategory("entity.function.name"), None);
        assert_eq!(extract_subcategory("entity.class"), None);
    }

    #[test]
    fn test_extract_call_category() {
        assert_eq!(
            extract_call_category("call.function.name"),
            Some("function")
        );
        assert_eq!(
            extract_call_category("call.method.static.name"),
            Some("method")
        );
        assert_eq!(extract_call_category("entity.function.name"), None);
    }

    #[test]
    fn test_extract_call_subcategory() {
        assert_eq!(
            extract_call_subcategory("call.method.static.name"),
            Some("static")
        );
        assert_eq!(extract_call_subcategory("call.function.name"), None);
        assert_eq!(extract_call_subcategory("call.class"), None);
    }
}
