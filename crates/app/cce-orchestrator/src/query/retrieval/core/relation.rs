//! Relation retrieval implementation
//!
//! Provides relation search operations for call chains as a stateless
//! wrapper over `CallChainQuery`. Operates at the parser abstraction level
//! rather than directly on storage backends.

use crate::query::error::QueryError;
use crate::query::types::CallInfo;
use cce_relation::CallChainQuery;
use cce_types::EntityId;

/// Search options for relation queries
#[derive(Debug, Clone, Default)]
pub struct RelationOptions {
    /// Maximum depth for call chain traversal
    pub max_depth: usize,
    /// Whether to include callers (functions that call this entity)
    pub include_callers: bool,
    /// Whether to include callees (functions called by this entity)
    pub include_callees: bool,
    /// Maximum number of results per direction (callers or callees)
    pub limit_per_direction: usize,
}

impl RelationOptions {
    /// Create new relation options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set whether to include callers
    pub fn with_callers(mut self, include: bool) -> Self {
        self.include_callers = include;
        self
    }

    /// Set whether to include callees
    pub fn with_callees(mut self, include: bool) -> Self {
        self.include_callees = include;
        self
    }

    /// Set limit per direction
    pub fn with_limit_per_direction(mut self, limit: usize) -> Self {
        self.limit_per_direction = limit;
        self
    }

    /// Create options for both directions with default depth
    pub fn both_directions() -> Self {
        Self {
            include_callers: true,
            include_callees: true,
            ..Default::default()
        }
    }

    /// Create options for callers only
    pub fn callers_only() -> Self {
        Self {
            include_callers: true,
            include_callees: false,
            ..Default::default()
        }
    }

    /// Create options for callees only
    pub fn callees_only() -> Self {
        Self {
            include_callers: false,
            include_callees: true,
            ..Default::default()
        }
    }
}

/// Relation retrieval handler
#[derive(Clone)]
pub struct RelationRetrieval {
    // Retrieval is stateless - operates on provided CallChainQuery
}

impl RelationRetrieval {
    /// Create a new relation retrieval instance
    pub fn new() -> Self {
        Self {}
    }

