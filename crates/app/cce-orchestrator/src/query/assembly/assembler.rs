//! SPSR-Graph assembler
//!
//! Main assembler that coordinates extraction, caller-supplied relation
//! expansion, and structure-preserving concatenation. Graph traversal
//! lives on the caller side; this module only attaches, deduplicates,
//! caps and concatenates the units it is given.

use futures::future;

use cce_types::EntityId;

use super::concatenator::StructureConcatenator;
use super::error::Result;
use super::extractor::SemanticUnitExtractor;
use super::types::{
    AssembledResult, AssemblyMetadata, DedupStrategy, ExpandedUnit, SPSRGraphConfig,
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
    /// * `forward` - Caller-resolved callee units to attach (ignored when
    ///   `expansion_enabled` is false; capped by `max_expanded_units`)
    /// * `backward` - Caller-resolved caller units (additionally dropped
    ///   when `expansion_include_callers` is false)
    pub async fn assemble_single(
        &self,
        input: SearchResultInput,
        forward: Vec<ExpandedUnit>,
        backward: Vec<ExpandedUnit>,
    ) -> Result<AssembledResult> {
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

        // 2. Prepare expansion units (dedup + budget cap)
        let (forward, backward) = if self.config.expansion_enabled {
            self.prepare_expansion(
                input.entity_id,
                &primary_unit,
                forward,
                backward,
                &self.config,
            )
        } else {
            (Vec::new(), Vec::new())
        };

        // 3. Concatenate with structure-preserving markers
        let concatenator = StructureConcatenator::new(self.config.clone());
        let (assembled_content, involved_files) = concatenator
            .concatenate(&primary_unit, &forward, &backward)
            .await;

        // 4. Build metadata
        let expanded_nodes = forward.len() + backward.len();
        let metadata = AssemblyMetadata {
            expanded: expanded_nodes > 0,
            expanded_nodes,
            forward_nodes: forward.len(),
            backward_nodes: backward.len(),
            file_count: involved_files.len(),
            original_length: input.content.len(),
            assembled_length: assembled_content.len(),
            truncated: assembled_content.len() >= self.config.get_max_length(),
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
            assembled_content,
            involved_files,
            metadata,
            original_content: input.content,
        })
    }

    /// Assemble multiple search results
    ///
    /// Only the top-N results (based on config.assembly_top_n) are assembled.
    /// The rest are returned as simple results. Each input carries its own
    /// caller-resolved forward/backward expansion units.
    pub async fn assemble_batch(
        &self,
        results: Vec<(SearchResultInput, Vec<ExpandedUnit>, Vec<ExpandedUnit>)>,
    ) -> Result<Vec<AssembledResult>> {
        let top_n = self.config.assembly_top_n;

        // Split into top-N (to be assembled) and rest (simple results)
        let (top_results, rest_results): (Vec<_>, Vec<_>) = results
            .into_iter()
            .enumerate()
            .partition(|(idx, _)| *idx < top_n);

        // Process top-N results in parallel
        let mut futures = Vec::new();
        for (_, (input, forward, backward)) in top_results {
            futures.push(self.assemble_single(input, forward, backward));
        }

        // Execute all futures concurrently
        let assembled_top = future::join_all(futures).await;

        // Convert Results to AssembledResults
        let mut assembled: Vec<AssembledResult> =
            assembled_top.into_iter().collect::<Result<Vec<_>>>()?;

        // Add simple results for the rest
        for (_, (input, _, _)) in rest_results {
            assembled.push(self.create_simple_result(&input));
        }

        Ok(assembled)
    }

    /// Deduplicate caller-supplied expansion units against the primary and
    /// cap the combined count to `max_expanded_units`. Forward (callee)
    /// units are kept first; backward (caller) units are dropped entirely
    /// when `expansion_include_callers` is false.
    fn prepare_expansion(
        &self,
        primary_entity: Option<EntityId>,
        primary_unit: &ExpandedUnit,
        forward: Vec<ExpandedUnit>,
        backward: Vec<ExpandedUnit>,
        config: &SPSRGraphConfig,
    ) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
        use std::collections::HashSet;

        let cap = config.max_expanded_units;
        if cap == 0 {
            return (Vec::new(), Vec::new());
        }

        let mut seen_entities: HashSet<EntityId> = HashSet::new();
        let mut seen_hashes: HashSet<u64> = HashSet::new();
        if let Some(id) = primary_entity {
            seen_entities.insert(id);
        }
        if config.dedup_strategy == DedupStrategy::ByContentHash {
            seen_hashes.insert(primary_unit.content_hash());
        }

        let mut kept_forward = Vec::new();
        let mut kept_backward = Vec::new();
        let mut kept = 0usize;

        let mut pass = |units: Vec<ExpandedUnit>, into_backward: bool| {
            for unit in units {
                if kept >= cap {
                    break;
                }
                if config.dedup_strategy == DedupStrategy::ByContentHash
                    && !seen_hashes.insert(unit.content_hash())
                {
                    continue;
                }
                if let Some(id) = unit.entity_id {
                    if !seen_entities.insert(id) {
                        continue;
                    }
                }
                if into_backward {
                    kept_backward.push(unit);
                } else {
                    kept_forward.push(unit);
                }
                kept += 1;
            }
        };

        pass(forward, false);
        if config.expansion_include_callers {
            pass(backward, true);
        }

        (kept_forward, kept_backward)
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

