//! SPSR-Graph assembler
//!
//! Main assembler that coordinates extraction and concatenation.
//! Assembly preserves code structure only; graph expansion lives
//! on the dedicated graph retrieval path.

use futures::future;

use super::concatenator::StructureConcatenator;
use super::error::Result;
use super::extractor::SemanticUnitExtractor;
use super::types::{
    AssembledResult, AssemblyMetadata, CallChainAssembly, ExpandedUnit, SPSRGraphConfig,
    SearchResultInput,
};

/// SPSR-Graph assembler
///
/// Coordinates the assembly of search results while preserving structure.
pub struct SPSRGraphAssembler {
    /// Semantic unit extractor
    extractor: SemanticUnitExtractor,
    /// Configuration
    config: SPSRGraphConfig,
}

impl SPSRGraphAssembler {
    /// Create a new assembler
    pub fn new(config: SPSRGraphConfig) -> Self {
        Self {
            extractor: SemanticUnitExtractor::new(),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_default_config() -> Self {
        Self::new(SPSRGraphConfig::default())
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

        // 2. Concatenate the primary unit on its own. Graph expansion is
        // served by the dedicated graph retrieval path, not by assembly.
        let concatenator = StructureConcatenator::new(self.config.clone());
        let (assembled_content, involved_files) =
            concatenator.concatenate(&primary_unit, &[], &[]).await;

        // 3. Build metadata
        let metadata = AssemblyMetadata {
            expanded: false,
            expanded_nodes: 0,
            file_count: involved_files.len(),
            strategy: self.config.expansion_strategy,
            max_depth: 0,
            original_length: input.content.len(),
            assembled_length: assembled_content.len(),
            truncated: assembled_content.len() >= self.config.get_max_length(),
        };

        // 4. Build call chain assembly
        let call_chain = CallChainAssembly {
            forward_expansion: Vec::new(),
            backward_expansion: Vec::new(),
            max_depth: 0,
            total_nodes: 0,
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

    /// Get the configuration
    pub fn config(&self) -> &SPSRGraphConfig {
        &self.config
    }

    /// Get the extractor
    pub fn extractor(&self) -> &SemanticUnitExtractor {
        &self.extractor
    }
}
