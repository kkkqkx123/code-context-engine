//! Virtual relation representation with persistent symbol identification
//!
//! Represents relations using source-code-level symbol identification,
//! enabling stable tracking across parsing sessions without EntityId dependency.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::super::classification::{ExternalCallType, RelationType};
use super::symbol::VirtualSymbolId;
use super::verification::RelationVerificationStatus;

/// Virtual Relation - persistent relation record
///
/// Represents a relation using source-code-level symbol identification
/// (VirtualSymbolId) instead of ephemeral EntityId. This enables stable
/// relation tracking across parsing sessions.
///
/// Target resolution states:
/// - None: Unresolved (awaiting symbol resolution)
/// - Some(None): External reference
/// - Some(Some(vsid)): Project-internal symbol
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct VirtualRelation {
    /// Relation ID (UUID)
    pub relation_id: String,
    /// Calling symbol
    pub caller: VirtualSymbolId,
    /// Target symbol (None=unresolved, Some(None)=external, Some(Some(vsid))=project internal)
    pub target: Option<Option<VirtualSymbolId>>,
    /// Raw target name from source code
    pub raw_target_name: String,
    /// Relation type
    pub relation_type: RelationType,
    /// Verification status
    pub verification_status: RelationVerificationStatus,
    /// External call type (if target is None)
    pub external_type: Option<ExternalCallType>,
    /// Timestamp
    pub created_at: String,
    /// Last verification timestamp
    pub last_verified: Option<String>,
}

impl VirtualRelation {
    /// Create a new virtual relation with unresolved target
    pub fn new(
        relation_id: String,
        caller: VirtualSymbolId,
        raw_target_name: String,
        relation_type: RelationType,
    ) -> Self {
        Self {
            relation_id,
            caller,
            target: None,
            raw_target_name,
            relation_type,
            verification_status: RelationVerificationStatus::Verified,
            external_type: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_verified: None,
        }
    }

    /// Set target as external
    pub fn with_external_type(mut self, external_type: ExternalCallType) -> Self {
        self.target = Some(None);
        self.external_type = Some(external_type);
        self.last_verified = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    /// Set target as project-internal symbol
    pub fn with_target(mut self, target: VirtualSymbolId) -> Self {
        self.target = Some(Some(target));
        self.last_verified = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    /// Check if target is resolved
    pub fn is_target_resolved(&self) -> bool {
        self.target.is_some()
    }

    /// Check if target is external
    pub fn is_external(&self) -> bool {
        matches!(self.target, Some(None))
    }

    /// Check if target is project-internal
    pub fn is_internal(&self) -> bool {
        matches!(self.target, Some(Some(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::entity::EntityKind;
    use super::*;

    #[test]
    fn test_virtual_relation_creation() {
        let caller = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            1,
        );

        let relation = VirtualRelation::new(
            "rel_001".to_string(),
            caller.clone(),
            "foo".to_string(),
            RelationType::DirectCall,
        );

        assert_eq!(relation.relation_id, "rel_001");
        assert_eq!(relation.caller, caller);
        assert_eq!(relation.raw_target_name, "foo");
        assert_eq!(relation.relation_type, RelationType::DirectCall);
        assert!(!relation.is_target_resolved());
    }

    #[test]
    fn test_virtual_relation_resolve_external() {
        let caller = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            1,
        );

        let relation = VirtualRelation::new(
            "rel_001".to_string(),
            caller,
            "println".to_string(),
            RelationType::DirectCall,
        );

        let resolved = relation.with_external_type(ExternalCallType::standard_library("std"));

        assert!(resolved.is_target_resolved());
        assert!(resolved.is_external());
        assert!(!resolved.is_internal());
        assert!(resolved.external_type.is_some());
    }

    #[test]
    fn test_virtual_relation_resolve_internal() {
        let caller = VirtualSymbolId::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            1,
        );

        let target = VirtualSymbolId::new(
            "src/lib.rs".to_string(),
            "Module::helper".to_string(),
            EntityKind::Function,
            1,
        );

        let relation = VirtualRelation::new(
            "rel_001".to_string(),
            caller,
            "helper".to_string(),
            RelationType::DirectCall,
        );

        let resolved = relation.with_target(target.clone());

        assert!(resolved.is_target_resolved());
        assert!(!resolved.is_external());
        assert!(resolved.is_internal());
        assert_eq!(resolved.target, Some(Some(target)));
    }
}