#[cfg(test)]
mod tests {
    use super::super::types::ExpansionOrigin;
    use super::*;

    const PRIMARY_CODE: &str = "fn a() {\n    call_b();\n}";

    fn input(content: &str, entity: u64) -> SearchResultInput {
        SearchResultInput {
            id: "r1".to_string(),
            entity_id: Some(EntityId(entity)),
            name: "primary".to_string(),
            kind: "function".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 1,
            end_line: 3,
            content: content.to_string(),
            score: 0.9,
        }
    }

    fn unit(entity: u64, name: &str, origin: ExpansionOrigin) -> ExpandedUnit {
        ExpandedUnit::new(
            format!("fn {name}() {{\n    body\n}}"),
            format!("src/{name}.rs"),
            1,
            3,
            name.to_string(),
        )
        .with_entity_id(EntityId(entity))
        .with_expansion(
            origin,
            if origin == ExpansionOrigin::Forward {
                "calls"
            } else {
                "called by"
            },
        )
    }

    fn expansion_config(cap: usize, callers: bool) -> SPSRGraphConfig {
        SPSRGraphConfig::new()
            .enable(true)
            .with_expansion(true)
            .with_max_expanded_units(cap)
            .with_caller_expansion(callers)
    }

    #[tokio::test]
    async fn test_forward_and_backward_attached() {
        let assembler = SPSRGraphAssembler::new(expansion_config(4, true));
        let result = assembler
            .assemble_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("assembly ok");
        assert!(result.metadata.expanded);
        assert_eq!(result.metadata.forward_nodes, 1);
        assert_eq!(result.metadata.backward_nodes, 1);
        assert_eq!(result.metadata.expanded_nodes, 2);
        assert!(
            result
                .assembled_content
                .contains("// --> calls: b (src/b.rs:1-3)")
        );
        assert!(
            result
                .assembled_content
                .contains("// <-- called by: c (src/c.rs:1-3)")
        );
    }

    #[tokio::test]
    async fn test_budget_cap_forward_first() {
        let assembler = SPSRGraphAssembler::new(expansion_config(2, true));
        let forward = (10..14)
            .map(|i| unit(i, &format!("f{i}"), ExpansionOrigin::Forward))
            .collect();
        let backward = vec![unit(20, "g", ExpansionOrigin::Backward)];
        let result = assembler
            .assemble_single(input(PRIMARY_CODE, 1), forward, backward)
            .await
            .expect("assembly ok");
        assert_eq!(result.metadata.forward_nodes, 2);
        assert_eq!(result.metadata.backward_nodes, 0);
    }

    #[tokio::test]
    async fn test_duplicate_entity_dropped() {
        let assembler = SPSRGraphAssembler::new(expansion_config(4, true));
        let forward = vec![
            unit(1, "dup", ExpansionOrigin::Forward),
            unit(2, "ok", ExpansionOrigin::Forward),
            unit(2, "again", ExpansionOrigin::Forward),
        ];
        let result = assembler
            .assemble_single(input(PRIMARY_CODE, 1), forward, Vec::new())
            .await
            .expect("assembly ok");
        assert_eq!(result.metadata.forward_nodes, 1);
    }

    #[tokio::test]
    async fn test_callers_excluded_when_disabled() {
        let assembler = SPSRGraphAssembler::new(expansion_config(4, false));
        let result = assembler
            .assemble_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("assembly ok");
        assert_eq!(result.metadata.forward_nodes, 1);
        assert_eq!(result.metadata.backward_nodes, 0);
        assert!(!result.assembled_content.contains("<--"));
    }

    #[tokio::test]
    async fn test_expansion_disabled_drops_units() {
        let assembler = SPSRGraphAssembler::new(SPSRGraphConfig::new().enable(true));
        let result = assembler
            .assemble_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("assembly ok");
        assert!(!result.metadata.expanded);
        assert_eq!(result.metadata.expanded_nodes, 0);
    }

    #[tokio::test]
    async fn test_assembly_disabled_shortcuts() {
        let assembler = SPSRGraphAssembler::new(SPSRGraphConfig::new());
        let result = assembler
            .assemble_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                Vec::new(),
            )
            .await
            .expect("assembly ok");
        assert!(!result.metadata.expanded);
        assert_eq!(result.assembled_content, PRIMARY_CODE);
    }
}
