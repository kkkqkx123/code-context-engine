//! Find references tool
//!
//! This tool finds all references to a symbol using the relation index
//! and SQLite chunk storage for code snippets.

use std::collections::HashMap;
use std::sync::Arc;

use cce_relation::index::LayeredSnapshotIndex;
use cce_relation::index::snapshot_query::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotRelationQueryOps,
};
use cce_relation::query::QueryCache;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::EntityRepository;
use cce_types::{Entity, EntityId};
use parking_lot::RwLock;

use super::types::{
    CallerEntityInfo, FindReferencesRequest, FindReferencesResponse, GroupedReferences,
    ReferenceLocation, SymbolKind, SymbolLookupError,
};

/// Configuration for find references tool
#[derive(Debug, Clone)]
pub struct FindReferencesConfig {
    /// Default number of context lines
    pub default_context_lines: usize,
    /// Whether to include the definition itself
    pub include_definition: bool,
}

impl Default for FindReferencesConfig {
    fn default() -> Self {
        Self {
            default_context_lines: 2,
            include_definition: false,
        }
    }
}

/// Find references tool
pub struct FindReferencesTool {
    /// Published snapshot index (shared, read-only)
    index: Arc<LayeredSnapshotIndex>,
    /// Configuration
    config: FindReferencesConfig,
    /// Optional SQLite database for chunk content lookup
    sqlite: Option<Arc<SqliteClient>>,
    /// Project ID for SQLite queries
    project_id: i64,
    /// Query result cache (LRU) for repeated lookups
    cache: Arc<RwLock<QueryCache>>,
}

