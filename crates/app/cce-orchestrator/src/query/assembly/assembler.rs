//! SPSR-Graph assembler
//!
//! Main assembler that coordinates extraction, expansion, and concatenation.

use std::path::Path;
use std::sync::Arc;

use futures::future;

use cce_relation::{CallChainNode, CallChainQuery, index::SnapshotEntityQueryOps};
use cce_utils::file::read_file_to_utf8_async;

use super::concatenator::StructureConcatenator;
use super::error::{AssemblyError, Result};
use super::extractor::SemanticUnitExtractor;
use super::types::{
    AssembledResult, AssemblyMetadata, CallChainAssembly, ExpandedUnit, ExpansionStrategy,
    RelationType, SPSRGraphConfig, SearchResultInput,
};
use crate::query::relation_searcher::{RelationQueryOptions, RelationSearcher};

/// SPSR-Graph assembler
///
/// Coordinates the assembly of search results with call chain context.
pub struct SPSRGraphAssembler {
    /// Relation searcher for call chain queries
    relation_searcher: Arc<RelationSearcher>,
    /// Semantic unit extractor
    extractor: SemanticUnitExtractor,
    /// Configuration
    config: SPSRGraphConfig,
}

impl SPSRGraphAssembler {
    /// Create a new assembler
    pub fn new(call_chain_query: Arc<CallChainQuery>, config: SPSRGraphConfig) -> Self {
        Self {
            relation_searcher: Arc::new(RelationSearcher::new(call_chain_query)),
            extractor: SemanticUnitExtractor::new(),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_default_config(call_chain_query: Arc<CallChainQuery>) -> Self {
        Self::new(call_chain_query, SPSRGraphConfig::default())
    }

    /// Create from an existing RelationSearcher
    pub fn from_relation_searcher(
        relation_searcher: Arc<RelationSearcher>,
        config: SPSRGraphConfig,
    ) -> Self {
        Self {
            relation_searcher,
            extractor: SemanticUnitExtractor::new(),
            config,
        }
    }

    /// Assemble a single search result
    ///
    /// # Arguments
    ///
    /// * `input` - Search result input containing all necessary parameters
    pub async fn assemble_single(&self, input: SearchResultInput) -> Result<AssembledResult> {
        // Check if assembly is enabled
        if !self.config.enable_assembly {
            return Ok(self.create_simple_result(&input));
        }

        // 1. Extract the primary unit
        let primary_unit = self.extractor.extract_unit_from_content(
            &input.content,
            &input.file_path,
            input.start_line,
            input.end_line,
            &input.name,
            &input.kind,
        )?;

        // 2. Expand call chain if entity_id is available
        let (forward, backward) = if let Some(eid) = input.entity_id {
            self.expand_call_chain(eid)?
        } else {
            (Vec::new(), Vec::new())
        };

        // 3. Fill code content for expanded units
        let forward_filled = self.fill_unit_codes(forward).await?;
        let backward_filled = self.fill_unit_codes(backward).await?;

        // 4. Deduplicate units
        let (forward_dedup, backward_dedup) =
            self.deduplicate_units(forward_filled, backward_filled, &self.config);

        // 5. Concatenate
        let concatenator = StructureConcatenator::new(self.config.clone());
        let (assembled_content, involved_files) = concatenator
            .concatenate(&primary_unit, &forward_dedup, &backward_dedup)
            .await;

        // 6. Build metadata
        let max_depth = std::cmp::max(
            forward_dedup.iter().map(|u| u.depth).max().unwrap_or(0),
            backward_dedup.iter().map(|u| u.depth).max().unwrap_or(0),
        );

        let metadata = AssemblyMetadata {
            expanded: !forward_dedup.is_empty() || !backward_dedup.is_empty(),
            expanded_nodes: forward_dedup.len() + backward_dedup.len(),
            file_count: involved_files.len(),
            strategy: self.config.expansion_strategy,
            max_depth,
            original_length: input.content.len(),
            assembled_length: assembled_content.len(),
            truncated: assembled_content.len() >= self.config.get_max_length(),
        };

        // 7. Build call chain assembly
        let call_chain = CallChainAssembly {
            forward_expansion: forward_dedup,
            backward_expansion: backward_dedup,
            max_depth,
            total_nodes: metadata.expanded_nodes,
        };

        Ok(AssembledResult {
            id: input.id,
            entity_id: input.entity_id,
            name: input.name,
            kind: input.kind,
            file_path: input.file_path,
            score: input.score,
            start_line: input.start_line,
            end_line: input.end_line,
            call_chain,
            assembled_content,
            involved_files,
            metadata,
            original_content: input.content,
        })
    }

    /// Assemble multiple search results
    ///
    /// Only the top-N results (based on config.assembly_top_n) are assembled.
    /// The rest are returned as simple results.
    pub async fn assemble_batch(
        &self,
        results: Vec<SearchResultInput>,
    ) -> Result<Vec<AssembledResult>> {
        let top_n = self.config.assembly_top_n;

        // Split into top-N (to be assembled) and rest (simple results)
        let (top_results, rest_results): (Vec<_>, Vec<_>) = results
            .into_iter()
            .enumerate()
            .partition(|(idx, _)| *idx < top_n);

        // Process top-N results in parallel
        let mut futures = Vec::new();
        for (_, input) in top_results {
            futures.push(self.assemble_single(input));
        }

        // Execute all futures concurrently
        let assembled_top = future::join_all(futures).await;

        // Convert Results to AssembledResults
        let mut assembled: Vec<AssembledResult> =
            assembled_top.into_iter().collect::<Result<Vec<_>>>()?;

        // Add simple results for the rest
        for (_, input) in rest_results {
            assembled.push(self.create_simple_result(&input));
        }

        Ok(assembled)
    }

    /// Create a simple (non-assembled) result
    fn create_simple_result(&self, input: &SearchResultInput) -> AssembledResult {
        let unit = ExpandedUnit::new(
            input.content.clone(),
            input.file_path.clone(),
            input.start_line,
            input.end_line,
            input.name.clone(),
        );

        AssembledResult::from_primary(unit, input.score, input.id.clone(), input.kind.clone())
    }

    /// Deduplicate expanded units
    fn deduplicate_units(
        &self,
        forward: Vec<ExpandedUnit>,
        backward: Vec<ExpandedUnit>,
        config: &SPSRGraphConfig,
    ) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
        use super::types::UnitDeduplicator;

        let mut dedup = UnitDeduplicator::new();

        let forward_dedup: Vec<ExpandedUnit> = forward
            .into_iter()
            .filter(|u| dedup.should_keep(u, config.dedup_strategy))
            .collect();

        let backward_dedup: Vec<ExpandedUnit> = backward
            .into_iter()
            .filter(|u| dedup.should_keep(u, config.dedup_strategy))
            .collect();

        (forward_dedup, backward_dedup)
    }

    /// Fill code content for expanded units
    ///
    /// This method reads file contents and extracts code for units
    /// that have empty `code` fields (populated by expander).
    async fn fill_unit_codes(&self, units: Vec<ExpandedUnit>) -> Result<Vec<ExpandedUnit>> {
        use futures::future::BoxFuture;

        type FillResult = Result<(usize, String, u32, u32, String, String, String)>;

        // Create futures for all units that need code filling
        let mut futures: Vec<BoxFuture<'_, FillResult>> = Vec::new();
        let mut unit_info = Vec::new();

        for (idx, unit) in units.into_iter().enumerate() {
            if unit.code.is_empty() && !unit.file_path.is_empty() {
                let file_path = unit.file_path.clone();
                let start_line = unit.start_line;
                let end_line = unit.end_line;
                let name = unit.name.clone();
                let kind = match unit.unit_type {
                    super::types::SemanticUnitType::Function => "function",
                    super::types::SemanticUnitType::Method => "method",
                    super::types::SemanticUnitType::Class => "class",
                    super::types::SemanticUnitType::Struct => "struct",
                    super::types::SemanticUnitType::Interface => "interface",
                    super::types::SemanticUnitType::Enum => "enum",
                    super::types::SemanticUnitType::Module => "module",
                    _ => "function", // Default
                };

                let fut: BoxFuture<'_, FillResult> = Box::pin(async move {
                    let file_content = read_file_to_utf8_async(Path::new(&file_path))
                        .await
                        .map_err(|e| AssemblyError::extraction_failed(&file_path, e.to_string()))?;

                    Ok((
                        idx,
                        file_content,
                        start_line,
                        end_line,
                        name,
                        file_path,
                        kind.to_string(),
                    ))
                });
                futures.push(fut);
                unit_info.push(Some(unit));
            } else {
                // Unit already has code, no need to process
                unit_info.push(Some(unit));
                let fut: BoxFuture<'_, FillResult> =
                    Box::pin(
                        async move { Err(AssemblyError::IoError("Already filled".to_string())) },
                    );
                futures.push(fut);
            }
        }

