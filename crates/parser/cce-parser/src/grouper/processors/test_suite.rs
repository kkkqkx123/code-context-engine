//! Test suite processor
//!
//! This processor groups test suites with their test cases, similar to how
//! ClassMethodProcessor groups classes with their methods.
//!
//! # Supported Test Frameworks
//!
//! - JavaScript/TypeScript: Jest, Vitest, Mocha (describe/it)
//! - Rust: Built-in test framework (#[cfg(test)] + #[test])

use crate::grouper::context::FileProcessingContext;

use crate::grouper::types::EntityGroup;
use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

/// Test suite processor
///
/// Groups test suites (describe, #[cfg(test)] mod) with their test cases
/// and handles nested test suites with configurable depth limits.
pub struct TestSuiteProcessor {}

impl Default for TestSuiteProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl TestSuiteProcessor {
    /// Create a new processor
    pub fn new() -> Self {
        Self {}
    }
}

impl TestSuiteProcessor {
    /// Process test suites and their test cases
    ///
    /// Returns a tuple of (groups, association_count)
    pub fn process(&self, ctx: FileProcessingContext) -> (Vec<EntityGroup>, usize) {
        let mut groups = Vec::new();
        let mut processed_ids = std::collections::HashSet::new();
        let mut association_count = 0;
        let language = *ctx.language();

        // Check if test entity grouping is enabled
        if !ctx.config.enable_test_entity_grouping {
            // Return all entities as standalone groups
            for entity in ctx.entities {
                groups.push(EntityGroup::from_entity(entity.clone(), language));
                processed_ids.insert(entity.id);
            }
            return (groups, 0);
        }

        // Step 1: Find all test suites
        let test_suites: Vec<&Entity> = ctx
            .entities
            .iter()
            .filter(|e| e.kind == EntityKind::TestSuite)
            .collect();

        // Step 2: Process each test suite and find its test cases
        for suite in test_suites {
            if processed_ids.contains(&suite.id) {
                continue;
            }

            // Find test cases that belong to this suite
            // Test cases are either:
            // 1. Children of the test suite entity (parent-child relationship)
            // 2. Entities within the suite's span that are test cases
            let test_cases = self.find_test_cases_for_suite(suite, ctx.entities, &language);

            // Check nesting depth
            let nesting_depth = self.calculate_nesting_depth(suite, ctx.entities);
            if nesting_depth > ctx.config.max_test_suite_nesting {
                tracing::warn!(
                    "Test suite '{}' exceeds maximum nesting depth ({} > {}), flattening",
                    suite.name,
                    nesting_depth,
                    ctx.config.max_test_suite_nesting
                );

                // Flatten: create standalone group for this suite
                groups.push(EntityGroup::from_entity(suite.clone(), language));
                processed_ids.insert(suite.id);

                // Add test cases as standalone groups too
                for case in &test_cases {
                    if !processed_ids.contains(&case.id) {
                        groups.push(EntityGroup::from_entity(case.clone(), language));
                        processed_ids.insert(case.id);
                    }
                }
                continue;
            }

            // Create test suite with cases group
            if test_cases.is_empty() {
                // No test cases, but still preserve TestSuiteWithCases type identity
                let group = EntityGroup::test_suite_with_cases(suite.clone(), Vec::new(), language);
                groups.push(group);
            } else {
                // Create grouped test suite
                let group =
                    EntityGroup::test_suite_with_cases(suite.clone(), test_cases.clone(), language);
                groups.push(group);
                association_count += 1;

                // Mark test cases as processed
                for case in &test_cases {
                    processed_ids.insert(case.id);
                }
            }

            processed_ids.insert(suite.id);
        }

        // Step 3: Add remaining test entities as standalone groups
        for entity in ctx.entities.iter().filter(|e| e.kind.is_test_related()) {
            if !processed_ids.contains(&entity.id) {
                groups.push(EntityGroup::from_entity(entity.clone(), language));
                processed_ids.insert(entity.id);
            }
        }

        // Note: Non-test entities are NOT processed here. They will be handled
        // by the ClassMethodProcessor in the next pipeline stage.
        // Previously this code created standalone groups for all non-test entities,
        // which prevented the ClassMethodProcessor from merging classes with methods.

        (groups, association_count)
    }

