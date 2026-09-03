//! Relation capture for persistence across parsing sessions
//!
//! Captures relations immediately after parsing for stable, source-code-level
//! identification and cross-session persistence.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::super::super::Span;
use super::super::super::entity::EntityKind;
use super::super::classification::RelationType;

/// Relation capture record for persistence
///
/// Captures relations immediately after parsing for stable tracking across
/// sessions. This enables:
/// - Stable, source-code-level identification using VirtualSymbolId
/// - Asynchronous symbol resolution
/// - Cross-session persistence without EntityId dependency
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct RelationCapture {
    /// Unique identifier for this captured relation
    pub relation_id: String,
    /// File path being parsed
    pub file_path: String,
    /// Scoped name of the calling entity (e.g., "Module::Class::method")
    pub caller_scoped_name: String,
    /// Entity kind of the caller
    pub caller_kind: EntityKind,
    /// Target name as written in source code
    pub raw_target_name: String,
    /// Type of relation
    pub relation_type: RelationType,
    /// Source code location
    pub span: Span,
    /// Symbol table version at capture time
    pub symbol_version: u64,
    /// Timestamp when captured
    pub created_at: String,
}

impl RelationCapture {
    /// Create a new relation capture record
    pub fn new(
        file_path: String,
        caller_scoped_name: String,
        caller_kind: EntityKind,
        raw_target_name: String,
        relation_type: RelationType,
        span: Span,
        symbol_version: u64,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        file_path.hash(&mut hasher);
        caller_scoped_name.hash(&mut hasher);
        raw_target_name.hash(&mut hasher);
        span.start_byte.hash(&mut hasher);
        let hash = hasher.finish();

        Self {
            relation_id: format!("{:x}", hash),
            file_path,
            caller_scoped_name,
            caller_kind,
            raw_target_name,
            relation_type,
            span,
            symbol_version,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_capture_creation() {
        let capture = RelationCapture::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            "helper".to_string(),
            RelationType::DirectCall,
            Span::default(),
            1,
        );

        assert_eq!(capture.file_path, "src/main.rs");
        assert_eq!(capture.caller_scoped_name, "main");
        assert_eq!(capture.caller_kind, EntityKind::Function);
        assert_eq!(capture.raw_target_name, "helper");
        assert_eq!(capture.relation_type, RelationType::DirectCall);
        assert_eq!(capture.symbol_version, 1);
        assert!(!capture.relation_id.is_empty());
    }

    #[test]
    fn test_relation_capture_deterministic_id() {
        let capture1 = RelationCapture::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            "helper".to_string(),
            RelationType::DirectCall,
            Span::default(),
            1,
        );

        let capture2 = RelationCapture::new(
            "src/main.rs".to_string(),
            "main".to_string(),
            EntityKind::Function,
            "helper".to_string(),
            RelationType::DirectCall,
            Span::default(),
            1,
        );

        assert_eq!(capture1.relation_id, capture2.relation_id);
    }
}
