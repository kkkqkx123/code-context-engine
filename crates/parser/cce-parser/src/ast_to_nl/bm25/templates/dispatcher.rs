//! BM25 group template dispatcher
//!
//! Routes entity groups to appropriate BM25 templates based on pattern information.

use super::getter_setter::GetterSetterTemplate;
use super::group_trait::{GroupTemplate, PatternGroupTemplate};
use super::regular::RegularGroupTemplate;
use super::stdlib::StdlibTemplate;
use crate::grouper::types::{EntityGroup, PatternInfo};

/// BM25 group template dispatcher
///
/// Routes entity groups to appropriate templates for keyword-optimized text generation.
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
    /// Returns a keyword-optimized string for BM25 matching.
    pub fn dispatch(&self, group: &EntityGroup) -> String {
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
        let text = dispatcher.dispatch(&group);

        assert!(!text.is_empty());
    }

    #[test]
    fn test_dispatcher_getter_setter_pattern() {
        let group = EntityGroup {
            name: "User".into(),
            pattern_info: PatternInfo::GetterSetter(
                crate::grouper::types::GetterSetterSummary::new(vec!["name".to_string()]),
            ),
            ..Default::default()
        };

        let dispatcher = GroupTemplateDispatcher::new();
        let text = dispatcher.dispatch(&group);

        assert!(text.contains("data"));
        assert!(text.contains("name"));
    }
}
