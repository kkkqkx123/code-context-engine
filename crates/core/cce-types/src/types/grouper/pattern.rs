use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::types::entity::EntityId;

pub use super::design_pattern::GetterSetterSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemberRole {
    SignificantMethod,
    BoilerplateMethod,
    CoreMethod,
}

impl std::fmt::Display for MemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemberRole::SignificantMethod => write!(f, "significant_method"),
            MemberRole::BoilerplateMethod => write!(f, "boilerplate_method"),
            MemberRole::CoreMethod => write!(f, "core_method"),
        }
    }
}

impl MemberRole {
    pub fn has_independent_description(&self) -> bool {
        matches!(self, MemberRole::SignificantMethod | MemberRole::CoreMethod)
    }

    pub fn is_boilerplate(&self) -> bool {
        matches!(self, MemberRole::BoilerplateMethod)
    }

    pub fn is_core(&self) -> bool {
        matches!(self, MemberRole::CoreMethod)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum PatternInfo {
    #[default]
    None,
    GetterSetter(GetterSetterSummary),
}

impl PatternInfo {
    pub fn is_getter_setter(&self) -> bool {
        matches!(self, PatternInfo::GetterSetter(_))
    }

    pub fn as_getter_setter(&self) -> Option<&GetterSetterSummary> {
        match self {
            PatternInfo::GetterSetter(summary) => Some(summary),
            _ => None,
        }
    }

    pub fn pattern_name(&self) -> &'static str {
        match self {
            PatternInfo::None => "None",
            PatternInfo::GetterSetter(_) => "GetterSetter",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemberRolesBuilder {
    roles: SmallVec<[(EntityId, MemberRole); 8]>,
}

impl MemberRolesBuilder {
    pub fn new() -> Self {
        Self {
            roles: SmallVec::new(),
        }
    }

    pub fn mark_significant(&mut self, entity_id: EntityId) {
        self.roles.retain(|(id, _)| *id != entity_id);
        self.roles.push((entity_id, MemberRole::SignificantMethod));
    }

    pub fn mark_boilerplate(&mut self, entity_id: EntityId) {
        self.roles.retain(|(id, _)| *id != entity_id);
        self.roles.push((entity_id, MemberRole::BoilerplateMethod));
    }

    pub fn mark_core(&mut self, entity_id: EntityId) {
        self.roles.retain(|(id, _)| *id != entity_id);
        self.roles.push((entity_id, MemberRole::CoreMethod));
    }

    pub fn build(self) -> SmallVec<[(EntityId, MemberRole); 8]> {
        self.roles
    }

    pub fn len(&self) -> usize {
        self.roles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }
}

pub fn get_member_role<'a>(
    roles: &'a SmallVec<[(EntityId, MemberRole); 8]>,
    entity_id: &EntityId,
) -> Option<&'a MemberRole> {
    roles
        .iter()
        .find(|(id, _)| id == entity_id)
        .map(|(_, role)| role)
}
