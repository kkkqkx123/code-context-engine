//! Language-specific pattern utilities
//!
//! This module provides language-specific patterns and rules for
//! method detection, naming conventions, etc.

use cce_types::language::Language;

/// Check if language has explicit self parameter
///
/// # Arguments
/// * `language` - The programming language
///
/// # Returns
/// `true` if the language uses explicit self/this parameters
pub fn has_explicit_self_parameter(language: &Language) -> bool {
    matches!(
        language,
        Language::Python | Language::Rust | Language::Cpp | Language::Php
    )
}

/// Check if a parameter type is a valid self reference
///
/// # Arguments
/// * `param_type` - The parameter type string
/// * `language` - The programming language
///
/// # Returns
/// `true` if the parameter type represents a self reference
pub fn is_self_reference(param_type: &str, language: &Language) -> bool {
    let type_lower = param_type.to_lowercase();
    match language {
        Language::Python => type_lower == "self",
        Language::Rust => type_lower == "&self" || type_lower == "&mut self",
        Language::Cpp => type_lower.contains("this"),
        Language::Php => type_lower == "$this" || type_lower.contains("$this"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_explicit_self_parameter() {
        assert!(!has_explicit_self_parameter(&Language::Java));
        assert!(has_explicit_self_parameter(&Language::Python));
        assert!(has_explicit_self_parameter(&Language::Rust));
    }

    #[test]
    fn test_is_self_reference() {
        assert!(is_self_reference("self", &Language::Python));
        assert!(is_self_reference("&self", &Language::Rust));
        assert!(is_self_reference("&mut self", &Language::Rust));
        assert!(!is_self_reference("other", &Language::Python));
    }
}