impl FindReferencesTool {
    /// Create a new instance with a bound project ID
    pub fn new(index: Arc<LayeredSnapshotIndex>, project_id: i64) -> Self {
        Self {
            index,
            config: FindReferencesConfig::default(),
            sqlite: None,
            project_id,
            cache: Arc::new(RwLock::new(QueryCache::new(128))),
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: FindReferencesConfig) -> Self {
        self.config = config;
        self
    }

    /// Attach SQLite database for chunk content lookup
    pub fn with_sqlite(mut self, sqlite: Arc<SqliteClient>) -> Self {
        self.sqlite = Some(sqlite);
        self
    }

    /// Find all references to a symbol (unified query path).
    ///
    /// Prioritizes the in-memory snapshot; SQLite is only used for snippet
    /// extraction, not for reference resolution. This follows the unified
    /// query path described in the performance optimization plan.
    pub fn find_references(
        &self,
        request: FindReferencesRequest,
    ) -> Result<FindReferencesResponse, SymbolLookupError> {
        let target_id = self.resolve_symbol(&request)?;

        // Check cache first
        let cache_key = target_id;
        if let Some(cached) = self.cache.write().get_callers(cache_key).cloned() {
            let references = self.get_reference_locations(cached, target_id, &request)?;
            let grouped = self.group_by_file(references);
            return Ok(FindReferencesResponse {
                symbol: request.symbol,
                total_count: grouped.iter().map(|g| g.references.len()).sum(),
                file_count: grouped.len(),
                references: grouped,
            });
        }

        let callers = self.index.get_callers_by_callee_entity(target_id);
        self.cache.write().put_callers(cache_key, callers.clone());

        let references = self.get_reference_locations(callers, target_id, &request)?;

        let grouped = self.group_by_file(references);

        Ok(FindReferencesResponse {
            symbol: request.symbol,
            total_count: grouped.iter().map(|g| g.references.len()).sum(),
            file_count: grouped.len(),
            references: grouped,
        })
    }

    /// Resolve symbol by name (unified path: snapshot first, SQLite fallback).
    pub fn resolve_symbol_by_name(
        &self,
        symbol_name: &str,
        project_id: i64,
        sqlite_conn: &rusqlite::Connection,
    ) -> Result<EntityId, SymbolLookupError> {
        // Unified path: try snapshot name index first (O(1) without DB).
        let ids = self.index.get_function_ids_by_name(symbol_name);
        if let Some(&first) = ids.first() {
            return Ok(first);
        }

        let view = crate::query::filter::load_active_query_filter(sqlite_conn, project_id)
            .map_err(|e| SymbolLookupError::Internal(e.to_string()))?;
        let mut entities = EntityRepository::search_fts_at_epoch(
            sqlite_conn,
            symbol_name,
            project_id,
            1,
            view.epoch_value(),
        )
        .map_err(|e| SymbolLookupError::Internal(format!("FTS5 search failed: {}", e)))?;

        // Two-stage resolution: an empty own generation falls back to the
        // inherited parent epoch; parent hits for overridden files are dropped.
        if entities.is_empty()
            && let Some(parent_epoch) = view.parent_epoch()
        {
            entities = EntityRepository::search_fts_at_epoch(
                sqlite_conn,
                symbol_name,
                project_id,
                1,
                parent_epoch,
            )
            .map_err(|e| SymbolLookupError::Internal(format!("FTS5 search failed: {}", e)))?;
            if !view.excluded_files().is_empty() {
                let excluded: std::collections::HashSet<&str> =
                    view.excluded_files().iter().map(String::as_str).collect();
                entities.retain(|entity| {
                    sqlite_conn
                        .query_row(
                            "SELECT path FROM files WHERE id = ?1",
                            rusqlite::params![entity.file_id],
                            |row| row.get::<_, String>(0),
                        )
                        .map(|path| !excluded.contains(path.as_str()))
                        .unwrap_or(true)
                });
            }
        }

        if let Some(entity) = entities.first() {
            Ok(EntityId(entity.id as u64))
        } else {
            Err(SymbolLookupError::SymbolNotFound)
        }
    }

    /// Resolve symbol from request.
    ///
    /// Correctly distinguishes call sites from definition sites:
    /// - If cursor is on a call expression (spans overlap), returns the callee EntityId
    /// - If cursor is on an entity definition, returns that entity's own EntityId
    fn resolve_symbol(
        &self,
        request: &FindReferencesRequest,
    ) -> Result<EntityId, SymbolLookupError> {
        let entities = self.get_entities_by_file(&request.path)?;

        let enclosing = entities
            .iter()
            .find(|e| self.contains_position(&e.span, request.line, request.column))
            .ok_or(SymbolLookupError::SymbolNotFound)?;

        if let Some(relations) = self.index.get_resolved_relations_by_caller(enclosing.id) {
            for rel in relations.iter() {
                if self.contains_position(&rel.span, request.line, request.column) {
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

    /// Check if a position is contained in a span
    fn contains_position(
        &self,
        span: &cce_types::Span,
        line: usize,
        column: Option<usize>,
    ) -> bool {
        let line_matches = span.start_position.row < line && span.end_position.row + 1 >= line;

        if !line_matches {
            return false;
        }

        if let Some(col) = column {
            if span.start_position.row == span.end_position.row {
                return span.start_position.column < col && span.end_position.column + 1 >= col;
            }
            if span.start_position.row + 1 == line {
                return span.start_position.column < col;
            }
            if span.end_position.row + 1 == line {
                return span.end_position.column + 1 >= col;
            }
            return true;
        }

        true
    }

    /// Get reference locations from callers
    fn get_reference_locations(
        &self,
        callers: Vec<EntityId>,
        target_id: EntityId,
        request: &FindReferencesRequest,
    ) -> Result<Vec<ReferenceLocation>, SymbolLookupError> {
        let mut locations = Vec::new();
        let include_snippet = request.include_snippet.unwrap_or(false);
        let context_lines = request
            .context_lines
            .or(Some(self.config.default_context_lines));
        let include_entity_info = request.include_entity_info.unwrap_or(false);

        for caller_id in callers {
            if let Some(relations) = self.index.get_resolved_relations_by_caller(caller_id) {
                for relation in relations.iter() {
                    if relation.callee_id == Some(target_id) {
                        let path = self
                            .index
                            .get_file_path_by_entity(caller_id)
                            .ok_or(SymbolLookupError::EntityNotFound(caller_id))?;

                        let snippet = if include_snippet {
                            let ctx = context_lines.unwrap_or(0);
                            let call_line = relation.span.start_position.row + 1;
                            self.extract_snippet(&path, call_line, ctx)
                        } else {
                            None
                        };

                        let caller_entity = if include_entity_info {
                            self.get_caller_entity_info(caller_id)
                        } else {
                            None
                        };

                        let (callee_file, callee_line, callee_end_line) = relation
                            .callee_symbol
                            .as_ref()
                            .map(|sym| {
                                (
                                    Some(sym.location.file_path.clone()),
                                    Some(sym.location.span.start_position.row + 1),
                                    Some(sym.location.span.end_position.row + 1),
                                )
                            })
                            .unwrap_or((None, None, None));

                        locations.push(ReferenceLocation {
                            path,
                            line: relation.span.start_position.row + 1,
                            column: relation.span.start_position.column + 1,
                            end_line: relation.span.end_position.row + 1,
                            end_column: relation.span.end_position.column + 1,
                            snippet,
                            caller_entity,
                            callee_file,
                            callee_line,
                            callee_end_line,
                        });
                    }
                }
            }
        }

        Ok(locations)
    }

    /// Extract code snippet centered on a call line from SQLite chunk storage
    fn extract_snippet(
        &self,
        path: &str,
        call_line: usize,
        context_lines: usize,
    ) -> Option<String> {
        let content = self.get_file_content_from_chunks(path)?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || call_line == 0 || call_line > lines.len() {
            return None;
        }

        let start = call_line.saturating_sub(1).saturating_sub(context_lines);
        let end = (call_line + context_lines).min(lines.len());

        let snippet_lines: Vec<&str> = lines[start..end].to_vec();
        Some(snippet_lines.join("\n"))
    }

    /// Read file content from disk (chunk rows no longer persist raw code)
    fn get_file_content_from_chunks(&self, path: &str) -> Option<String> {
        let sqlite = self.sqlite.as_ref()?;
        let project_id = self.project_id;
        let conn = sqlite.write_connection().ok()?;
        let project_root =
            cce_storage_sqlite::source_reader::resolve_project_root(&conn, project_id)?;
        let content = cce_storage_sqlite::source_reader::read_source_lines(
            Some(project_root.as_path()),
            path,
            0,
            u32::MAX,
        );
        (!content.is_empty()).then_some(content)
    }

    /// Get caller entity information
    fn get_caller_entity_info(&self, entity_id: EntityId) -> Option<CallerEntityInfo> {
        let entity = self.index.get_function_by_entity_id(entity_id)?;

        Some(CallerEntityInfo {
            name: entity.name.clone(),
            kind: SymbolKind::from(entity.kind),
            entity_id,
        })
    }

    /// Group references by file
    fn group_by_file(&self, references: Vec<ReferenceLocation>) -> Vec<GroupedReferences> {
        let mut groups: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();

        for reference in references {
            groups
                .entry(reference.path.clone())
                .or_default()
                .push(reference);
        }

        groups
            .into_iter()
            .map(|(path, refs)| GroupedReferences {
                path,
                count: refs.len(),
                references: refs,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_references_tool_creation() {
        let index = Arc::new(LayeredSnapshotIndex::empty());
        let tool = FindReferencesTool::new(index, 1);
        assert!(std::matches!(tool.config, FindReferencesConfig { .. }));
    }
}
