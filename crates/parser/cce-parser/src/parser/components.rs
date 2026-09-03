//! Component holder for the parsing pipeline
//!
//! Provides access to shared components like parsers, extractors, etc.
//! This avoids creating multiple instances of the same component.

use crate::parser::ast_parser::AstParser;
use crate::parser::comment_processor::CommentProcessor;
use crate::parser::embedded_types::EmbeddedParseConfig;
use crate::parser::extractor::{
    BehaviorExtractor, ControlFlowExtractor, EmbeddedParser, EntityExtractor, MacroBodyExtractor,
    RelationExtractor, StructuralExtractor,
};
use crate::parser::language_detector::LanguageDetector;
use std::sync::Arc;

/// Component holder for sharing components across pipeline stages
///
/// Provides access to shared components like parsers, extractors, etc.
/// This avoids creating multiple instances of the same component.
pub struct Components {
    /// Language detector
    pub language_detector: LanguageDetector,
    /// AST parser
    pub ast_parser: AstParser,
    /// Entity extractor
    pub entity_extractor: EntityExtractor,
    /// Behavior extractor
    pub behavior_extractor: BehaviorExtractor,
    /// Macro body extractor
    pub macro_body_extractor: MacroBodyExtractor,
    /// Control-flow extractor
    pub control_flow_extractor: ControlFlowExtractor,
    /// Relation extractor
    pub relation_extractor: RelationExtractor,
    /// Structural extractor
    pub structural_extractor: StructuralExtractor,
    /// Comment processor
    pub comment_processor: CommentProcessor,
    /// Embedded parser
    pub embedded_parser: EmbeddedParser,
}

impl Components {
    /// Create a new component holder with default components
    pub fn new() -> Self {
        Self {
            language_detector: LanguageDetector::new(),
            ast_parser: AstParser::new(),
            entity_extractor: EntityExtractor::new(),
            behavior_extractor: BehaviorExtractor::new(),
            macro_body_extractor: MacroBodyExtractor::new(),
            control_flow_extractor: ControlFlowExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            structural_extractor: StructuralExtractor::new(),
            comment_processor: CommentProcessor::new(),
            embedded_parser: EmbeddedParser::new(),
        }
    }

    /// Create a new component holder with custom embedded parse config
    pub fn with_embedded_config(config: EmbeddedParseConfig) -> Self {
        Self {
            language_detector: LanguageDetector::new(),
            ast_parser: AstParser::new(),
            entity_extractor: EntityExtractor::new(),
            behavior_extractor: BehaviorExtractor::new(),
            macro_body_extractor: MacroBodyExtractor::new(),
            control_flow_extractor: ControlFlowExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            structural_extractor: StructuralExtractor::new(),
            comment_processor: CommentProcessor::new(),
            embedded_parser: EmbeddedParser::with_config(config),
        }
    }

    /// Create a new component holder with a seeded entity ID counter.
    ///
    /// The entity extractor assigns raw entity IDs from this counter. Seeding it
    /// above the previously indexed maximum keeps hot-update parses from
    /// reusing IDs that still belong to unchanged entities.
    pub fn with_entity_id_seed(seed: u64) -> Self {
        Self {
            language_detector: LanguageDetector::new(),
            ast_parser: AstParser::new(),
            entity_extractor: EntityExtractor::new().with_id_seed(seed),
            behavior_extractor: BehaviorExtractor::new(),
            macro_body_extractor: MacroBodyExtractor::new(),
            control_flow_extractor: ControlFlowExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            structural_extractor: StructuralExtractor::new(),
            comment_processor: CommentProcessor::new(),
            embedded_parser: EmbeddedParser::new(),
        }
    }

    /// Create a new component holder wired with the plugin registry for the
    /// `LangHeuristics` entity-kind hook.
    pub fn with_plugin_registry(registry: Arc<cce_plugin::PluginRegistry>) -> Self {
        Self {
            language_detector: LanguageDetector::new(),
            ast_parser: AstParser::new(),
            entity_extractor: EntityExtractor::new().with_heuristics_registry(registry),
            behavior_extractor: BehaviorExtractor::new(),
            macro_body_extractor: MacroBodyExtractor::new(),
            control_flow_extractor: ControlFlowExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            structural_extractor: StructuralExtractor::new(),
            comment_processor: CommentProcessor::new(),
            embedded_parser: EmbeddedParser::new(),
        }
    }
}

impl Default for Components {
    fn default() -> Self {
        Self::new()
    }
}
