//! Group template trait definition for embedding templates
//!
//! Defines the core interface for all entity group templates used in
//! semantic summary generation optimized for vector embedding.
//!
//! # Design Principles
//!
//! - Entity group oriented: Generate descriptions for groups, not individual entities
//! - Boilerplate compression: Compress boilerplate code into group descriptions
//! - Role filtering: Filter member descriptions based on MemberRole
//! - No counting: Use semantic descriptions instead of counting information
//!
//! # Architecture
//!
//! See docs/ast_to_nl/group_templates.md for detailed design.

use crate::ast_to_nl::common::GroupTemplateBase;
use crate::grouper::types::EntityGroup;

/// Core trait for entity group templates
///
/// Provides the interface for generating natural language descriptions
/// from entity groups, with support for member filtering based on roles.
///
/// # Return Value
///
/// Returns `Vec<String>` where:
/// - First element: Entity group overall description
/// - Subsequent elements: Independent descriptions for significant members
///
/// # Example
///
/// ```ignore
/// let template = BuilderTemplate::new();
/// let descriptions = template.generate(&entity_group);
/// // descriptions[0] = "Builder for creating User instances..."
/// // descriptions[1] = "Constructs and returns a User instance..."
/// ```
pub trait GroupTemplate: GroupTemplateBase {
    /// Generate descriptions for an entity group
    ///
    /// Returns a vector of strings where the first element is the
    /// group's overall description, and subsequent elements are
    /// independent descriptions for significant members.
    fn generate(&self, group: &EntityGroup) -> Vec<String>;
}

/// Pattern-aware group template trait
///
/// Extends GroupTemplate with pattern-specific generation capabilities.
/// Templates for detected patterns (Builder, Factory, etc.) implement
/// this trait to access pattern summary information.
///
/// # Type Parameter
///
/// `Summary`: The pattern summary type containing pattern-specific metadata
///
/// # Example
///
/// ```ignore
/// impl PatternGroupTemplate<BuilderSummary> for BuilderTemplate {
///     fn generate_with_pattern(&self, group: &EntityGroup, summary: &BuilderSummary) -> Vec<String> {
///         // Generate Builder-specific description
///     }
/// }
/// ```
pub trait PatternGroupTemplate<Summary>: GroupTemplate {
    /// Generate descriptions with pattern-specific information
    ///
    /// Called by the dispatcher when a pattern is detected,
    /// providing access to pattern summary metadata.
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &Summary) -> Vec<String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::MemberRole;

    // Mock template for testing
    struct MockTemplate;

    impl GroupTemplateBase for MockTemplate {}

    impl GroupTemplate for MockTemplate {
        fn generate(&self, _group: &EntityGroup) -> Vec<String> {
            vec!["Mock group description".to_string()]
        }
    }

    #[test]
    fn test_should_generate_member_description() {
        let template = MockTemplate;

        assert!(template.should_generate_member_description(&MemberRole::SignificantMethod));
        assert!(template.should_generate_member_description(&MemberRole::CoreMethod));
        assert!(!template.should_generate_member_description(&MemberRole::BoilerplateMethod));
    }
}
