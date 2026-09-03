//! Importance decision module
//!
//! Simplified decision logic for determining file importance and generation strategy.
//! Based on ParsedFile and entity group types, focusing on design patterns and file characteristics.

use crate::grouper::{GroupType, ProcessingResult};
use crate::summary::strategy::categorization::FileCategory;
use crate::summary::strategy::doc_quality::{
    AggregateQuality, DocCommentQuality, calculate_aggregate_quality,
};
use crate::summary::types::GenerationDecision;
use crate::summary::{SummaryConfig, SummaryStrategy};
use cce_types::ParsedFile;

/// Importance level for file summaries
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ImportanceLevel {
    /// Low importance - simple files, utilities, tests, boilerplate patterns
    Low,
    /// Medium importance - regular source files
    #[default]
    Medium,
    /// High importance - core modules, main files, complex logic
    High,
}

/// Decision context containing all information needed for importance decision
pub struct DecisionContext<'a> {
    /// Processing result from pre-processor
    pub processing_result: &'a ProcessingResult,
    /// Parsed file information
    pub parsed_file: &'a ParsedFile,
}

impl<'a> DecisionContext<'a> {
    /// Create a new decision context
    pub fn new(processing_result: &'a ProcessingResult, parsed_file: &'a ParsedFile) -> Self {
        Self {
            processing_result,
            parsed_file,
        }
    }
}

/// Importance decision engine
///
/// Simplified decision logic based on file characteristics and design patterns.
pub struct ImportanceDecision;

