//! Relation enrichment bridge layer
//!
//! This module provides the bridge between chunk-based retrieval and entity-based relations.
//! It enables mapping from chunks (with line ranges) to entities, then to their relations.
//!
//! # Architecture
//!
//! ```text
//! Chunk Retrieval → Line Range Mapping → Entity Lookup → Relation Expansion → Enriched Result
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use crate::query::relation_bridge::RelationBridge;
//!
//! let bridge = RelationBridge::new(relation_index);
//! let enriched_chunks = bridge.enrich_chunks_with_relations(chunks, max_depth).await?;
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use cce_config::project_registry::ProjectScope;
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_relation::index::RelationIndex;
use cce_types::{EntityId, ResolvedRelation};

/// Relation enrichment configuration
#[derive(Debug, Clone)]
pub struct RelationEnrichmentConfig {
    /// Maximum depth for relation expansion (0 = unlimited)
    pub max_depth: usize,
    /// Maximum number of related entities per chunk
    pub max_relations_per_chunk: usize,
    /// Whether to include external relations
    pub include_external: bool,
    /// Relation types to include
    pub relation_types: Vec<cce_types::RelationType>,
}

impl Default for RelationEnrichmentConfig {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_relations_per_chunk: 10,
            include_external: false,
            relation_types: vec![
                cce_types::RelationType::DirectCall,
                cce_types::RelationType::InstanceMethodCall,
                cce_types::RelationType::StaticMethodCall,
            ],
        }
    }
}

/// Enriched chunk with relation context
#[derive(Debug, Clone)]
pub struct EnrichedChunk {
    /// Original chunk
    pub chunk: ChunkedResult,
    /// Entities found in this chunk's line range
    pub entities: Vec<EntityWithContext>,
    /// Related entities (callers/callees)
    pub related_entities: Vec<RelatedEntity>,
}

/// Entity with context information
#[derive(Debug, Clone)]
pub struct EntityWithContext {
    /// Entity ID
    pub entity_id: EntityId,
    /// Entity name
    pub entity_name: String,
    /// File path
    pub file_path: String,
    /// Line range
    pub start_line: usize,
    pub end_line: usize,
}

/// Related entity with relation information
#[derive(Debug, Clone)]
pub struct RelatedEntity {
    /// Entity ID
    pub entity_id: EntityId,
    /// Entity name
    pub entity_name: String,
    /// File path
    pub file_path: String,
    /// Relation type
    pub relation_type: cce_types::RelationType,
    /// Whether this is a caller or callee
    pub direction: RelationDirection,
    /// Call location span (if available)
    pub call_span: Option<cce_types::Span>,
}

/// Relation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    /// This entity calls the chunk entity
    Caller,
    /// This entity is called by the chunk entity
    Callee,
}

/// Backing index for the relation bridge.
#[derive(Debug, Clone)]
enum BridgeIndex {
    Mutable(Arc<RelationIndex>),
    Snapshot(Arc<cce_relation::index::snapshot_index::LayeredSnapshotIndex>),
}

/// Relation bridge for enriching chunks with relation context
///
/// Each bridge is bound to exactly one project's relation index/snapshot at
/// construction. The mutable variant is used during indexing, while the
/// snapshot variant reuses the same enrichment logic at query time without
/// requiring a mutable index.
pub struct RelationBridge {
    /// Immutable project scope
    scope: ProjectScope,
    /// Backing relation index (mutable or snapshot)
    index: BridgeIndex,
    /// Enrichment configuration
    config: RelationEnrichmentConfig,
}

impl RelationBridge {
    /// Create a new relation bridge, bound to a project scope and its index.
    pub fn new(scope: ProjectScope, relation_index: Arc<RelationIndex>) -> Self {
        Self {
            scope,
            index: BridgeIndex::Mutable(relation_index),
            config: RelationEnrichmentConfig::default(),
        }
    }

    /// Create a bridge from an immutable snapshot (query-time).
    pub fn from_snapshot(
        scope: ProjectScope,
        snapshot_index: Arc<cce_relation::index::snapshot_index::LayeredSnapshotIndex>,
    ) -> Self {
        Self {
            scope,
            index: BridgeIndex::Snapshot(snapshot_index),
            config: RelationEnrichmentConfig::default(),
        }
    }

