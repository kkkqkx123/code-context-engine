//! Relation enhancer for export
//!
//! This module provides functionality to enhance exported documents with
//! code relationship information.

use std::sync::Arc;

use cce_relation::index::{EntityIndexOps, RelationIndex, RelationQueryOps};
use cce_types::{ExternalCallType, RelationType, Span};

use super::aggregator::FileNlDocument;
use super::config::RelationEnhancerConfig;
use super::path_utils::paths_match;

/// Information about a relation for display
struct RelationInfo {
    name: String,
    relation_type: String,
    file_path: Option<String>,
    span: Option<Span>,
}

/// Relation enhancer
///
/// Enhances exported documents with code relationship information
/// from the relation index.
pub struct RelationEnhancer {
    /// Relation index (read-only reference)
    relation_index: Arc<RelationIndex>,
    /// Configuration
    config: RelationEnhancerConfig,
}

impl RelationEnhancer {
    /// Create a new relation enhancer
    pub fn new(relation_index: Arc<RelationIndex>, config: RelationEnhancerConfig) -> Self {
        Self {
            relation_index,
            config,
        }
    }

    /// Returns true if the given `relation_type` passes the include/exclude filters.
    fn passes_relation_type_filter(&self, relation_type: RelationType) -> bool {
        if let Some(ref include) = self.config.include_relation_types {
            return include.contains(&relation_type);
        }
        if let Some(ref exclude) = self.config.exclude_relation_types {
            return !exclude.contains(&relation_type);
        }
        true
    }

    /// Returns true if the given `external_type` passes the classification filters.
    /// Matching uses discriminant equality (ignores inner data fields).
    fn passes_classification_filter(&self, external_type: Option<&ExternalCallType>) -> bool {
        use std::mem::discriminant;
        let some_ext = match external_type {
            Some(e) => e,
            None => return true,
        };
        if let Some(ref include) = self.config.include_classifications {
            if !include
                .iter()
                .any(|c| discriminant(c) == discriminant(some_ext))
            {
                return false;
            }
        }
        if let Some(ref exclude) = self.config.exclude_classifications {
            if exclude
                .iter()
                .any(|c| discriminant(c) == discriminant(some_ext))
            {
                return false;
            }
        }
        true
    }

    /// Enhance a file document with relation information
    ///
    /// # Arguments
    ///
    /// * `doc` - File document to enhance (modified in place)
    pub fn enhance(&self, doc: &mut FileNlDocument) {
        for entity in &mut doc.entities {
            self.enhance_entity(entity, &doc.source_path);
        }
    }

    /// Enhance a single entity with relation information
    fn enhance_entity(&self, entity: &mut super::aggregator::EntityNlDocument, file_path: &str) {
        entity.related_entities = self.related_for_entity(&entity.name, file_path);
    }

    /// Query related entities for an entity name within a file.
    ///
    /// Returns a best-effort list of related entities (callers, callees,
    /// dependencies) sourced from the relation index. Used by both the
    /// chunk-based aggregation path and the direct export path.
    pub fn related_for_entity(
        &self,
        entity_name: &str,
        file_path: &str,
    ) -> Vec<super::aggregator::RelatedEntity> {
        let mut related = Vec::new();

        // Query relations for this entity
        if let Some(relations) = self.query_entity_relations(entity_name, file_path) {
            for relation in relations {
                // Filter by cross-file
                if !self.config.include_cross_file && relation.file_path.is_some() {
                    continue;
                }

                related.push(super::aggregator::RelatedEntity {
                    name: relation.name,
                    relation_type: relation.relation_type,
                    file_path: relation.file_path,
                    location: relation.span,
                });

                // Limit count
                if related.len() >= self.config.max_related_entities {
                    break;
                }
            }
        }

        related
    }