        // Execute all file reads in parallel
        let results = future::join_all(futures).await;

        // Process results and rebuild units
        let mut filled_units = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok((idx, file_content, start_line, end_line, name, file_path, kind)) => {
                    if let Some(mut unit) = unit_info[idx].take() {
                        let extracted = self.extractor.extract_unit_from_content(
                            &file_content,
                            &file_path,
                            start_line,
                            end_line,
                            &name,
                            &kind,
                        )?;
                        unit.code = extracted.code;
                        unit.unit_type = extracted.unit_type;
                        filled_units.push(unit);
                    }
                }
                Err(_) => {
                    // Already had code or error occurred
                    if let Some(unit) = unit_info[i].take() {
                        filled_units.push(unit);
                    }
                }
            }
        }

        Ok(filled_units)
    }

    /// Get the configuration
    pub fn config(&self) -> &SPSRGraphConfig {
        &self.config
    }

    /// Get the relation searcher
    pub fn relation_searcher(&self) -> &RelationSearcher {
        &self.relation_searcher
    }

    /// Get the extractor
    pub fn extractor(&self) -> &SemanticUnitExtractor {
        &self.extractor
    }

    /// Expand call chain from an entity
    ///
    /// Uses RelationSearcher for traversal and converts results to ExpandedUnits.
    /// Returns (forward_expansion, backward_expansion) based on the strategy.
    fn expand_call_chain(
        &self,
        entity_id: cce_types::EntityId,
    ) -> Result<(Vec<ExpandedUnit>, Vec<ExpandedUnit>)> {
        if self.config.expansion_strategy == ExpansionStrategy::None {
            return Ok((Vec::new(), Vec::new()));
        }

        // Early budget check: estimate if expansion will fit within limits
        if !self.should_expand(entity_id)? {
            tracing::trace!(
                "Skipping expansion for entity {:?} due to budget constraints",
                entity_id
            );
            return Ok((Vec::new(), Vec::new()));
        }

        let options = RelationQueryOptions::new()
            .with_max_depth(self.config.max_expansion_depth)
            .with_limit(self.config.max_expanded_nodes);

        let forward = match self.config.expansion_strategy {
            ExpansionStrategy::ForwardOnly | ExpansionStrategy::Bidirectional => {
                self.expand_forward(entity_id, &options)?
            }
            _ => Vec::new(),
        };

        let backward = match self.config.expansion_strategy {
            ExpansionStrategy::BackwardOnly | ExpansionStrategy::Bidirectional => {
                self.expand_backward(entity_id, &options)?
            }
            _ => Vec::new(),
        };

        Ok((forward, backward))
    }

    /// Check if we should expand based on budget constraints
    fn should_expand(&self, entity_id: cce_types::EntityId) -> Result<bool> {
        // Quick metadata-only check to estimate expansion size
        let index = self.relation_searcher.query().index();

        // Get function info to estimate code size
        if let Some(func_info) = index.get_function_by_entity_id(entity_id) {
            let span = func_info.span;
            let estimated_lines = span.end_position.row - span.start_position.row + 1;

            // Rough estimate: ~50 chars per line average
            let estimated_size = estimated_lines * 50;

            // If primary unit alone is > 50% of budget, skip expansion
            let budget = self.config.get_max_length();
            if estimated_size > budget / 2 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Expand forward (get callees) using RelationSearcher
    fn expand_forward(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<ExpandedUnit>> {
        let nodes = self
            .relation_searcher
            .query_forward(entity_id, options)
            .map_err(|e| AssemblyError::IoError(e.to_string()))?;

        // Convert CallChainNode to ExpandedUnit
        let units: Vec<ExpandedUnit> = nodes
            .into_iter()
            .take(self.config.max_expanded_nodes)
            .map(|node| self.node_to_unit(node, RelationType::Callee))
            .collect();

        Ok(units)
    }

    /// Expand backward (get callers) using RelationSearcher
    fn expand_backward(
        &self,
        entity_id: cce_types::EntityId,
        options: &RelationQueryOptions,
    ) -> Result<Vec<ExpandedUnit>> {
        let nodes = self
            .relation_searcher
            .query_backward(entity_id, options)
            .map_err(|e| AssemblyError::IoError(e.to_string()))?;

        // Convert CallChainNode to ExpandedUnit
        let units: Vec<ExpandedUnit> = nodes
            .into_iter()
            .take(self.config.max_expanded_nodes)
            .map(|node| self.node_to_unit(node, RelationType::Caller))
            .collect();

        Ok(units)
    }

    /// Convert a CallChainNode to an ExpandedUnit
    ///
    /// Note: The `code` field is left empty and should be populated later
    /// by fill_unit_codes when the actual source code is needed.
    fn node_to_unit(&self, node: CallChainNode, relation: RelationType) -> ExpandedUnit {
        // Get entity metadata from the relation index
        let index = self.relation_searcher.query().index();

        // Get span info if available
        let (start_line, end_line) =
            if let Some(func_info) = index.get_function_by_entity_id(node.function_id) {
                let span = func_info.span;
                (
                    (span.start_position.row + 1) as u32,
                    (span.end_position.row + 1) as u32,
                )
            } else {
                (1, 1) // Default if not found
            };

        ExpandedUnit {
            entity_id: Some(node.function_id),
            code: String::new(), // Will be populated by fill_unit_codes
            file_path: node.file_path,
            start_line,
            end_line,
            name: node.function_name,
            unit_type: super::types::SemanticUnitType::Unknown, // Will be set by extractor
            relation,
            depth: node.depth,
        }
    }
}