    /// Create with custom configuration (mutable index)
    pub fn with_config(
        scope: ProjectScope,
        relation_index: Arc<RelationIndex>,
        config: RelationEnrichmentConfig,
    ) -> Self {
        Self {
            scope,
            index: BridgeIndex::Mutable(relation_index),
            config,
        }
    }

    /// Create with custom configuration from a snapshot (query-time)
    pub fn with_config_from_snapshot(
        scope: ProjectScope,
        snapshot_index: Arc<cce_relation::index::snapshot_index::LayeredSnapshotIndex>,
        config: RelationEnrichmentConfig,
    ) -> Self {
        Self {
            scope,
            index: BridgeIndex::Snapshot(snapshot_index),
            config,
        }
    }

    /// Whether this bridge is backed by a snapshot.
    pub fn is_snapshot_backed(&self) -> bool {
        matches!(self.index, BridgeIndex::Snapshot(_))
    }

    /// Enrich chunks with relation context
    ///
    /// For each chunk:
    /// 1. Find entities within the chunk's line range
    /// 2. Look up relations for those entities
    /// 3. Expand relations up to max_depth
    /// 4. Return enriched chunks
    ///
    /// # Errors
    ///
    /// Returns an error if `project_id` does not match this bridge's bound scope.
    pub async fn enrich_chunks(
        &self,
        chunks: &[ChunkedResult],
        project_id: i64,
    ) -> Result<Vec<EnrichedChunk>, Box<dyn std::error::Error + Send + Sync>> {
        if project_id != self.scope.project_id() {
            return Err(format!(
                "RelationBridge is bound to project {}, but enrich_chunks was called for project {}",
                self.scope.project_id(),
                project_id
            )
            .into());
        }
        let mut enriched_chunks = Vec::with_capacity(chunks.len());

        for chunk in chunks {
            let enriched = self.enrich_single_chunk(chunk, project_id).await?;
            enriched_chunks.push(enriched);
        }

        Ok(enriched_chunks)
    }

    fn get_entities_in_line_range(
        &self,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<EntityId> {
        use cce_relation::index::snapshot_query::SnapshotEntityQueryOps;
        match &self.index {
            BridgeIndex::Mutable(idx) => {
                <RelationIndex as SnapshotEntityQueryOps>::get_entities_in_line_range(
                    idx.as_ref(),
                    file_path,
                    start_line,
                    end_line,
                )
            }
            BridgeIndex::Snapshot(idx) => {
                idx.get_entities_in_line_range(file_path, start_line, end_line)
            }
        }
    }

    fn get_function_by_entity_id(&self, entity_id: EntityId) -> Option<cce_types::Entity> {
        use cce_relation::index::snapshot_query::SnapshotEntityQueryOps;
        match &self.index {
            BridgeIndex::Mutable(idx) => {
                <RelationIndex as SnapshotEntityQueryOps>::get_function_by_entity_id(
                    idx.as_ref(),
                    entity_id,
                )
            }
            BridgeIndex::Snapshot(idx) => idx.get_function_by_entity_id(entity_id),
        }
    }

    fn get_file_path_by_entity(&self, entity_id: EntityId) -> Option<String> {
        use cce_relation::index::snapshot_query::SnapshotEntityQueryOps;
        match &self.index {
            BridgeIndex::Mutable(idx) => {
                <RelationIndex as SnapshotEntityQueryOps>::get_file_path_by_entity(
                    idx.as_ref(),
                    entity_id,
                )
            }
            BridgeIndex::Snapshot(idx) => idx.get_file_path_by_entity(entity_id),
        }
    }

    fn get_resolved_relations_by_caller(
        &self,
        caller_id: EntityId,
    ) -> Option<Vec<ResolvedRelation>> {
        use cce_relation::index::snapshot_query::SnapshotRelationQueryOps;
        match &self.index {
            BridgeIndex::Mutable(idx) => {
                <RelationIndex as SnapshotRelationQueryOps>::get_resolved_relations_by_caller(
                    idx.as_ref(),
                    caller_id,
                )
            }
            BridgeIndex::Snapshot(idx) => idx.get_resolved_relations_by_caller(caller_id),
        }
    }

    fn get_callers_by_callee_entity(&self, callee_id: EntityId) -> Vec<EntityId> {
        use cce_relation::index::snapshot_query::SnapshotRelationQueryOps;
        match &self.index {
            BridgeIndex::Mutable(idx) => {
                <RelationIndex as SnapshotRelationQueryOps>::get_callers_by_callee_entity(
                    idx.as_ref(),
                    callee_id,
                )
            }
            BridgeIndex::Snapshot(idx) => idx.get_callers_by_callee_entity(callee_id),
        }
    }

    /// Enrich a single chunk with relation context
    async fn enrich_single_chunk(
        &self,
        chunk: &ChunkedResult,
        _project_id: i64,
    ) -> Result<EnrichedChunk, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = &chunk.metadata.file_path;
        let start_line = chunk.metadata.source_span.start_position.row;
        let end_line = chunk.metadata.source_span.end_position.row;

        // Step 1: Find entities within the chunk's line range
        let entity_ids = self.get_entities_in_line_range(file_path, start_line, end_line);

        // Step 2: Build entity context
        let mut entities = Vec::new();
        for entity_id in &entity_ids {
            if let Some(entity) = self.get_function_by_entity_id(*entity_id) {
                entities.push(EntityWithContext {
                    entity_id: *entity_id,
                    entity_name: entity.name.clone(),
                    file_path: file_path.clone(),
                    start_line: entity.span.start_position.row,
                    end_line: entity.span.end_position.row,
                });
            }
        }

        // Step 3: Find related entities (callers and callees)
        let related_entities = self.find_related_entities(&entity_ids).await;

        Ok(EnrichedChunk {
            chunk: chunk.clone(),
            entities,
            related_entities,
        })
    }