    /// Find test cases that belong to a test suite
    fn find_test_cases_for_suite(
        &self,
        suite: &Entity,
        entities: &[Entity],
        _language: &Language,
    ) -> Vec<Entity> {
        let mut test_cases = Vec::new();

        // Method 1: Check parent-child relationship
        // If test cases have this suite as their parent, they belong here
        for entity in entities.iter().filter(|e| e.kind == EntityKind::TestCase) {
            if let Some(parent_id) = entity.parent {
                if parent_id == suite.id {
                    test_cases.push(entity.clone());
                    continue;
                }
            }

            // Method 2: Check if test case is within suite's span
            // This handles cases where parent-child relationship isn't set
            if entity.span.start_byte >= suite.span.start_byte
                && entity.span.end_byte <= suite.span.end_byte
            {
                // Additional check: ensure it's a direct child, not from nested suite
                if !self.is_in_nested_suite(entity, suite, entities) {
                    test_cases.push(entity.clone());
                }
            }
        }

        // Sort by position in source code to maintain order
        test_cases.sort_by_key(|tc| tc.span.start_byte);

        test_cases
    }

    /// Check if a test case is within a nested suite (not the target suite)
    fn is_in_nested_suite(
        &self,
        test_case: &Entity,
        target_suite: &Entity,
        entities: &[Entity],
    ) -> bool {
        // Find if there's another test suite between target_suite and test_case
        for entity in entities
            .iter()
            .filter(|e| e.kind == EntityKind::TestSuite && e.id != target_suite.id)
        {
            // Check if this suite is inside target_suite
            if entity.span.start_byte >= target_suite.span.start_byte
                && entity.span.end_byte <= target_suite.span.end_byte
            {
                // Check if test_case is inside this nested suite
                if test_case.span.start_byte >= entity.span.start_byte
                    && test_case.span.end_byte <= entity.span.end_byte
                {
                    return true;
                }
            }
        }
        false
    }

    /// Calculate the nesting depth of a test suite
    fn calculate_nesting_depth(&self, suite: &Entity, entities: &[Entity]) -> usize {
        let mut depth = 1;
        let mut current_suite = suite;

        // Walk up the parent chain counting test suites
        while let Some(parent_id) = current_suite.parent {
            if let Some(parent) = entities.iter().find(|e| e.id == parent_id) {
                if parent.kind == EntityKind::TestSuite {
                    depth += 1;
                    current_suite = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::GroupType;
    use cce_config::NestProcessorConfig;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};

    #[test]
    fn test_processor_creation() {
        let _processor = TestSuiteProcessor::new();
        // TestSuiteProcessor is a zero-sized type (no fields),
        // so we just verify it can be created
    }

    #[test]
    fn test_process_empty_entities() {
        let processor = TestSuiteProcessor::new();
        let config = NestProcessorConfig::default();
        let parsed_file =
            cce_types::entity::ParsedFile::new(Language::Rust, "test.rs".to_string(), "");
        let entities: Vec<Entity> = Vec::new();
        let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

        let (groups, count) = processor.process(ctx);

        assert_eq!(groups.len(), 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_process_test_suite_with_cases() {
        let processor = TestSuiteProcessor::new();
        let config = NestProcessorConfig::default();
        let parsed_file =
            cce_types::entity::ParsedFile::new(Language::Rust, "test.rs".to_string(), "");

        // Create a test suite
        let suite = Entity::new(
            EntityId(0),
            EntityKind::TestSuite,
            "user authentication".to_string(),
            Span::default(),
        );

        // Create test cases with the suite as parent
        let case1 = Entity::new(
            EntityId(1),
            EntityKind::TestCase,
            "should login with valid credentials".to_string(),
            Span::default(),
        )
        .with_parent(Some(EntityId(0)));

        let case2 = Entity::new(
            EntityId(2),
            EntityKind::TestCase,
            "should reject invalid password".to_string(),
            Span::default(),
        )
        .with_parent(Some(EntityId(0)));

        let entities = vec![suite, case1, case2];
        let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

        let (groups, count) = processor.process(ctx);

        assert_eq!(groups.len(), 1);
        assert_eq!(count, 1);
        assert_eq!(groups[0].group_type, GroupType::TestSuiteWithCases);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn test_disabled_test_grouping() {
        let processor = TestSuiteProcessor::new();
        let config = NestProcessorConfig {
            enable_test_entity_grouping: false,
            ..Default::default()
        };

        let parsed_file =
            cce_types::entity::ParsedFile::new(Language::Rust, "test.rs".to_string(), "");

        let suite = Entity::new(
            EntityId(0),
            EntityKind::TestSuite,
            "test suite".to_string(),
            Span::default(),
        );

        let case = Entity::new(
            EntityId(1),
            EntityKind::TestCase,
            "test case".to_string(),
            Span::default(),
        )
        .with_parent(Some(EntityId(0)));

        let entities = vec![suite, case];
        let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

        let (groups, count) = processor.process(ctx);

        // When disabled, each entity should be standalone
        assert_eq!(groups.len(), 2);
        assert_eq!(count, 0);
        assert!(groups.iter().all(|g| g.group_type == GroupType::Standalone));
    }
}
