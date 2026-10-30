//! Group template dispatcher for embedding templates
//!
//! Routes entity groups to one of three templates:
//! - a detected getter/setter pattern
//! - a standard library header
//! - the regular template
//!
//! # Architecture
//!
//! The dispatcher is the central component that routes entity groups
//! to the appropriate template for description generation. The routing rule
//! lives in [`GroupTemplateKind`] so the reported name and the template that
//! runs stay in sync.
//!
//! See docs/ast_to_nl/architecture.md for detailed design.

use super::getter_setter::GetterSetterTemplate;
use super::group_trait::{GroupTemplate, PatternGroupTemplate};
use super::regular::RegularGroupTemplate;
use super::stdlib::StdlibTemplate;
use crate::ast_to_nl::common::GroupTemplateKind;
use crate::grouper::types::EntityGroup;

/// Group template dispatcher
///
/// Routes entity groups to appropriate templates based on pattern information
/// and stdlib identity.
///
/// # Dispatch Logic
///
/// 1. A detected pattern uses its pattern-specific template
/// 2. Otherwise a stdlib header uses the stdlib template
/// 3. Otherwise the regular template
pub struct GroupTemplateDispatcher {
    // Getter/Setter pattern template
    getter_setter_template: GetterSetterTemplate,

    // Regular entity template
    regular_template: RegularGroupTemplate,

    // Standard library template
    stdlib_template: StdlibTemplate,
}

impl GroupTemplateDispatcher {
    /// Create a new GroupTemplateDispatcher
    pub fn new() -> Self {
        Self {
            getter_setter_template: GetterSetterTemplate::new(),
            regular_template: RegularGroupTemplate::new(),
            stdlib_template: StdlibTemplate::new(),
        }
    }

    /// Dispatch an entity group to the appropriate template
    ///
    /// Returns a vector of descriptions where:
    /// - First element: Group overall description
    /// - Subsequent elements: Independent member descriptions
    pub fn dispatch(&self, group: &EntityGroup) -> Vec<String> {
        match GroupTemplateKind::select(group) {
            GroupTemplateKind::GetterSetter(summary) => self
                .getter_setter_template
                .generate_with_pattern(group, summary),
            GroupTemplateKind::Stdlib => self.stdlib_template.generate(group),
            GroupTemplateKind::Regular => self.regular_template.generate(group),
        }
    }

    /// Name the template `dispatch` would pick for this group
    ///
    /// This labels the *template*, not the design pattern: `Stdlib` and
    /// `Regular` are not `PatternInfo` variants, and pattern identity is
    /// `PatternInfo::pattern_name`'s job (which can answer `None`). Answering
    /// "Regular" for every pattern-less group would claim the regular template
    /// ran when the stdlib one did, which is exactly the case worth counting.
    pub fn template_name(&self, group: &EntityGroup) -> &'static str {
        GroupTemplateKind::select(group).name()
    }
}

impl Default for GroupTemplateDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::{GetterSetterSummary, PatternInfo};

    fn stdlib_group() -> EntityGroup {
        EntityGroup {
            name: "std::collections::HashMap".into(),
            header: Some(cce_types::GroupedEntity {
                is_stdlib: true,
                stdlib_category: Some(cce_types::StdlibCategory::Collection),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_dispatcher_regular() {
        let group = EntityGroup::default();
        let dispatcher = GroupTemplateDispatcher::new();
        let results = dispatcher.dispatch(&group);

        assert!(!results.is_empty());
        assert_eq!(dispatcher.template_name(&group), "Regular");
    }

    /// The reported name has to name the template that actually produced the
    /// text, otherwise the label cannot be used to count template coverage.
    #[test]
    fn test_template_name_matches_dispatch() {
        let group = stdlib_group();
        let dispatcher = GroupTemplateDispatcher::new();

        assert_eq!(dispatcher.template_name(&group), "Stdlib");
        assert_eq!(
            dispatcher.dispatch(&group),
            StdlibTemplate::new().generate(&group)
        );

        let regular = EntityGroup::default();
        assert_eq!(dispatcher.template_name(&regular), "Regular");
        assert_eq!(
            dispatcher.dispatch(&regular),
            RegularGroupTemplate::new().generate(&regular)
        );

        let pattern = EntityGroup {
            pattern_info: PatternInfo::GetterSetter(GetterSetterSummary::empty()),
            ..Default::default()
        };
        assert_eq!(dispatcher.template_name(&pattern), "GetterSetter");
    }

    #[test]
    fn test_dispatcher_getter_setter_pattern() {
        let group = EntityGroup {
            pattern_info: PatternInfo::GetterSetter(GetterSetterSummary::new(vec![
                "name".to_string(),
            ])),
            ..Default::default()
        };

        let dispatcher = GroupTemplateDispatcher::new();
        let results = dispatcher.dispatch(&group);

        assert!(!results.is_empty());
        assert!(results[0].contains("data class"));
    }
}
