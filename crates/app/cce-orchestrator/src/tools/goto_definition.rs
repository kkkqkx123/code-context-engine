//! Goto definition tool
//!
//! This tool finds the definition of a symbol using the relation index
//! and SQLite chunk storage for definition body code.

use std::sync::Arc;

use cce_codegraph::index::LayeredSnapshotIndex;
use cce_codegraph::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotRelationQueryOps};
use cce_storage_common::RelationStorage;
use cce_storage_common::source::read_source_lines;
use cce_types::{Entity, EntityId};

use crate::index::vector_store::RelationStore;
use crate::query::filter::load_active_query_filter_from_store;
use crate::tools::symbol_lookup_types::{
    DefinitionCode, DefinitionLocation, GotoDefinitionRequest, GotoDefinitionResponse,
    SymbolLookupError,
};

/// Goto definition tool
pub struct GotoDefinitionTool {
    /// Published snapshot index (shared, read-only)
    index: Arc<LayeredSnapshotIndex>,
    /// Relation backend for chunk content lookup (local branch only)
    relation: Option<RelationStore>,
    /// Project ID for relation queries
    project_id: i64,
}

impl GotoDefinitionTool {
    /// Create a new instance with a bound project ID
    pub fn new(index: Arc<LayeredSnapshotIndex>, project_id: i64) -> Self {
        Self {
            index,
            relation: None,
            project_id,
        }
    }

    /// Attach the relation backend for chunk content lookup.
    ///
    /// Reads go through the relation contract, so both branches share one
    /// read path and branch selection stays inside the enum.
    pub fn with_relation_store(mut self, store: RelationStore) -> Self {
        self.relation = Some(store);
        self
    }

    /// Find the definition of a symbol
    pub async fn goto_definition(
        &self,
        request: GotoDefinitionRequest,
    ) -> Result<GotoDefinitionResponse, SymbolLookupError> {
        let symbol_id = self.resolve_symbol_at_position(&request)?;

        let definitions = self.find_definitions(symbol_id)?;

        let mut definition_codes = Vec::with_capacity(definitions.len());
        for def in definitions {
            definition_codes.push(self.get_definition_code(def, request.include_body).await?);
        }

        Ok(GotoDefinitionResponse {
            symbol: request.symbol,
            definitions: definition_codes,
        })
    }

    /// Resolve symbol at position.
    ///
    /// Distinguishes call sites from definition sites:
    /// - If cursor is on a call expression, returns the callee's EntityId
    /// - If cursor is on an entity definition, returns that entity's own EntityId
    fn resolve_symbol_at_position(
        &self,
        request: &GotoDefinitionRequest,
    ) -> Result<EntityId, SymbolLookupError> {
        let entities = self.get_entities_by_file(&request.path)?;

        let enclosing = entities
            .iter()
            .find(|e| {
                e.span.start_position.row < request.line
                    && e.span.end_position.row + 1 >= request.line
            })
            .ok_or(SymbolLookupError::NoSymbolAtPosition)?;

        if let Some(relations) = self.index.get_resolved_relations_by_caller(enclosing.id) {
            for rel in relations.iter() {
                let line_matches = rel.span.start_position.row < request.line
                    && rel.span.end_position.row + 1 >= request.line;
                if line_matches {
                    if let Some(col) = request.column {
                        let col_matches =
                            if rel.span.start_position.row == rel.span.end_position.row {
                                rel.span.start_position.column < col
                                    && rel.span.end_position.column + 1 >= col
                            } else {
                                true
                            };
                        if !col_matches {
                            continue;
                        }
                    }
                    if let Some(callee_id) = rel.callee_id {
                        return Ok(callee_id);
                    }
                }
            }
        }

        Ok(enclosing.id)
    }

    /// Get entities by file path
    fn get_entities_by_file(&self, path: &str) -> Result<Vec<Entity>, SymbolLookupError> {
        crate::tools::common::get_entities_by_file(&self.index, path)
    }

    /// Find definition locations
    fn find_definitions(
        &self,
        symbol_id: EntityId,
    ) -> Result<Vec<DefinitionLocation>, SymbolLookupError> {
        let mut definitions = Vec::new();

        if let Some(entity) = self.index.get_function_by_entity_id(symbol_id) {
            let file_path = self
                .index
                .get_file_path_by_entity(symbol_id)
                .ok_or(SymbolLookupError::FileNotFound("Unknown file".to_string()))?;

            definitions.push(DefinitionLocation {
                path: file_path,
                entity_id: symbol_id,
                line: entity.span.start_position.row + 1,
                end_line: entity.span.end_position.row + 1,
            });
        }

        Ok(definitions)
    }

    /// Get definition code
    async fn get_definition_code(
        &self,
        location: DefinitionLocation,
        include_body: bool,
    ) -> Result<DefinitionCode, SymbolLookupError> {
        let entity = self
            .index
            .get_function_by_entity_id(location.entity_id)
            .ok_or(SymbolLookupError::EntityNotFound(location.entity_id))?;

        let code = if include_body {
            self.get_body_from_chunks(location.entity_id)
                .await
                .unwrap_or_else(|| entity.signature.clone())
        } else {
            entity.signature.clone()
        };

        Ok(DefinitionCode {
            location,
            name: entity.name.clone(),
            kind: entity.kind.into(),
            code,
            signature: entity.signature.clone(),
        })
    }

    /// Get entity body from relation-store chunks by EntityId.
    ///
    /// Resolves the mapping and chunks with two-stage epoch resolution
    /// ("own first, miss → parent") so inherited generations stay readable.
    async fn get_body_from_chunks(&self, entity_id: EntityId) -> Option<String> {
        let project_id = self.project_id;
        let store = self.relation.as_ref()?;

        let view = load_active_query_filter_from_store(store, project_id)
            .await
            .ok()?;

        // Two-stage resolution: an own-generation miss falls back to the
        // inherited parent epoch (the own generation always wins because it
        // is probed first).
        let (mapping, chunk_epoch) =
            match store
                .mappings_by_entity(project_id, entity_id.0 as i64, view.epoch_value())
                .await
                .ok()?
            {
                Some(mapping) => (mapping, view.epoch_value()),
                None => {
                    let parent_epoch = view.parent_epoch()?;
                    let mapping = store
                        .mappings_by_entity(project_id, entity_id.0 as i64, parent_epoch)
                        .await
                        .ok()??;
                    (mapping, parent_epoch)
                }
            };
        let chunk_ids = mapping.get_qdrant_point_ids();
        if chunk_ids.is_empty() {
            return None;
        }

        let mut chunks = store
            .chunks_by_ids(project_id, &chunk_ids, &[chunk_epoch])
            .await
            .ok()?;
        // Parent hits of overridden files stay hidden.
        if Some(chunk_epoch) == view.parent_epoch()
            && let Some(first) = chunks.first()
            && view
                .excluded_files()
                .iter()
                .any(|excluded| excluded == &first.file_path)
        {
            return None;
        }
        chunks.sort_by_key(|c| c.start_line);
        let (first, last) = (chunks.first()?, chunks.last()?);
        // The body is lazy-loaded from the source file; chunk rows no longer
        // persist raw code.
        let project_root = crate::tools::common::resolve_project_root(&self.relation, project_id)
            .await?;
        Some(read_source_lines(
            Some(project_root.as_path()),
            &first.file_path,
            first.start_line.max(0) as u32,
            last.end_line.max(0) as u32,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goto_definition_tool_creation() {
        let index = Arc::new(LayeredSnapshotIndex::empty());
        let _tool = GotoDefinitionTool::new(index, 1);
    }
}
