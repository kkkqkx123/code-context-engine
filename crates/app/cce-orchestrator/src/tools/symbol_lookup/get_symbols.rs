//! Get symbols tool
//!
//! This tool retrieves all symbols from files using the relation index.

use std::collections::HashMap;
use std::sync::Arc;

use cce_relation::index::LayeredSnapshotIndex;
use cce_relation::index::snapshot_query::SnapshotFileQueryOps;
use cce_types::{Entity, EntityId};

use super::types::{
    FileSymbolResult, GetSymbolsRequest, GetSymbolsResponse, SymbolInfo, SymbolLookupError,
};

/// Get symbols tool
pub struct GetSymbolsTool {
    /// Published snapshot index (shared, read-only)
    index: Arc<LayeredSnapshotIndex>,
}

impl GetSymbolsTool {
    /// Create a new instance
    pub fn new(index: Arc<LayeredSnapshotIndex>) -> Self {
        Self { index }
    }

    /// Get symbols from files
    pub fn get_symbols(
        &self,
        request: GetSymbolsRequest,
    ) -> Result<GetSymbolsResponse, SymbolLookupError> {
        let mut results = Vec::new();

        for path in &request.paths {
            let result = self.get_symbols_for_file(path);
            results.push(result);
        }

        let success_count = results.iter().filter(|r| r.success).count();
        let fail_count = results.len() - success_count;

        Ok(GetSymbolsResponse {
            results,
            success_count,
            fail_count,
        })
    }

    /// Get symbols for a single file
    fn get_symbols_for_file(&self, path: &str) -> FileSymbolResult {
        // Try to get entities from the index
        let entities = self.get_entities_from_index(path);

        if !entities.is_empty() {
            // Build symbol tree from indexed entities
            match self.build_symbol_tree(&entities) {
                Ok(symbols) => {
                    return FileSymbolResult {
                        path: path.to_string(),
                        success: true,
                        symbol_count: Some(symbols.len()),
                        symbols: Some(symbols),
                        error: None,
                    };
                }
                Err(e) => {
                    return FileSymbolResult {
                        path: path.to_string(),
                        success: false,
                        symbol_count: None,
                        symbols: None,
                        error: Some(e.to_string()),
                    };
                }
            }
        }

        // No entities in index
        FileSymbolResult {
            path: path.to_string(),
            success: false,
            symbol_count: None,
            symbols: None,
            error: Some("No entities found in index".to_string()),
        }
    }

    /// Get entities from the index for a file
    fn get_entities_from_index(&self, path: &str) -> Vec<Entity> {
        self.index
            .get_entities_by_file(path)
            .into_iter()
            .map(|(_, entity)| entity)
            .collect()
    }

    /// Build symbol tree from entities
    fn build_symbol_tree(&self, entities: &[Entity]) -> Result<Vec<SymbolInfo>, SymbolLookupError> {
        // Build parent-child relationship map
        let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut root_ids = Vec::new();

        for entity in entities {
            if let Some(parent_id) = entity.parent {
                children_map.entry(parent_id).or_default().push(entity.id);
            } else {
                root_ids.push(entity.id);
            }
        }

        // Recursively build symbol tree
        let symbols = root_ids
            .into_iter()
            .filter_map(|id| self.build_symbol_node(id, entities, &children_map))
            .collect();

        Ok(symbols)
    }

    /// Build a symbol node recursively
    fn build_symbol_node(
        &self,
        entity_id: EntityId,
        entities: &[Entity],
        children_map: &HashMap<EntityId, Vec<EntityId>>,
    ) -> Option<SymbolInfo> {
        let entity = entities.iter().find(|e| e.id == entity_id)?;

        // Recursively build children
        let children: Vec<SymbolInfo> = children_map
            .get(&entity_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.build_symbol_node(*id, entities, children_map))
                    .collect()
            })
            .unwrap_or_default();

        Some(SymbolInfo {
            name: entity.name.clone(),
            kind: entity.kind.into(),
            line: entity.span.start_position.row + 1,
            end_line: entity.span.end_position.row + 1,
            detail: Some(entity.signature.clone()),
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_symbols_tool_creation() {
        let index = Arc::new(LayeredSnapshotIndex::empty());
        let _tool = GetSymbolsTool::new(index);
        // Tool created successfully
    }

    #[test]
    fn test_get_symbols_empty() {
        let index = Arc::new(LayeredSnapshotIndex::empty());
        let tool = GetSymbolsTool::new(index);

        let request = GetSymbolsRequest {
            paths: vec!["test.rs".to_string()],
        };

        let response = tool.get_symbols(request).expect("Should succeed");
        assert_eq!(response.success_count, 0);
        assert_eq!(response.fail_count, 1);
    }
}
