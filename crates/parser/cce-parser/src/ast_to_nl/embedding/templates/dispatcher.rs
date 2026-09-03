//! Group template dispatcher for embedding templates
//!
//! Dispatches entity groups to appropriate templates based on:
//! - PatternInfo (getter/setter pattern)
//! - GroupType (regular entities)
//!
//! # Architecture
//!
//! The dispatcher is the central component that routes entity groups
//! to the appropriate template for description generation.
//!
//! See docs/ast_to_nl/architecture.md for detailed design.

use super::getter_setter::GetterSetterTemplate;
use super::group_trait::{GroupTemplate, PatternGroupTemplate};
use super::regular::RegularGroupTemplate;
use super::stdlib::StdlibTemplate;
use crate::grouper::types::{EntityGroup, PatternInfo};

/// Group template dispatcher
///
/// Routes entity groups to appropriate templates based on pattern information
/// and group characteristics.
///
/// # Dispatch Logic
///
/// 1. If PatternInfo is not None, use pattern-specific template
/// 2. Otherwise, use regular template
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
        match &group.pattern_info {
            PatternInfo::GetterSetter(summary) => self
                .getter_setter_template
                .generate_with_pattern(group, summary),
            // No pattern detected — check for stdlib
            PatternInfo::None => {
                if group.header.as_ref().is_some_and(|h| h.is_stdlib) {
                    self.stdlib_template.generate(group)
                } else {
                    self.regular_template.generate(group)
                }
            }
        }
    }

    /// Get the pattern name for a group
    ///
    /// Returns the pattern name if a pattern is detected,
    /// or "Regular" otherwise.
    pub fn get_pattern_name(&self, group: &EntityGroup) -> &'static str {
        if !matches!(group.pattern_info, PatternInfo::None) {
            group.pattern_info.pattern_name()
        } else {
            "Regular"
        }
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

    #[test]
    fn test_dispatcher_regular() {
        let group = EntityGroup::default();
        let dispatcher = GroupTemplateDispatcher::new();
        let results = dispatcher.dispatch(&group);

        assert!(!results.is_empty());
    }

    #[test]
    fn test_dispatcher_getter_setter_pattern() {
        let group = EntityGroup {
            pattern_info: PatternInfo::GetterSetter(
                crate::grouper::types::GetterSetterSummary::new(vec!["name".to_string()]),
            ),
            ..Default::default()
        };

        let dispatcher = GroupTemplateDispatcher::new();
        let results = dispatcher.dispatch(&group);

        assert!(!results.is_empty());
        assert!(results[0].contains("data class"));
    }

    #[test]
    fn test_get_pattern_name() {
        let group = EntityGroup {
            pattern_info: PatternInfo::GetterSetter(
                crate::grouper::types::GetterSetterSummary::empty(),
            ),
            ..Default::default()
        };

        let dispatcher = GroupTemplateDispatcher::new();
        assert_eq!(dispatcher.get_pattern_name(&group), "GetterSetter");

        let group = EntityGroup::default();
        assert_eq!(dispatcher.get_pattern_name(&group), "Regular");
    }
}
