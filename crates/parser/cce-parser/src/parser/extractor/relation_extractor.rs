//! Relation extractor for semantic relation extraction
//!
//! Extracts relations (calls, dependencies) between entities using tree-sitter queries.
//!
//! # Design Principles
//!
//! - **Deferred Resolution**: Calee names are stored as strings, resolved later by IndexBuilder
//! - **Caller Identification**: Uses entity stack to find current caller
//! - **Stateless Output**: Output structures are self-contained
//!
//! # Layout
//!
//! - `entity_index` - Caller lookup over extracted entities
//! - `relation_handlers` - Capture selection, callee naming and type mapping
//! - `call_extractor` - Single call-match to relation conversion
//! - `dependency_extractor` - Dependency-match conversion and import dedup
//! - `require_filter` - Shadowed `require()` filtering (JS/TS)
//! - `tests` - Extraction tests

mod call_extractor;
mod dependency_extractor;
mod entity_index;
mod relation_handlers;
mod require_filter;

use crate::tree_sitter_query::error::QueryError;
use crate::tree_sitter_query::executor::QueryExecutor;
use crate::tree_sitter_query::loader::{QueryLoader, QueryType};
use cce_types::language::Language;
use cce_types::{Entity, Relation};
use std::sync::Arc;
use tree_sitter::Tree;

use call_extractor::process_call_match;
use dependency_extractor::{deduplicate_generic_import_relations, process_dependency_match};
use entity_index::EntityIndex;
use relation_handlers::{extract_impl_block_relations, find_dependency_capture};
use require_filter::is_shadowed_require;

/// Relation extractor
///
/// Extracts relations between entities using tree-sitter queries.
pub struct RelationExtractor {
    /// Query executor
    query_executor: Arc<QueryExecutor>,
}

impl RelationExtractor {
    /// Create a new relation extractor
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
        }
    }

    /// Create with custom query executor
    pub fn with_executor(executor: Arc<QueryExecutor>) -> Self {
        Self {
            query_executor: executor,
        }
    }

    /// Extract relations from source code
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    /// * `entities` - Previously extracted entities (for source identification)
    /// * `file_id` - File ID for file-level relations (imports, exports, etc.)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Relation>)` - List of relations (unresolved)
    /// * `Err(QueryError)` - If query execution fails
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        let mut relations = Vec::new();

        // Extract call relations
        let call_relations = self.extract_calls(tree, source, language, entities, file_id)?;
        relations.extend(call_relations);

        // Extract dependency relations (file-level)
        let dep_relations = self.extract_dependencies(tree, source, language, entities, file_id)?;
        relations.extend(dep_relations);

        Ok(relations)
    }

    /// Extract call relations
    fn extract_calls(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        // Template and style languages declare no call query: they have no
        // call semantics, so absence yields no relations instead of an error.
        if !matches!(language, Language::Custom(_))
            && !QueryLoader::supports_builtin_query(*language, QueryType::Call)
        {
            return Ok(Vec::new());
        }
        let matches = self
            .query_executor
            .execute_call_query(tree, source, language)?;

        // Build entity index for efficient caller lookup
        let entity_index = EntityIndex::new(entities);

        let mut relations = Vec::new();

        for mat in &matches {
            if let Some(relation) =
                process_call_match(mat, &entity_index, language, file_id, tree, source)
            {
                relations.push(relation);
            }
        }

        Ok(relations)
    }

    /// Extract dependency relations
    fn extract_dependencies(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        file_id: Option<i64>,
    ) -> Result<Vec<Relation>, QueryError> {
        // Languages without a declared dependency query have no dependency
        // semantics; absence yields no relations instead of an error.
        if !matches!(language, Language::Custom(_))
            && !QueryLoader::supports_builtin_query(*language, QueryType::Dependency)
        {
            return Ok(Vec::new());
        }
        let matches = self
            .query_executor
            .execute_dependency_query(tree, source, language)?;

        let entity_index = EntityIndex::new(entities);
        let mut relations = Vec::new();

        for mat in &matches {
            if let Some(dep_capture) = find_dependency_capture(mat) {
                if is_shadowed_require(mat, dep_capture, entities, language) {
                    continue;
                }
                if let Some(relation) =
                    process_dependency_match(dep_capture, file_id, &entity_index)
                {
                    relations.push(relation);
                }
            }
        }

        // Derive impl-block structural relations from parsed entities.
        // Impl blocks are parsed once during entity extraction; the
        // dependency query does not re-match impl_item nodes.
        relations.extend(extract_impl_block_relations(entities));

        deduplicate_generic_import_relations(&mut relations);

        Ok(relations)
    }

    // Note: resolve_local_calls and calculate_call_order have been moved to
    // relation::LocalCallResolver to maintain separation of concerns:
    // - parser module: Extracts raw semantic data
    // - relation module: Resolves and indexes relationships
}

impl Default for RelationExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
