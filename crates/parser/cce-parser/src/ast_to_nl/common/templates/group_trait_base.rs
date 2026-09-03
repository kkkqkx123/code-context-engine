//! Base trait for entity group templates
//!
//! Provides common methods shared by both BM25 and Embedding templates
//! for member filtering and role management.

use crate::ast_to_nl::clean_comment_content;
use crate::grouper::types::{EntityGroup, MemberRole};
use cce_types::entity::{EntityId, GroupedEntity};
use cce_utils::normalize_whitespace_preserving_newlines;

/// Base trait for entity group templates
///
/// Provides common functionality for:
/// - Member role checking
/// - Significant member filtering
/// - Role-based description generation
/// - Shared text processing helpers
///
/// This trait is implemented by both BM25 and Embedding template traits
/// to avoid code duplication.
pub trait GroupTemplateBase {
    /// Check if a member should have an independent description
    ///
    /// Only `CoreMethod` and `SignificantMethod` roles get independent
    /// descriptions. `BoilerplateMethod` roles are compressed into
    /// the group description.
    fn should_generate_member_description(&self, role: &MemberRole) -> bool {
        role.has_independent_description()
    }

    /// Filter members that need independent descriptions
    ///
    /// Returns a vector of references to members that should have
    /// their own descriptions based on their roles.
    ///
    /// Uses O(1) HashMap lookup for role checking.
    fn filter_significant_members<'a>(&self, group: &'a EntityGroup) -> Vec<&'a GroupedEntity> {
        // Build role map for O(1) lookup
        let role_map = group.build_role_map();

        group
            .members
            .iter()
            .filter(|member| {
                role_map
                    .get(&member.id)
                    .map(|role| self.should_generate_member_description(role))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Get member role for a specific entity
    ///
    /// Returns the role assigned to a member, or None if not found.
    /// Uses O(1) HashMap lookup.
    fn get_member_role(&self, group: &EntityGroup, entity_id: &EntityId) -> Option<MemberRole> {
        group.build_role_map().get(entity_id).copied()
    }

    /// Check if a member is a boilerplate method
    ///
    /// Boilerplate methods are compressed into group descriptions
    /// and do not get independent descriptions.
    fn is_boilerplate_member(&self, group: &EntityGroup, entity_id: &EntityId) -> bool {
        self.get_member_role(group, entity_id)
            .map(|role| role.is_boilerplate())
            .unwrap_or(false)
    }

    /// Check if a member is a core method
    ///
    /// Core methods (e.g., Builder.build, Factory.create) always
    /// get independent descriptions.
    fn is_core_member(&self, group: &EntityGroup, entity_id: &EntityId) -> bool {
        self.get_member_role(group, entity_id)
            .map(|role| role.is_core())
            .unwrap_or(false)
    }

    /// Count significant members in a group
    ///
    /// Returns the number of members that should have independent descriptions.
    fn count_significant_members(&self, group: &EntityGroup) -> usize {
        self.filter_significant_members(group).len()
    }

    /// Clean doc comment text: strip comment markers and normalize whitespace.
    fn clean_doc_text(doc: &str) -> String {
        normalize_whitespace_preserving_newlines(&clean_comment_content(doc))
    }
}

/// Macro to implement GroupTemplateBase for a template struct
///
/// This macro simplifies the implementation of GroupTemplateBase
/// for template structs that don't need custom behavior.
///
/// # Example
///
/// ```ignore
/// use cce_parser::ast_to_nl::common::impl_group_template_base;
///
/// pub struct MyTemplate;
/// impl_group_template_base!(MyTemplate);
/// ```
#[macro_export]
macro_rules! impl_group_template_base {
    ($ty:ty) => {
        impl $crate::ast_to_nl::common::GroupTemplateBase for $ty {}
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock template for testing
    struct MockTemplate;

    impl GroupTemplateBase for MockTemplate {}

    #[test]
    fn test_should_generate_member_description() {
        let template = MockTemplate;

        assert!(template.should_generate_member_description(&MemberRole::SignificantMethod));
        assert!(template.should_generate_member_description(&MemberRole::CoreMethod));
        assert!(!template.should_generate_member_description(&MemberRole::BoilerplateMethod));
    }

    #[test]
    fn test_filter_significant_members() {
        let template = MockTemplate;
        let group = EntityGroup::default();

        // Empty group should have no significant members
        let significant = template.filter_significant_members(&group);
        assert!(significant.is_empty());
    }

    #[test]
    fn test_count_significant_members() {
        let template = MockTemplate;
        let group = EntityGroup::default();

        assert_eq!(template.count_significant_members(&group), 0);
    }
}
