//! Relation annotator
//!
//! Coordinates extraction, caller-supplied relation expansion, and
//! structure-preserving concatenation. Graph traversal lives on the caller
//! side; this module only attaches, deduplicates, caps and concatenates the
//! units it is given.

use futures::future;

use cce_types::EntityId;

use super::concatenator::StructureConcatenator;
use super::error::Result;
use super::extractor::SemanticUnitExtractor;
use super::types::{
    AnnotatedResult, AnnotationMetadata, DedupStrategy, ExpandedUnit, RelationAnnotationConfig,
    SearchResultInput,
};

/// Relation annotator
///
/// Coordinates the annotation of search results with caller-resolved
/// call-graph neighbours while preserving structure.
pub struct RelationAnnotator {
    /// Semantic unit extractor
    extractor: SemanticUnitExtractor,
    /// Configuration
    config: RelationAnnotationConfig,
}

impl RelationAnnotator {
    /// Create a new annotator
    pub fn new(config: RelationAnnotationConfig) -> Self {
        Self {
            extractor: SemanticUnitExtractor::new(),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_default_config() -> Self {
        Self::new(RelationAnnotationConfig::default())
    }

    /// Annotate a single search result
    ///
    /// # Arguments
    ///
    /// * `input` - Search result input containing all necessary parameters
    /// * `forward` - Caller-resolved callee units to attach (ignored when
    ///   `expansion_enabled` is false; capped by `max_expanded_units`)
    /// * `backward` - Caller-resolved caller units (additionally dropped
    ///   when `expansion_include_callers` is false)
    pub async fn annotate_single(
        &self,
        input: SearchResultInput,
        forward: Vec<ExpandedUnit>,
        backward: Vec<ExpandedUnit>,
    ) -> Result<AnnotatedResult> {
        // Check if annotation is enabled
        if !self.config.enable_annotation {
            return Ok(self.create_unannotated_result(&input));
        }

        // 1. Extract the primary unit and carry the hit score onto it so
        // downstream score ordering and budget selection can rank it.
        let mut primary_unit = self.extractor.extract_unit_from_content(
            &input.content,
            &input.file_path,
            input.start_line,
            input.end_line,
            &input.name,
            &input.kind,
        )?;
        primary_unit.score = input.score;

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
        let (annotated_content, involved_files) = concatenator
            .concatenate(&primary_unit, &forward, &backward)
            .await;

        // 4. Build metadata (truncation compares tokens against the token budget)
        let expanded_nodes = forward.len() + backward.len();
        let max_length = self.config.get_max_length();
        let metadata = AnnotationMetadata {
            expanded: expanded_nodes > 0,
            expanded_nodes,
            forward_nodes: forward.len(),
            backward_nodes: backward.len(),
            file_count: involved_files.len(),
            original_length: input.content.len(),
            annotated_length: annotated_content.len(),
            truncated: self.config.estimate_content_tokens(&annotated_content) >= max_length,
        };

        Ok(AnnotatedResult {
            id: input.id,
            entity_id: input.entity_id,
            name: input.name,
            kind: input.kind,
            file_path: input.file_path,
            score: input.score,
            start_line: input.start_line,
            end_line: input.end_line,
            annotated_content,
            involved_files,
            metadata,
            original_content: input.content,
        })
    }

    /// Annotate multiple search results.
    ///
    /// Only the top-N results (based on config.annotation_top_n) are annotated.
    /// The rest are returned as simple results. Each input carries its own
    /// caller-resolved forward/backward expansion units. Every result is
    /// capped by the single per-result quota; the batch total is bounded by
    /// `annotation_top_n` times that quota and never downgrades tail results.
    pub async fn annotate_batch(
        &self,
        results: Vec<(SearchResultInput, Vec<ExpandedUnit>, Vec<ExpandedUnit>)>,
    ) -> Result<Vec<AnnotatedResult>> {
        let top_n = self.config.annotation_top_n;

        // Split into top-N (to be annotated) and rest (simple results)
        let (top_results, rest_results): (Vec<_>, Vec<_>) = results
            .into_iter()
            .enumerate()
            .partition(|(idx, _)| *idx < top_n);

        // Process top-N results in parallel
        let mut futures = Vec::new();
        for (_, (input, forward, backward)) in top_results {
            futures.push(self.annotate_single(input, forward, backward));
        }

        // Execute all futures concurrently
        let annotated_top = future::join_all(futures).await;

        // Convert Results to AnnotatedResults
        let mut annotated: Vec<AnnotatedResult> =
            annotated_top.into_iter().collect::<Result<Vec<_>>>()?;

        // Add simple results for the rest
        for (_, (input, _, _)) in rest_results {
            annotated.push(self.create_unannotated_result(&input));
        }

        Ok(annotated)
    }

    /// Deduplicate caller-supplied expansion units against the primary and
    /// cap the combined count to `max_expanded_units`.
    ///
    /// Noise is filtered before the budget cap so dropped units never occupy
    /// a slot: stdlib and external targets go first (each behind its own
    /// switch, as a defensive second layer after caller-side filtering),
    /// then non-call edges unless structural edges are allowed. Survivors are
    /// ordered by score descending within each direction; forward (callee)
    /// units fill the shared cap first and backward (caller) units take the
    /// remainder (dropped entirely when `expansion_include_callers` is false).
    fn prepare_expansion(
        &self,
        primary_entity: Option<EntityId>,
        primary_unit: &ExpandedUnit,
        forward: Vec<ExpandedUnit>,
        backward: Vec<ExpandedUnit>,
        config: &RelationAnnotationConfig,
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

        // Defensive noise filter: drop units the caller should already have
        // excluded so they never occupy a budget slot.
        let mut forward: Vec<ExpandedUnit> = forward
            .into_iter()
            .filter(|unit| Self::keep_unit(unit, config))
            .collect();
        let mut backward: Vec<ExpandedUnit> = backward
            .into_iter()
            .filter(|unit| Self::keep_unit(unit, config))
            .collect();

        // Highest score first within each direction (stable: ties keep
        // caller order). Forward fills the shared cap before backward.
        forward.sort_by(|a, b| b.score.total_cmp(&a.score));
        backward.sort_by(|a, b| b.score.total_cmp(&a.score));

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

    /// Decide whether an expansion unit survives noise filtering.
    fn keep_unit(unit: &ExpandedUnit, config: &RelationAnnotationConfig) -> bool {
        if config.filter_stdlib && unit.is_stdlib {
            return false;
        }
        if config.filter_external && unit.is_external {
            return false;
        }
        if !config.allow_structural_edges && !unit.is_call_domain() {
            return false;
        }
        true
    }

    /// Create a simple (non-annotated) result
    fn create_unannotated_result(&self, input: &SearchResultInput) -> AnnotatedResult {
        let unit = ExpandedUnit::new(
            input.content.clone(),
            input.file_path.clone(),
            input.start_line,
            input.end_line,
            input.name.clone(),
        );

        AnnotatedResult::from_primary(unit, input.score, input.id.clone(), input.kind.clone())
    }

    /// Get the configuration
    pub fn config(&self) -> &RelationAnnotationConfig {
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
        unit_scored(entity, name, origin, 0.0)
    }

    fn unit_scored(entity: u64, name: &str, origin: ExpansionOrigin, score: f32) -> ExpandedUnit {
        ExpandedUnit::new(
            format!("fn {name}() {{\n    body\n}}"),
            format!("src/{name}.rs"),
            1,
            3,
            name.to_string(),
        )
        .with_entity_id(EntityId(entity))
        .with_score(score)
        .with_expansion(
            origin,
            if origin == ExpansionOrigin::Forward {
                "calls"
            } else {
                "called by"
            },
        )
    }

    fn expansion_config(cap: usize, callers: bool) -> RelationAnnotationConfig {
        RelationAnnotationConfig::new()
            .enable(true)
            .with_expansion(true)
            .with_max_expanded_units(cap)
            .with_caller_expansion(callers)
    }

    #[tokio::test]
    async fn test_forward_and_backward_attached() {
        let annotator = RelationAnnotator::new(expansion_config(4, true));
        let result = annotator
            .annotate_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("annotation ok");
        assert!(result.metadata.expanded);
        assert_eq!(result.metadata.forward_nodes, 1);
        assert_eq!(result.metadata.backward_nodes, 1);
        assert_eq!(result.metadata.expanded_nodes, 2);
        assert!(
            result
                .annotated_content
                .contains("<unit rel=\"calls\" name=\"b\"")
        );
        assert!(
            result
                .annotated_content
                .contains("<unit rel=\"called by\" name=\"c\"")
        );
    }

    #[tokio::test]
    async fn test_budget_cap_forward_first() {
        let annotator = RelationAnnotator::new(expansion_config(2, true));
        let forward = (10..14)
            .map(|i| unit(i, &format!("f{i}"), ExpansionOrigin::Forward))
            .collect();
        let backward = vec![unit(20, "g", ExpansionOrigin::Backward)];
        let result = annotator
            .annotate_single(input(PRIMARY_CODE, 1), forward, backward)
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 2);
        assert_eq!(result.metadata.backward_nodes, 0);
    }

    #[tokio::test]
    async fn test_duplicate_entity_dropped() {
        let annotator = RelationAnnotator::new(expansion_config(4, true));
        let forward = vec![
            unit(1, "dup", ExpansionOrigin::Forward),
            unit(2, "ok", ExpansionOrigin::Forward),
            unit(2, "again", ExpansionOrigin::Forward),
        ];
        let result = annotator
            .annotate_single(input(PRIMARY_CODE, 1), forward, Vec::new())
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 1);
    }

    #[tokio::test]
    async fn test_callers_excluded_when_disabled() {
        let annotator = RelationAnnotator::new(expansion_config(4, false));
        let result = annotator
            .annotate_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 1);
        assert_eq!(result.metadata.backward_nodes, 0);
        assert!(!result.annotated_content.contains("called by"));
    }