impl ImportanceDecision {
    /// Determine importance level based on file characteristics and design patterns
    ///
    /// Simplified logic focusing on:
    /// 1. File category (test, config, core module, etc.)
    /// 2. Design patterns (boilerplate patterns have lower importance)
    /// 3. Entity count and complexity
    pub fn determine_importance(
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> ImportanceLevel {
        // Step 1: Check file category
        // Test and generated files are always low importance (orthogonal
        // markers detected independently of the content type).
        if FileCategory::is_test_file(&parsed_file.path)
            || FileCategory::is_generated_file(&parsed_file.path, &parsed_file.source)
        {
            return ImportanceLevel::Low;
        }

        // Special categories have fixed importance
        match FileCategory::determine(parsed_file) {
            FileCategory::Config | FileCategory::Other => {
                return ImportanceLevel::Low;
            }
            FileCategory::Documentation | FileCategory::Schema => {
                return ImportanceLevel::Medium;
            }
            FileCategory::Code => {
                // Continue to detailed analysis
            }
        }

        // Step 2: Check if core module (main, lib, mod, index)
        if FileCategory::is_core_module(&parsed_file.path) {
            return ImportanceLevel::High;
        }

        // Step 3: Analyze design patterns
        let pattern_analysis = Self::analyze_design_patterns(processing_result);

        // If most groups are boilerplate patterns, lower importance
        if pattern_analysis.boilerplate_ratio > 0.7 {
            return ImportanceLevel::Low;
        }

        // Step 4: Check entity count and complexity
        let entity_count = parsed_file.entities.len();

        // High entity count indicates important file
        if entity_count >= 15 {
            return ImportanceLevel::High;
        }

        // Check for complex structures (multiple classes/traits)
        let complex_type_count = processing_result
            .groups
            .iter()
            .filter(|g| {
                matches!(
                    g.group_type,
                    GroupType::ClassWithMethods | GroupType::TraitWithImpls
                )
            })
            .count();

        if complex_type_count >= 2 {
            return ImportanceLevel::High;
        }

        // Step 5: Check for exports (public API)
        let exports = crate::relation_helpers::extract_exports_from_entities(
            &parsed_file.entities,
            &parsed_file.language,
        );
        if !exports.is_empty() {
            return ImportanceLevel::High;
        }

        // Step 6: Low entity count indicates simple file
        if entity_count < 5 {
            return ImportanceLevel::Low;
        }

        // Default to medium importance
        ImportanceLevel::Medium
    }

    /// Analyze design patterns in the file
    ///
    /// Returns information about boilerplate vs significant patterns
    fn analyze_design_patterns(processing_result: &ProcessingResult) -> PatternAnalysis {
        let total_groups = processing_result.groups.len();

        if total_groups == 0 {
            return PatternAnalysis::default();
        }

        let mut boilerplate_count = 0;

        for group in &processing_result.groups {
            // Check if group has a boilerplate design pattern
            let pattern_info = &group.pattern_info;
            // Getter/setter groups are mostly boilerplate code
            if pattern_info.is_getter_setter() {
                boilerplate_count += 1;
            }

            // Standalone simple functions are likely utilities
            if group.group_type == GroupType::Standalone {
                // Check if it's a simple function (no children, short span)
                if group.members.is_empty() && group.span.len() < 200 {
                    boilerplate_count += 1;
                }
            }
        }

        let boilerplate_ratio = boilerplate_count as f32 / total_groups as f32;

        PatternAnalysis { boilerplate_ratio }
    }

    /// Calculate documentation quality for all entities
    ///
    /// Returns aggregate quality statistics including coverage ratio and quality scores.
    ///
    /// Note: documentation_ratio is calculated as (well_documented_entities / total_public_entities),
    /// not just among entities that have documentation.
    fn calculate_documentation_quality(parsed_file: &ParsedFile) -> AggregateQuality {
        use crate::summary::strategy::categorization::is_entity_public;

        let public_entities: Vec<_> = parsed_file
            .entities
            .iter()
            .filter(|e| is_entity_public(e))
            .collect();

        let total_public = public_entities.len();

        if total_public == 0 {
            // No public entities means this is an internal implementation file
            // Return default which will trigger rule-based generation
            return AggregateQuality::default();
        }

        // Evaluate quality for entities that have documentation
        let qualities: Vec<DocCommentQuality> = public_entities
            .iter()
            .filter_map(|e| e.doc_comment.as_ref())
            .map(|doc| DocCommentQuality::evaluate(doc))
            .collect();

        // Calculate aggregate with awareness of total public entities
        let mut aggregate = calculate_aggregate_quality(&qualities);

        // Recalculate documentation_ratio based on total public entities, not just documented ones
        // This gives true coverage: how many public entities are well-documented
        if total_public > 0 {
            aggregate.documentation_ratio =
                aggregate.well_documented_count as f32 / total_public as f32;
            aggregate.total_entities = total_public; // Track actual total for logging
        }

        aggregate
    }

    /// Determine generation decision based on file characteristics
    ///
    /// Simplified logic focusing on design patterns and documentation.
    pub fn determine_generation_strategy(
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
        config: &SummaryConfig,
    ) -> GenerationDecision {
        match config.strategy {
            SummaryStrategy::RuleBased => GenerationDecision::RuleOnly,
            SummaryStrategy::Minimal => GenerationDecision::RuleOnly,
            SummaryStrategy::Auto | SummaryStrategy::ModelEnhanced => {
                // Check for special file categories (test/config/documentation/
                // generated files skip model enhancement; schema files stay eligible)
                if FileCategory::should_skip_model_enhancement(parsed_file) {
                    return GenerationDecision::RuleOnly;
                }

                // Calculate documentation quality (new approach)
                let doc_quality = Self::calculate_documentation_quality(parsed_file);

                // High quality documentation coverage - use rule-based
                // This considers both coverage ratio AND quality score
                if doc_quality.should_skip_model_enhancement() {
                    return GenerationDecision::RuleOnly;
                }

                // Analyze design patterns
                let pattern_analysis = Self::analyze_design_patterns(processing_result);

                // High boilerplate ratio - use rule-based (patterns are well-described by rules)
                if pattern_analysis.boilerplate_ratio > 0.6 {
                    return GenerationDecision::RuleOnly;
                }

                // Core modules need model enhancement
                if FileCategory::is_core_module(&parsed_file.path) {
                    return GenerationDecision::ModelEnhanced;
                }

                // Complex files need model enhancement
                let entity_count = parsed_file.entities.len();
                if entity_count >= 10 || processing_result.groups.len() >= 7 {
                    return GenerationDecision::ModelEnhanced;
                }

                // Default to rule-based for simple files
                GenerationDecision::RuleOnly
            }
        }
    }

    /// Determine if model-enhanced generation should be used
    ///
    /// Simplified decision based on importance and patterns.
    pub fn should_use_model_enhancement(
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
        config: &SummaryConfig,
    ) -> bool {
        matches!(
            Self::determine_generation_strategy(parsed_file, processing_result, config),
            GenerationDecision::ModelEnhanced
        )
    }
}

/// Pattern analysis result
#[derive(Debug, Clone, Default)]
struct PatternAnalysis {
    /// Ratio of boilerplate patterns
    boilerplate_ratio: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::ProcessingStats;
    use cce_types::{Entity, EntityId, EntityKind, Language};

