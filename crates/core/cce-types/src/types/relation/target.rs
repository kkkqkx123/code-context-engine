//! Relation target representation
//!
//! Represents the destination of a relation in various states:
//! - Unresolved: Only the name is known (parser phase)
//! - Resolved: Both EntityId and name are known (indexer phase)

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::super::entity::EntityId;

/// Target of a relation
///
/// Represents the destination of a relation, which can be either:
/// - Unresolved: Only the name is known (parser phase)
/// - Resolved: Both EntityId and name are known (indexer phase)
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub enum RelationTarget {
    /// Unresolved target (parser phase - only name is known)
    Unresolved(String),

    /// Resolved target (indexer phase - both ID and name are known)
    ///
    /// The ID is optional to handle external/unknown references.
    /// For external references, id will be None but name is kept for debugging.
    Resolved { id: Option<EntityId>, name: String },
}

impl RelationTarget {
    /// Get the target name
    pub fn name(&self) -> &str {
        match self {
            RelationTarget::Unresolved(name) => name,
            RelationTarget::Resolved { name, .. } => name,
        }
    }

    /// Get the target ID if available
    pub fn id(&self) -> Option<EntityId> {
        match self {
            RelationTarget::Unresolved(_) => None,
            RelationTarget::Resolved { id, .. } => *id,
        }
    }

    /// Check if this target is resolved
    pub fn is_resolved(&self) -> bool {
        matches!(self, RelationTarget::Resolved { .. })
    }

    /// Create an unresolved target
    pub fn unresolved(name: String) -> Self {
        Self::Unresolved(name)
    }

    /// Create a resolved target
    pub fn resolved(id: Option<EntityId>, name: String) -> Self {
        Self::Resolved { id, name }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_target_unresolved() {
        let target = RelationTarget::unresolved("foo".to_string());
        assert_eq!(target.name(), "foo");
        assert_eq!(target.id(), None);
        assert!(!target.is_resolved());
    }

    #[test]
    fn test_relation_target_resolved() {
        let target = RelationTarget::resolved(Some(EntityId(1)), "foo".to_string());
        assert_eq!(target.name(), "foo");
        assert_eq!(target.id(), Some(EntityId(1)));
        assert!(target.is_resolved());
    }

    #[test]
    fn test_relation_target_resolved_external() {
        let target = RelationTarget::resolved(None, "external".to_string());
        assert_eq!(target.name(), "external");
        assert_eq!(target.id(), None);
        assert!(target.is_resolved());
    }
}