    /// Find related entities for a set of entity IDs
    async fn find_related_entities(&self, entity_ids: &[EntityId]) -> Vec<RelatedEntity> {
        let mut related = Vec::new();
        let mut seen_entities = HashSet::new();

        for &entity_id in entity_ids {
            // Get callees (entities called by this entity)
            if let Some(relations) = self.get_resolved_relations_by_caller(entity_id) {
                for relation in relations.iter() {
                    if !self.should_include_relation(relation) {
                        continue;
                    }

                    if let Some(callee_id) = relation.callee_id {
                        if seen_entities.insert(callee_id) {
                            if let Some(callee_info) = self.get_entity_info(callee_id) {
                                related.push(RelatedEntity {
                                    entity_id: callee_id,
                                    entity_name: callee_info.name,
                                    file_path: callee_info.file_path,
                                    relation_type: relation.relation_type,
                                    direction: RelationDirection::Callee,
                                    call_span: Some(relation.span),
                                });
                            }
                        }
                    }
                }
            }

            // Get callers (entities that call this entity)
            let callers = self.get_callers_by_callee_entity(entity_id);
            for &caller_id in &callers {
                if seen_entities.insert(caller_id) {
                    if let Some(caller_info) = self.get_entity_info(caller_id) {
                        // Try to get the relation to determine the relation type
                        let relation_type = self
                            .get_relation_type(caller_id, entity_id)
                            .unwrap_or(cce_types::RelationType::DirectCall);

                        related.push(RelatedEntity {
                            entity_id: caller_id,
                            entity_name: caller_info.name,
                            file_path: caller_info.file_path,
                            relation_type,
                            direction: RelationDirection::Caller,
                            call_span: None, // Would need to look up the specific relation
                        });
                    }
                }
            }
        }

        // Limit the number of related entities
        related.truncate(self.config.max_relations_per_chunk);
        related
    }

    /// Check if a relation should be included based on configuration
    fn should_include_relation(&self, relation: &ResolvedRelation) -> bool {
        // Check if external relations are included
        if !self.config.include_external && relation.is_external {
            return false;
        }

        // Check if relation type is in the allowed list
        if !self.config.relation_types.is_empty()
            && !self.config.relation_types.contains(&relation.relation_type)
        {
            return false;
        }

        true
    }