    /// Query relations for an entity by name
    ///
    /// # Arguments
    ///
    /// * `entity_name` - Name of the entity to query
    /// * `file_path` - File path for filtering entities (helps disambiguate same-named functions)
    fn query_entity_relations(
        &self,
        entity_name: &str,
        file_path: &str,
    ) -> Option<Vec<RelationInfo>> {
        // Try multiple name variants to improve matching with RelationIndex
        let name_variants = Self::generate_name_variants(entity_name);

        for name in &name_variants {
            if let Some(relations) = self.query_relations_for_name(name, file_path) {
                if !relations.is_empty() {
                    return Some(relations);
                }
            }
        }

        None
    }

    /// Generate name variants for flexible matching
    ///
    /// Produces multiple forms of an entity name to handle inconsistencies
    /// between different data sources (e.g., `MyClass::method` vs `method`).
    fn generate_name_variants(name: &str) -> Vec<String> {
        let mut variants = vec![name.to_string()];

        // Add version without module prefix (e.g., `MyClass::method` → `method`)
        if let Some(pos) = name.rfind("::") {
            variants.push(name[pos + 2..].to_string());
        }

        // Add normalized version (remove type parameters)
        if let Some(pos) = name.find('<') {
            variants.push(name[..pos].to_string());
        }

        variants
    }

    /// Query relations for a specific name variant
    fn query_relations_for_name(
        &self,
        entity_name: &str,
        file_path: &str,
    ) -> Option<Vec<RelationInfo>> {
        let mut relations = Vec::new();

        // Find functions by name and get their relations
        let entity_ids = self.relation_index.get_function_ids_by_name(entity_name);

        // Filter by file path if there are multiple matches
        let filtered_ids: Vec<_> = if entity_ids.len() > 1 {
            // When multiple entities have the same name, prefer the one in the current file
            entity_ids
                .into_iter()
                .filter(|id| {
                    if let Some(entity_path) = self.relation_index.get_file_path_by_entity(*id) {
                        // Use unified path matching to handle different formats
                        paths_match(&entity_path, file_path)
                    } else {
                        true // Keep if no path info available
                    }
                })
                .collect()
        } else {
            entity_ids
        };

        for entity_id in filtered_ids {
            // Get resolved relations for this function
            if let Some(relations_ref) = self
                .relation_index
                .get_resolved_relations_by_caller(entity_id)
            {
                for r in relations_ref.value() {
                    // Filter stdlib relations if configured
                    if !self.config.include_stdlib
                        && matches!(
                            r.external_type,
                            Some(ExternalCallType::StandardLibrary { .. })
                        )
                    {
                        continue;
                    }

                    // Apply relation type filter
                    if !self.passes_relation_type_filter(r.relation_type) {
                        continue;
                    }

                    // Apply classification filter
                    if !self.passes_classification_filter(r.external_type.as_ref()) {
                        continue;
                    }

                    // Get file path for callee if available
                    let callee_file_path = r
                        .callee_id
                        .and_then(|id| self.relation_index.get_file_path_by_entity(id));

                    // Add call relations
                    if r.relation_type.is_call() {
                        relations.push(RelationInfo {
                            name: r.callee_name.clone(),
                            relation_type: "calls".to_string(),
                            file_path: callee_file_path.clone(),
                            span: Some(r.span),
                        });
                    }

                    // Add type dependencies
                    if matches!(
                        r.relation_type,
                        RelationType::TypeReference | RelationType::FieldAccess
                    ) {
                        relations.push(RelationInfo {
                            name: r.callee_name.clone(),
                            relation_type: "uses".to_string(),
                            file_path: callee_file_path,
                            span: Some(r.span),
                        });
                    }
                }
            }

            // Get callers (reverse lookup)
            let callers = self.relation_index.get_callers_by_callee_entity(entity_id);
            for caller_id in callers {
                if let Some(caller) = self.relation_index.get_function_by_entity_id(caller_id) {
                    // Get file path for caller
                    let caller_file_path = self.relation_index.get_file_path_by_entity(caller_id);

                    relations.push(RelationInfo {
                        name: caller.value().name.clone(),
                        relation_type: "called by".to_string(),
                        file_path: caller_file_path,
                        span: None,
                    });
                }
            }
        }

        if relations.is_empty() {
            None
        } else {
            Some(relations)
        }
    }
}
