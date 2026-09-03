//! Getter/Setter group template (embedding)
//!
//! Getter/Setter is the only spec-based pattern retained in the grouper:
//! simple `getX`/`setX`/property methods matching a field are merged into
//! their class and summarized here. Framework/design patterns are delegated
//! to the plugin system.

use super::group_trait::{GroupTemplate, PatternGroupTemplate};
use crate::ast_to_nl::common::GroupTemplateBase;
use crate::grouper::types::{EntityGroup, GetterSetterSummary};

/// Getter/Setter pattern template
pub struct GetterSetterTemplate;

impl GetterSetterTemplate {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GetterSetterTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupTemplateBase for GetterSetterTemplate {}

impl GroupTemplate for GetterSetterTemplate {
    fn generate(&self, group: &EntityGroup) -> Vec<String> {
        vec![format!("{} data class.", group.name)]
    }
}

impl PatternGroupTemplate<GetterSetterSummary> for GetterSetterTemplate {
    fn generate_with_pattern(
        &self,
        group: &EntityGroup,
        summary: &GetterSetterSummary,
    ) -> Vec<String> {
        let properties_str = if summary.properties.is_empty() {
            "no properties".to_string()
        } else {
            summary.properties.join(", ")
        };

        let group_desc = format!(
            "{} data class with properties: {}. Provides standard getters and setters.",
            group.name, properties_str
        );

        // All getters/setters are boilerplate, no independent descriptions
        vec![group_desc]
    }
}
