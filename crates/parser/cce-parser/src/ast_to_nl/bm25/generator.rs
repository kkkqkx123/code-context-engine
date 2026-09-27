//! BM25 text generator
//!
//! This module generates BM25-optimized text for code entities,
//! using the new group-oriented template architecture.

use crate::ast_to_nl::bm25::keyword_extractor::KeywordExtractor;
use crate::ast_to_nl::bm25::templates::GroupTemplateDispatcher;
use crate::ast_to_nl::common::create_standalone_group;
use crate::grouper::types::EntityGroup;
use cce_config::Bm25GeneratorConfig;
use cce_text::Bm25TextCleaner;
use cce_types::GroupedEntity;

/// BM25 text generator
///
/// Uses the new group-oriented template architecture for generating
/// BM25-optimized text for keyword search.
pub struct Bm25Generator {
    config: Bm25GeneratorConfig,
    bm25_text_cleaner: Bm25TextCleaner,
    keyword_extractor: KeywordExtractor,
    template_dispatcher: GroupTemplateDispatcher,
}

impl Bm25Generator {
    /// Create a new BM25 generator with configuration
    pub fn with_config(config: &Bm25GeneratorConfig) -> Self {
        Self {
            config: config.clone(),
            bm25_text_cleaner: Bm25TextCleaner::new(),
            keyword_extractor: KeywordExtractor::new(),
            template_dispatcher: GroupTemplateDispatcher::new(),
        }
    }

    /// Create a new BM25 generator with default configuration
    pub fn new() -> Self {
        Self::with_config(&Bm25GeneratorConfig::default())
    }

    /// Generate BM25 text for an entity group
    ///
    /// Returns a single text description optimized for BM25 keyword matching.
    /// Nested groups are handled as independent groups by the grouping pipeline
    /// and are not merged into the parent text here.
    pub fn generate_for_group(&self, group: &EntityGroup) -> String {
        let text = self.template_dispatcher.dispatch(group);

        self.bm25_text_cleaner.clean(&text)
    }

    /// Generate BM25 text for a single entity
    ///
    /// This is a compatibility method for standalone entities.
    /// For grouped entities, prefer `generate_for_group`.
    pub fn generate(&self, entity: &GroupedEntity) -> String {
        // Create a standalone group from the entity
        let group = create_standalone_group(entity);
        self.generate_for_group(&group)
    }

    /// Extract keywords for BM25 indexing
    ///
    /// Only named semantic entities (types, functions, methods, constants, modules,
    /// etc.) produce keywords. Structural entities (macros, annotations, impl blocks,
    /// fields, variables, CSS rules, template internals, test hooks, etc.) return
    /// an empty vector.
    pub fn extract_keywords(&self, entity: &GroupedEntity) -> Vec<String> {
        if !entity.kind.is_named_semantic_entity() {
            return Vec::new();
        }
        let keywords = self.keyword_extractor.extract(entity);
        keywords
            .into_iter()
            .take(self.config.max_keywords)
            .collect()
    }
}

impl Default for Bm25Generator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use cce_types::{EntityId, EntityKind};

    fn create_test_function() -> GroupedEntity {
        GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "calculate_total_price".to_string(),
            signature: "fn calculate_total_price(price: f64, quantity: i32) -> f64".to_string(),
            parameters: smallvec::smallvec![
                ("price".into(), Some("f64".into())),
                ("quantity".into(), Some("i32".into())),
            ],
            return_type: Some("f64".to_string()),
            doc_comment: Some("/// Calculates the total price.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_generate_function() {
        let generator = Bm25Generator::new();
        let entity = create_test_function();
        let text = generator.generate(&entity);

        // Should not be empty
        assert!(!text.is_empty());
    }

    #[test]
    fn test_extract_keywords() {
        let generator = Bm25Generator::new();
        let entity = create_test_function();
        let keywords = generator.extract_keywords(&entity);

        // Should contain the function name
        assert!(keywords.contains(&"calculate_total_price".to_string()));
    }

    #[test]
    fn test_generate_struct() {
        let generator = Bm25Generator::new();
        let entity = GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Struct,
            name: "User".to_string(),
            signature: "struct User".to_string(),
            parameters: smallvec::smallvec![
                ("id".into(), Some("i32".into())),
                ("name".into(), Some("String".into())),
            ],
            return_type: None,
            doc_comment: Some("/// Represents a user.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };

        let text = generator.generate(&entity);
        // Should not be empty
        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_for_group() {
        let generator = Bm25Generator::new();
        let group = EntityGroup::default();
        let text = generator.generate_for_group(&group);

        // Should not be empty
        assert!(!text.is_empty());
    }
}
