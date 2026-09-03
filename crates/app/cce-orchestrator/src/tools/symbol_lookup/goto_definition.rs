//! Goto definition tool
//!
//! This tool finds the definition of a symbol using the relation index
//! and SQLite chunk storage for definition body code.

use std::sync::Arc;

use cce_relation::index::LayeredSnapshotIndex;
use cce_relation::index::snapshot_query::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotRelationQueryOps,
};
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::{ChunkRepository, EntityDetailMappingRepository};
use cce_types::{Entity, EntityId};

use super::types::{
    DefinitionCode, DefinitionLocation, GotoDefinitionRequest, GotoDefinitionResponse,
    SymbolLookupError,
};

/// Goto definition tool
pub struct GotoDefinitionTool {
    /// Published snapshot index (shared, read-only)
    index: Arc<LayeredSnapshotIndex>,
    /// Optional SQLite database for chunk content lookup
    sqlite: Option<Arc<SqliteClient>>,
    /// Project ID for SQLite queries
    project_id: i64,
}

impl GotoDefinitionTool {
    /// Create a new instance with a bound project ID
    pub fn new(index: Arc<LayeredSnapshotIndex>, project_id: i64) -> Self {
        Self {
            index,
            sqlite: None,
            project_id,
        }
    }

    /// Attach SQLite database for chunk content lookup
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite);
        self
    }

    /// Find the definition of a symbol
    pub fn goto_definition(
        &self,
        request: GotoDefinitionRequest,
    ) -> Result<GotoDefinitionResponse, SymbolLookupError> {
        let symbol_id = self.resolve_symbol_at_position(&request)?;

        let definitions = self.find_definitions(symbol_id)?;

        let definition_codes = definitions
            .into_iter()
            .map(|def| self.get_definition_code(def, request.include_body))
            .collect::<Result<Vec<_>, _>>()?;

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
        let entities: Vec<Entity> = self
            .index
            .get_entities_by_file(path)
            .into_iter()
            .map(|(_, entity)| entity)
            .collect();

        if entities.is_empty() {
            Err(SymbolLookupError::FileNotFound(path.to_string()))
        } else {
            Ok(entities)
        }
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
    fn get_definition_code(
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

    /// Get entity body from SQLite chunks by EntityId.
    ///
    /// Resolves the mapping and chunks with two-stage epoch resolution
    /// ("own first, miss → parent") so inherited generations stay readable.
    fn get_body_from_chunks(&self, entity_id: EntityId) -> Option<String> {
        let sqlite = self.sqlite.as_ref()?;
        let project_id = self.project_id;
        let conn = sqlite.write_connection().ok()?;

        let view = crate::query::filter::load_active_query_filter(&conn, project_id).ok()?;

        let resolve_mapping = |epoch: i64| {
            EntityDetailMappingRepository::get_by_entity_id_at_epoch(
                &conn,
                entity_id.0 as i64,
                project_id,
                epoch,
            )
            .ok()
            .flatten()
        };
        // Two-stage resolution: an own-generation miss falls back to the
        // inherited parent epoch (the own generation always wins because it
        // is probed first).
        let (mapping, chunk_epoch) = match resolve_mapping(view.epoch_value()) {
            Some(mapping) => (mapping, view.epoch_value()),
            None => {
                let parent_epoch = view.parent_epoch()?;
                let mapping = resolve_mapping(parent_epoch)?;
                (mapping, parent_epoch)
            }
        };
        let chunk_ids = mapping.get_qdrant_point_ids();
        if chunk_ids.is_empty() {
            return None;
        }

        let chunks =
            ChunkRepository::get_by_chunk_ids(&conn, &chunk_ids, project_id, Some(chunk_epoch))
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
        let mut sorted = chunks;
        sorted.sort_by_key(|c| c.start_line);
        let (first, last) = (sorted.first()?, sorted.last()?);
        // The body is lazy-loaded from the source file; chunk rows no longer
        // persist raw code.
        let project_root =
            cce_storage_sqlite::source_reader::resolve_project_root(&conn, project_id)?;
        Some(cce_storage_sqlite::source_reader::read_source_lines(
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