    /// Get entity information
    fn get_entity_info(&self, entity_id: EntityId) -> Option<EntityInfo> {
        self.get_function_by_entity_id(entity_id).map(|entity| {
            let file_path = self.get_file_path_by_entity(entity_id).unwrap_or_default();
            EntityInfo {
                name: entity.name.clone(),
                file_path,
            }
        })
    }

    /// Get relation type between two entities
    fn get_relation_type(
        &self,
        caller_id: EntityId,
        callee_id: EntityId,
    ) -> Option<cce_types::RelationType> {
        self.get_resolved_relations_by_caller(caller_id)
            .and_then(|relations| {
                relations
                    .iter()
                    .find(|r| r.callee_id == Some(callee_id))
                    .map(|r| r.relation_type)
            })
    }

    #[cfg(test)]
    pub fn as_relation_index(&self) -> Option<Arc<RelationIndex>> {
        match &self.index {
            BridgeIndex::Mutable(idx) => Some(Arc::clone(idx)),
            BridgeIndex::Snapshot(_) => None,
        }
    }
}

/// Entity information helper struct
struct EntityInfo {
    name: String,
    file_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::index::EntityIndexOps;
    use cce_types::relation::CallContext;
    use cce_types::{Entity, EntityKind, Span};
    use std::collections::HashMap;

    fn create_test_entity(id: u32, name: &str, start_line: usize, end_line: usize) -> Entity {
        Entity {
            id: EntityId(id.into()),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::new(0, 100, start_line, 0, end_line, 0),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        }
    }

    #[tokio::test]
    async fn test_relation_bridge_basic() {
        let index = RelationIndex::new();
        let index_arc = Arc::new(index);

        // Add test entities
        index_arc.add_function_with_path(
            EntityId(1),
            create_test_entity(1, "func_a", 10, 20),
            "test.rs".to_string(),
        );

        index_arc.add_function_with_path(
            EntityId(2),
            create_test_entity(2, "func_b", 30, 40),
            "test.rs".to_string(),
        );

        // Add a relation
        let relation = ResolvedRelation {
            caller: EntityId(1),
            callee_id: Some(EntityId(2)),
            callee_name: "func_b".to_string(),
            relation_type: cce_types::RelationType::DirectCall,
            span: Span::default(),
            is_external: false,
            external_type: None,
            callee_symbol: None,
            stdlib_category: None,
            owner_type: None,
            call_context: CallContext::Direct,
            overload_signature: None,
        };
        index_arc.add_resolved_relation(relation);

        let scope = ProjectScope::new(1, "relation-test-project").expect("valid scope");
        let bridge = RelationBridge::new(scope, Arc::clone(&index_arc));

        // Verify entity lookup works via the bridge (mutable backing)
        let entities = bridge.get_entities_in_line_range("test.rs", 5, 25);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0], EntityId(1));

        // Verify snapshot-backed bridge returns the same result
        let snapshot = Arc::new(
            cce_relation::index::snapshot_index::LayeredSnapshotIndex::new(Arc::new(
                cce_relation::index::snapshot_index::RelationSnapshotIndex::from_index_shared(
                    &index_arc,
                ),
            )),
        );
        let snapshot_bridge = RelationBridge::from_snapshot(
            ProjectScope::new(1, "relation-test-project").expect("valid scope"),
            snapshot,
        );
        let snap_entities = snapshot_bridge.get_entities_in_line_range("test.rs", 5, 25);
        assert_eq!(snap_entities.len(), 1);
        assert_eq!(snap_entities[0], EntityId(1));
    }

    #[tokio::test]
    async fn relation_bridge_rejects_cross_project_enrichment() {
        let scope = ProjectScope::new(1, "relation-test-project").expect("valid scope");
        let bridge = RelationBridge::new(scope, Arc::new(RelationIndex::new()));

        let result = bridge.enrich_chunks(&[], 2).await;

        assert!(result.is_err());
        assert!(
            result
                .expect_err("cross-project enrichment must fail")
                .to_string()
                .contains("bound to project 1")
        );
    }
}
