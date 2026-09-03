//! Name normalizer for converting programming naming conventions to natural language
//!
//! This module handles the conversion of various programming naming conventions
//! (snake_case, camelCase, PascalCase) to natural language words.

/// Name normalizer
pub struct NameNormalizer;

impl NameNormalizer {
    /// Normalize a name to natural language
    ///
    /// Detects the naming convention and converts to natural language words.
    ///
    /// # Examples
    ///
    /// ```
    /// use cce_parser::ast_to_nl::NameNormalizer;
    ///
    /// assert_eq!(NameNormalizer::normalize("await_ready_for_timeout"), "await ready for timeout");
    /// assert_eq!(NameNormalizer::normalize("calculateTotalPrice"), "calculate total price");
    /// assert_eq!(NameNormalizer::normalize("DatabaseConnection"), "database connection");
    /// ```
    pub fn normalize(name: &str) -> String {
        if name.is_empty() {
            return String::new();
        }

        // Detect naming convention
        if name.contains('_') || name.contains('-') {
            // snake_case, kebab-case, or SCREAMING_SNAKE_CASE
            Self::split_snake_case(name)
        } else if name.chars().any(|c| c.is_uppercase()) {
            // camelCase or PascalCase
            Self::split_camel_case(name)
        } else {
            // Already lowercase single word
            name.to_string()
        }
    }

    /// Split snake_case name into words
    ///
    /// Handles snake_case, kebab-case, and SCREAMING_SNAKE_CASE
    fn split_snake_case(name: &str) -> String {
        // Replace both underscores and hyphens as separators
        name.replace('-', "_")
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Split camelCase or PascalCase name into words with lowercase output
    ///
    /// Handles consecutive uppercase letters (e.g., "XMLParser" -> "xml parser")
    fn split_camel_case(name: &str) -> String {
        cce_utils::text::split_camel_case(name).to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_snake_case() {
        assert_eq!(
            NameNormalizer::normalize("await_ready_for_timeout"),
            "await ready for timeout"
        );
        assert_eq!(
            NameNormalizer::normalize("calculate_total_price"),
            "calculate total price"
        );
        assert_eq!(
            NameNormalizer::normalize("is_user_authenticated"),
            "is user authenticated"
        );
    }

    #[test]
    fn test_normalize_screaming_snake_case() {
        assert_eq!(
            NameNormalizer::normalize("MAX_CONNECTIONS"),
            "max connections"
        );
        assert_eq!(
            NameNormalizer::normalize("DEFAULT_TIMEOUT"),
            "default timeout"
        );
    }

    #[test]
    fn test_normalize_camel_case() {
        assert_eq!(
            NameNormalizer::normalize("calculateTotalPrice"),
            "calculate total price"
        );
        assert_eq!(
            NameNormalizer::normalize("isUserAuthenticated"),
            "is user authenticated"
        );
        assert_eq!(NameNormalizer::normalize("getUserById"), "get user by id");
    }

    #[test]
    fn test_normalize_pascal_case() {
        assert_eq!(
            NameNormalizer::normalize("DatabaseConnection"),
            "database connection"
        );
        assert_eq!(NameNormalizer::normalize("UserManager"), "user manager");
        assert_eq!(NameNormalizer::normalize("HttpRequest"), "http request");
    }

    #[test]
    fn test_normalize_with_acronyms() {
        // Acronyms like XML, HTTP, ID should be handled
        assert_eq!(NameNormalizer::normalize("XMLParser"), "xml parser");
        assert_eq!(NameNormalizer::normalize("parseHTML"), "parse html");
        assert_eq!(NameNormalizer::normalize("getUserID"), "get user id");
    }

    #[test]
    fn test_normalize_single_word() {
        assert_eq!(NameNormalizer::normalize("function"), "function");
        assert_eq!(NameNormalizer::normalize("test"), "test");
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(NameNormalizer::normalize(""), "");
    }

    #[test]
    fn test_normalize_mixed_underscore() {
        // Handle edge cases with multiple underscores
        assert_eq!(NameNormalizer::normalize("__private__"), "private");
        assert_eq!(NameNormalizer::normalize("_internal"), "internal");
    }

    #[test]
    fn test_normalize_kebab_case() {
        assert_eq!(
            NameNormalizer::normalize("my-variable-name"),
            "my variable name"
        );
        assert_eq!(NameNormalizer::normalize("with-value"), "with value");
    }
}