    #[test]
    fn test_importance_level_default() {
        let level = ImportanceLevel::default();
        assert_eq!(level, ImportanceLevel::Medium);
    }

    #[test]
    fn test_is_core_module() {
        assert!(FileCategory::is_core_module("src/main.rs"));
        assert!(FileCategory::is_core_module("src/lib.rs"));
        assert!(FileCategory::is_core_module("src/mod.rs"));
        assert!(FileCategory::is_core_module("src/index.js"));
        assert!(!FileCategory::is_core_module("src/utils.rs"));
        assert!(!FileCategory::is_core_module("src/helper.rs"));
        assert!(!FileCategory::is_core_module("src/library.rs"));
        assert!(!FileCategory::is_core_module("src/cmd_indexer.rs"));
        assert!(!FileCategory::is_core_module("src/commodity.rs"));
        assert!(!FileCategory::is_core_module("src/imodal.rs"));
    }

    #[test]
    fn test_determine_importance_core_module() {
        let mut parsed_file =
            ParsedFile::new(Language::Rust, "src/main.rs".to_string(), "fn main() {}");
        parsed_file.add_entity(Entity::new(
            EntityId(0),
            EntityKind::Function,
            "main".to_string(),
            cce_types::Span::default(),
        ));

        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        };

        let importance = ImportanceDecision::determine_importance(&parsed_file, &processing_result);
        assert_eq!(importance, ImportanceLevel::High);
    }

    #[test]
    fn test_determine_importance_test_file() {
        let parsed_file = ParsedFile::new(Language::Rust, "src/main_test.rs".to_string(), "");

        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        };

        let importance = ImportanceDecision::determine_importance(&parsed_file, &processing_result);
        assert_eq!(importance, ImportanceLevel::Low);
    }

    #[test]
    fn test_determine_importance_high_entity_count() {
        let mut parsed_file = ParsedFile::new(Language::Rust, "src/complex.rs".to_string(), "");

        // Add 15 entities
        for i in 0..15 {
            parsed_file.add_entity(Entity::new(
                EntityId(i),
                EntityKind::Function,
                format!("func_{}", i),
                cce_types::Span::default(),
            ));
        }

        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        };

        let importance = ImportanceDecision::determine_importance(&parsed_file, &processing_result);
        assert_eq!(importance, ImportanceLevel::High);
    }

    #[test]
    fn test_analyze_design_patterns_empty() {
        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        };

        let analysis = ImportanceDecision::analyze_design_patterns(&processing_result);
        assert_eq!(analysis.boilerplate_ratio, 0.0);
    }

    #[test]
    fn test_documentation_quality_coverage_calculation() {
        // Test that documentation_ratio is calculated correctly based on total public entities
        use cce_types::Span;

        let mut parsed_file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");

        // Add 10 public entities (using signature to indicate public)
        for i in 0..10 {
            let mut entity = Entity::new(
                EntityId(i),
                EntityKind::Function,
                format!("public_func_{}", i),
                Span::default(),
            );
            // Set signature to indicate public access
            entity.signature = format!("pub fn public_func_{}()", i);

            // Only 3 out of 10 have documentation
            if i < 3 {
                entity.doc_comment =
                    Some("/// Good documentation\n/// with multiple lines".to_string());
            }

            parsed_file.add_entity(entity);
        }

        let quality = ImportanceDecision::calculate_documentation_quality(&parsed_file);

        // Should track total public entities
        assert_eq!(quality.total_entities, 10);

        // well_documented_count should be the number of entities with quality >= 0.5
        // The doc comment "Good documentation with multiple lines" should score reasonably
        assert!(quality.well_documented_count <= 3); // At most 3 documented entities

        // Coverage ratio should be well_documented / total_public (not well_documented / documented)
        // If all 3 docs are well-documented: 3/10 = 0.3
        // This should NOT trigger skip_model_enhancement (needs >= 0.6)
        assert!(quality.documentation_ratio <= 0.3);
        assert!(!quality.should_skip_model_enhancement());
    }
}
