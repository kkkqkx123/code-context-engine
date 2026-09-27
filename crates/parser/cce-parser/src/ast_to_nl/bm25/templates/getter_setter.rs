//! Getter/Setter group template (BM25)
//!
//! Getter/Setter is the only spec-based pattern retained in the grouper:
//! simple `getX`/`setX`/property methods matching a field are merged into
//! their class and summarized here. Framework/design patterns are delegated
//! to the plugin system.

use super::group_trait::GroupTemplate;
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
    fn generate(&self, group: &EntityGroup) -> String {
        format!("{} (getter/setter).", group.name)
    }
}

impl super::group_trait::PatternGroupTemplate<GetterSetterSummary> for GetterSetterTemplate {
    fn generate_with_pattern(&self, group: &EntityGroup, summary: &GetterSetterSummary) -> String {
        let name = group.name.as_str();
        if summary.properties.is_empty() {
            format!("{} (getter/setter).", name)
        } else {
            format!("{} (getter/setter) {}.", name, summary.properties.join(" "))
        }
    }
}