    #[tokio::test]
    async fn test_expansion_disabled_drops_units() {
        let annotator = RelationAnnotator::new(RelationAnnotationConfig::new().enable(true));
        let result = annotator
            .annotate_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                vec![unit(3, "c", ExpansionOrigin::Backward)],
            )
            .await
            .expect("annotation ok");
        assert!(!result.metadata.expanded);
        assert_eq!(result.metadata.expanded_nodes, 0);
    }

    #[tokio::test]
    async fn test_annotation_disabled_shortcuts() {
        let annotator = RelationAnnotator::new(RelationAnnotationConfig::new());
        let result = annotator
            .annotate_single(
                input(PRIMARY_CODE, 1),
                vec![unit(2, "b", ExpansionOrigin::Forward)],
                Vec::new(),
            )
            .await
            .expect("annotation ok");
        assert!(!result.metadata.expanded);
        assert_eq!(result.annotated_content, PRIMARY_CODE);
    }

    #[tokio::test]
    async fn test_stdlib_and_external_filtered() {
        let annotator = RelationAnnotator::new(expansion_config(4, true));
        let stdlib = unit(2, "b", ExpansionOrigin::Forward).with_stdlib(true);
        let external = unit(3, "c", ExpansionOrigin::Forward).with_external(true);
        let ok = unit(4, "d", ExpansionOrigin::Forward);
        let result = annotator
            .annotate_single(
                input(PRIMARY_CODE, 1),
                vec![stdlib, external, ok],
                Vec::new(),
            )
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 1);
        assert!(result.annotated_content.contains("fn d()"));
        assert!(!result.annotated_content.contains("fn b()"));
        assert!(!result.annotated_content.contains("fn c()"));
    }

    #[tokio::test]
    async fn test_structural_edge_filtered_unless_allowed() {
        use cce_types::RelationType;

        let structural =
            || unit(2, "b", ExpansionOrigin::Forward).with_relation_type(RelationType::Inheritance);

        let annotator = RelationAnnotator::new(expansion_config(4, true));
        let result = annotator
            .annotate_single(input(PRIMARY_CODE, 1), vec![structural()], Vec::new())
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 0);

        let config = expansion_config(4, true).with_structural_edges(true);
        let annotator = RelationAnnotator::new(config);
        let result = annotator
            .annotate_single(input(PRIMARY_CODE, 1), vec![structural()], Vec::new())
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 1);
    }

    #[tokio::test]
    async fn test_direction_internal_score_order() {
        let annotator = RelationAnnotator::new(expansion_config(1, true));
        // Low score arrives first; the high-score unit must win the single slot.
        let forward = vec![
            unit_scored(10, "low", ExpansionOrigin::Forward, 0.2),
            unit_scored(11, "high", ExpansionOrigin::Forward, 0.9),
        ];
        let result = annotator
            .annotate_single(input(PRIMARY_CODE, 1), forward, Vec::new())
            .await
            .expect("annotation ok");
        assert_eq!(result.metadata.forward_nodes, 1);
        assert!(result.annotated_content.contains("fn high()"));
        assert!(!result.annotated_content.contains("fn low()"));
    }

    #[tokio::test]
    async fn test_truncated_uses_token_basis() {
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .with_max_length(1000);
        let annotator = RelationAnnotator::new(config);
        // More than 1000 bytes but far fewer tokens: byte comparison would
        // report truncation, token comparison must not.
        let big = format!(
            "{}\n{}\n{}",
            "x".repeat(400),
            "y".repeat(400),
            "z".repeat(400)
        );
        let result = annotator
            .annotate_single(input(&big, 1), Vec::new(), Vec::new())
            .await
            .expect("annotation ok");
        assert!(result.annotated_content.len() > 1000);
        assert!(!result.metadata.truncated);
    }

    #[tokio::test]
    async fn test_batch_keeps_all_results() {
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .with_max_length(1000);
        let annotator = RelationAnnotator::new(config);
        let results = vec![
            (input(PRIMARY_CODE, 1), Vec::new(), Vec::new()),
            (input(PRIMARY_CODE, 2), Vec::new(), Vec::new()),
            (input(PRIMARY_CODE, 3), Vec::new(), Vec::new()),
        ];
        let annotated = annotator
            .annotate_batch(results)
            .await
            .expect("annotation ok");
        assert_eq!(annotated.len(), 3);
        for result in &annotated {
            assert!(result.annotated_content.contains("call_b"));
        }
    }
}
