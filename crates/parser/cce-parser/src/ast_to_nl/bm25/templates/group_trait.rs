//! BM25 group template trait definition
//!
//! Defines the core interface for BM25 templates optimized for keyword matching.
//!
//! # Key Differences from Embedding Templates
//!
//! - Returns single String instead of Vec<String>
//! - Includes original names (tokenizer emits whole-identifier tokens for
//!   spelling-accurate recall, alongside subword splits)
//! - Includes normalized names for fuzzy matching
//! - Includes keywords for keyword search
//!
//! # Design Principles
//!
//! - Preserve original names so the tokenizer produces whole-identifier tokens
//! - Normalize names for fuzzy matching
//! - Extract keywords for keyword search
//! - Compress boilerplate code
//!
//! # Architecture
//!
//! See docs/ast_to_nl/group_templates.md for detailed design.

use crate::ast_to_nl::common::GroupTemplateBase;
use crate::grouper::types::EntityGroup;

/// Core trait for BM25 entity group templates
///
/// Provides the interface for generating keyword-optimized text
/// from entity groups, with support for member filtering.
///
/// # Return Value
///
/// Returns a single String containing:
/// - Original names (tokenizer produces whole-identifier tokens for
///   spelling-accurate recall, plus subword splits)
/// - Normalized names (for fuzzy matching)
/// - Keywords (for keyword search)
/// - Group description
///
/// # Example
///
/// ```ignore
/// let template = BuilderBm25Template::new();
/// let text = template.generate(&entity_group);
/// // text = "UserBuilder builder User name email build construct"
/// ```
pub trait GroupTemplate: GroupTemplateBase {
    /// Generate keyword-optimized text for an entity group
    ///
    /// Returns a single string optimized for BM25 keyword matching,
    /// containing original names, normalized names, keywords, and description.
    fn generate(&self, group: &EntityGroup) -> String;
}

/// Pattern-aware BM25 group template trait
///
/// Extends GroupTemplate with pattern-specific generation capabilities.
/// Templates for detected patterns implement this trait to access
/// pattern summary information.
///
/// # Type Parameter
///
/// `Summary`: The pattern summary type containing pattern-specific metadata
pub trait PatternGroupTemplate<Summary>: GroupTemplate {
    /// Generate keyword-optimized text with pattern-specific information
    ///
    /// Called by the dispatcher when a pattern is detected,
    /// providing access to pattern summary metadata.
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &Summary) -> String;
}

/// Helper functions for BM25 text generation
///
/// These helpers wrap the common TemplateHelpers and NameNormalizer
/// for backward compatibility with existing BM25 template code.
pub mod helpers {
    use crate::ast_to_nl::common::{NameNormalizer, TemplateHelpers};

    /// Normalize a name for fuzzy matching
    ///
    /// Uses NameNormalizer for consistent normalization.
    pub fn normalize_name(name: &str) -> String {
        NameNormalizer::normalize(name).replace(' ', "_")
    }

    /// Extract keywords from a name
    ///
    /// Delegates to TemplateHelpers for consistent keyword extraction.
    pub fn extract_keywords(name: &str) -> Vec<String> {
        TemplateHelpers::extract_keywords(name)
    }

    /// Join text parts into a single string without deduplication.
    ///
    /// Preserves term frequency for BM25 ranking signal.
    pub fn join_parts(parts: &[&str]) -> String {
        TemplateHelpers::join_parts(parts)
    }

    /// Combine text parts into a single BM25-optimized string
    ///
    /// Delegates to TemplateHelpers for consistent text combination.
    pub fn combine_text(parts: &[&str]) -> String {
        TemplateHelpers::combine_text(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::MemberRole;

    // Mock template for testing
    struct MockTemplate;

    impl GroupTemplateBase for MockTemplate {}

    impl GroupTemplate for MockTemplate {
        fn generate(&self, _group: &EntityGroup) -> String {
            "mock group text".to_string()
        }
    }

    #[test]
    fn test_should_generate_member_description() {
        let template = MockTemplate;

        assert!(template.should_generate_member_description(&MemberRole::SignificantMethod));
        assert!(template.should_generate_member_description(&MemberRole::CoreMethod));
        assert!(!template.should_generate_member_description(&MemberRole::BoilerplateMethod));
    }

    #[test]
    fn test_normalize_name() {
        assert_eq!(helpers::normalize_name("UserBuilder"), "user_builder");
        assert_eq!(helpers::normalize_name("user_builder"), "user_builder");
        // NameNormalizer handles '_', '-', and camelCase
        assert_eq!(helpers::normalize_name("User-Builder!"), "user_builder!");
    }

    #[test]
    fn test_extract_keywords() {
        let keywords = helpers::extract_keywords("UserBuilder");
        assert_eq!(keywords, vec!["user", "builder"]);

        let keywords = helpers::extract_keywords("user_builder");
        assert_eq!(keywords, vec!["user", "builder"]);

        let keywords = helpers::extract_keywords("createUserAccount");
        assert_eq!(keywords, vec!["create", "user", "account"]);
    }

    #[test]
    fn test_combine_text() {
        let text = helpers::combine_text(&["User", "user", "Builder", "builder"]);
        assert_eq!(text, "user builder");
    }
}