    /// Search for callers (functions that call this entity)
    ///
    /// # Arguments
    ///
    /// * `call_query` - The call chain query instance
    /// * `entity_id` - The entity ID to query
    /// * `options` - Search options
    ///
    /// # Returns
    ///
    /// Returns a list of caller information
    pub fn search_callers(
        &self,
        call_query: &CallChainQuery,
        entity_id: EntityId,
        options: &RelationOptions,
    ) -> Result<Vec<CallInfo>, QueryError> {
        if !options.include_callers {
            return Ok(Vec::new());
        }

        call_query
            .query_backward_by_entity(entity_id, options.max_depth)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|node| CallInfo {
                        id: node.function_id,
                        name: node.function_name,
                        file: node.file_path,
                        line: node.call_line.map(|l| l as u32),
                    })
                    .take(options.limit_per_direction)
                    .collect()
            })
            .map_err(QueryError::Relation)
    }

    /// Search for callees (functions called by this entity)
    ///
    /// # Arguments
    ///
    /// * `call_query` - The call chain query instance
    /// * `entity_id` - The entity ID to query
    /// * `options` - Search options
    ///
    /// # Returns
    ///
    /// Returns a list of callee information
    pub fn search_callees(
        &self,
        call_query: &CallChainQuery,
        entity_id: EntityId,
        options: &RelationOptions,
    ) -> Result<Vec<CallInfo>, QueryError> {
        if !options.include_callees {
            return Ok(Vec::new());
        }

        call_query
            .query_forward_by_entity(entity_id, options.max_depth)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|node| CallInfo {
                        id: node.function_id,
                        name: node.function_name,
                        file: node.file_path,
                        line: node.call_line.map(|l| l as u32),
                    })
                    .take(options.limit_per_direction)
                    .collect()
            })
            .map_err(QueryError::Relation)
    }

    /// Search for both callers and callees
    ///
    /// # Arguments
    ///
    /// * `call_query` - The call chain query instance
    /// * `entity_id` - The entity ID to query
    /// * `options` - Search options
    ///
    /// # Returns
    ///
    /// Returns a tuple of (callers, callees)
    pub fn search_both(
        &self,
        call_query: &CallChainQuery,
        entity_id: EntityId,
        options: &RelationOptions,
    ) -> Result<(Vec<CallInfo>, Vec<CallInfo>), QueryError> {
        let callers = self.search_callers(call_query, entity_id, options)?;
        let callees = self.search_callees(call_query, entity_id, options)?;
        Ok((callers, callees))
    }

    /// Find call chain path between two entities
    ///
    /// # Arguments
    ///
    /// * `call_query` - The call chain query instance
    /// * `start_id` - The starting entity ID
    /// * `end_id` - The target entity ID
    /// * `options` - Search options (max_depth is used)
    ///
    /// # Returns
    ///
    /// Returns the path as a list of CallInfo, or None if no path exists
    pub fn find_path(
        &self,
        call_query: &CallChainQuery,
        start_id: EntityId,
        end_id: EntityId,
        options: &RelationOptions,
    ) -> Result<Option<Vec<CallInfo>>, QueryError> {
        call_query
            .find_call_chain(start_id, end_id, options.max_depth)
            .map(|path_opt| {
                path_opt.map(|path| {
                    path.into_iter()
                        .map(|node| CallInfo {
                            id: node.function_id,
                            name: node.function_name,
                            file: node.file_path,
                            line: node.call_line.map(|l| l as u32),
                        })
                        .collect()
                })
            })
            .map_err(QueryError::Relation)
    }

    /// Get immediate callees only (depth = 1)
    ///
    /// This is a convenience method for common use cases.
    pub fn get_direct_callees(
        &self,
        call_query: &CallChainQuery,
        entity_id: EntityId,
    ) -> Result<Vec<CallInfo>, QueryError> {
        let options = RelationOptions {
            max_depth: 1,
            include_callers: false,
            include_callees: true,
            limit_per_direction: usize::MAX,
        };
        self.search_callees(call_query, entity_id, &options)
    }

    /// Get immediate callers only (depth = 1)
    ///
    /// This is a convenience method for common use cases.
    pub fn get_direct_callers(
        &self,
        call_query: &CallChainQuery,
        entity_id: EntityId,
    ) -> Result<Vec<CallInfo>, QueryError> {
        let options = RelationOptions {
            max_depth: 1,
            include_callers: true,
            include_callees: false,
            limit_per_direction: usize::MAX,
        };
        self.search_callers(call_query, entity_id, &options)
    }
}

impl Default for RelationRetrieval {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_relation::CallChainQuery;

    #[test]
    fn test_relation_options_default() {
        let options = RelationOptions::default();
        assert_eq!(options.max_depth, 0);
        assert!(!options.include_callers);
        assert!(!options.include_callees);
        assert_eq!(options.limit_per_direction, 0);
    }

    #[test]
    fn test_relation_options_builder() {
        let options = RelationOptions::new()
            .with_max_depth(3)
            .with_callers(true)
            .with_callees(true)
            .with_limit_per_direction(10);

        assert_eq!(options.max_depth, 3);
        assert!(options.include_callers);
        assert!(options.include_callees);
        assert_eq!(options.limit_per_direction, 10);
    }

    #[test]
    fn test_relation_options_presets() {
        let both = RelationOptions::both_directions();
        assert!(both.include_callers);
        assert!(both.include_callees);

        let callers = RelationOptions::callers_only();
        assert!(callers.include_callers);
        assert!(!callers.include_callees);

        let callees = RelationOptions::callees_only();
        assert!(!callees.include_callers);
        assert!(callees.include_callees);
    }

    #[test]
    fn test_relation_retrieval_new() {
        let retrieval = RelationRetrieval::new();
        let call_query = CallChainQuery::new();

        // Test with empty index - should not panic
        let result =
            retrieval.search_callers(&call_query, EntityId(0), &RelationOptions::default());
        assert!(result.is_err() || result.unwrap().is_empty());

        let result =
            retrieval.search_callees(&call_query, EntityId(0), &RelationOptions::default());
        assert!(result.is_err() || result.unwrap().is_empty());
    }
}
