//! Assembly handler for SPSR-Graph result assembly
//!
//! Handles the structure-preserving assembly of search results
//! using the SPSR-Graph assembler.

use std::sync::Arc;

use crate::query::assembly::{
    AssembledResult, SPSRGraphAssembler, SearchResultInput,
};
use crate::query::error::{QueryError, Result};
use crate::query::types::SearchResult;

/// Handles SPSR-Graph assembly operations for search results
pub struct AssemblyHandler {
    /// The SPSR-Graph assembler instance
    assembler: Arc<SPSRGraphAssembler>,
}

impl AssemblyHandler {
    /// Create a new assembly handler
    pub fn new(assembler: Arc<SPSRGraphAssembler>) -> Self {
        Self { assembler }
    }

    /// Assemble search results with SPSR-Graph structure preservation
    ///
    /// For each search result, extracts the primary semantic unit and
    /// concatenates it while preserving code structure.
    pub async fn assemble_results(
        &self,
        results: Vec<SearchResult>,
    ) -> Result<Vec<SearchResult>> {
        let mut assembled_items = Vec::new();

        for item in results {
            let input = SearchResultInput {
                id: item.id.clone(),
                entity_id: item.entity_ids.first().copied(),
                name: item.name.clone(),
                kind: item.kind.clone(),
                file_path: item.file_path.clone(),
                start_line: item.start_line,
                end_line: item.end_line,
                content: item.content.clone(),
                score: item.score,
            };

            let assembled: AssembledResult = self
                .assembler
                .assemble_single(input)
                .await
                .map_err(|e| QueryError::Assembly(e.to_string()))?;

            // Convert AssembledResult back to SearchResult
            assembled_items.push(self.assembled_to_search_result(assembled, item));
        }

        Ok(assembled_items)
    }

    /// Convert AssembledResult to SearchResult
    fn assembled_to_search_result(
        &self,
        assembled: AssembledResult,
        original: SearchResult,
    ) -> SearchResult {
        SearchResult {
            id: assembled.id,
            entity_ids: original.entity_ids,
            segment_id: original.segment_id,
            kind: assembled.kind,
            name: assembled.name,
            file_path: assembled.file_path,
            score: assembled.score,
            original_score: original.original_score,
            vector_score: original.vector_score,
            bm25_score: original.bm25_score,
            sources: original.sources,
            snippet: original.snippet,
            content: assembled.assembled_content,
            start_line: assembled.start_line,
            end_line: assembled.end_line,
            is_boosted: original.is_boosted,
            boost_reason: original.boost_reason,
            category: None,
            metadata: original.metadata,
            pattern_info: original.pattern_info.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assembly_handler_creation() {
        // This test verifies that the handler can be created
        // Full integration tests would require a real assembler
        let config = crate::query::assembly::SPSRGraphConfig::default();
        let mock_assembler = Arc::new(SPSRGraphAssembler::new(config));
        let _handler = AssemblyHandler::new(mock_assembler);

        // Just verify it compiles and creates successfully
    }
}
