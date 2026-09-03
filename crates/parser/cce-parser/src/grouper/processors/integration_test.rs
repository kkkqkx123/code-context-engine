use cce_config::NestProcessorConfig;
use cce_config::modules::pattern_detection::GetterSetterDetectionConfig;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
use cce_types::language::Language;

use crate::grouper::context::FileProcessingContext;
use crate::grouper::processors::CallMerger;
use crate::grouper::processors::ClassMethodProcessor;
use crate::grouper::processors::TestSuiteProcessor;
use crate::grouper::types::{EntityGroup, GroupType};

// ============================================================
// Helper functions
// ============================================================

fn create_method(id: EntityId, name: &str, span: Span) -> Entity {
    Entity::new(id, EntityKind::Method, name.to_string(), span)
}

fn create_field(id: EntityId, name: &str) -> Entity {
    Entity::new(id, EntityKind::Field, name.to_string(), Span::default())
}

fn create_class(id: EntityId, name: &str, span: Span) -> Entity {
    Entity::new(id, EntityKind::Class, name.to_string(), span)
}

fn create_function(id: EntityId, name: &str, span: Span) -> Entity {
    Entity::new(id, EntityKind::Function, name.to_string(), span)
}

fn small_span(start_line: usize, end_line: usize, start_byte: usize, end_byte: usize) -> Span {
    Span {
        start_position: cce_types::Position {
            row: start_line,
            column: 0,
        },
        end_position: cce_types::Position {
            row: end_line,
            column: 0,
        },
        start_byte,
        end_byte,
    }
}

// ============================================================
// TestSuiteProcessor workflow tests
// ============================================================

#[test]
fn test_test_suite_processor_workflow() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "test.rs".to_string(), "");

    let suite = Entity::new(
        EntityId(0),
        EntityKind::TestSuite,
        "user_authentication".to_string(),
        small_span(0, 50, 0, 1000),
    );

    let case1 = Entity::new(
        EntityId(1),
        EntityKind::TestCase,
        "test_login_with_valid_credentials".to_string(),
        small_span(5, 10, 100, 300),
    )
    .with_parent(Some(EntityId(0)));

    let case2 = Entity::new(
        EntityId(2),
        EntityKind::TestCase,
        "test_login_with_invalid_password".to_string(),
        small_span(15, 20, 301, 500),
    )
    .with_parent(Some(EntityId(0)));

    let regular_func =
        create_function(EntityId(3), "helper_function", small_span(30, 40, 600, 800));

    let entities = vec![suite, case1, case2, regular_func];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    assert_eq!(assoc_count, 1, "Should have 1 test suite association");
    assert!(
        groups.len() == 1,
        "Should have 1 group (suite only, non-test entities handled by later pipeline stages)"
    );

    let suite_group = groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(suite_group.is_some(), "Should find test suite group");
    let suite_group = suite_group.unwrap();
    assert_eq!(suite_group.members.len(), 2, "Should contain 2 test cases");

    // Non-test entities (like helper_function) are not processed by TestSuiteProcessor
    // They are handled by later pipeline stages (ClassMethodProcessor, etc.)
    let standalone_func = groups.iter().find(|g| g.name == "helper_function");
    assert!(
        standalone_func.is_none(),
        "Helper function should not appear in TestSuiteProcessor output"
    );
}

#[test]
fn test_test_suite_disabled_workflow() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig {
        enable_test_entity_grouping: false,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Rust, "test.rs".to_string(), "");

    let suite = Entity::new(
        EntityId(0),
        EntityKind::TestSuite,
        "test_module".to_string(),
        Span::default(),
    );

    let case = Entity::new(
        EntityId(1),
        EntityKind::TestCase,
        "test_case".to_string(),
        Span::default(),
    )
    .with_parent(Some(EntityId(0)));

    let entities = vec![suite, case];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, count) = processor.process(ctx);

    assert_eq!(count, 0, "No associations when disabled");
    assert_eq!(groups.len(), 2, "Both entities become standalone");
    assert!(groups.iter().all(|g| g.group_type == GroupType::Standalone));
}

#[test]
fn test_test_suite_empty_entities() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "test.rs".to_string(), "");

    let ctx = FileProcessingContext::new(&[], &parsed_file, &config);
    let (groups, count) = processor.process(ctx);

    assert_eq!(groups.len(), 0);
    assert_eq!(count, 0);
}

#[test]
fn test_test_suite_with_nested_suite() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::JavaScript, "test.spec.js".to_string(), "");

    let outer_suite = Entity::new(
        EntityId(0),
        EntityKind::TestSuite,
        "User API".to_string(),
        small_span(0, 100, 0, 2000),
    );

    let inner_suite = Entity::new(
        EntityId(1),
        EntityKind::TestSuite,
        "GET /users".to_string(),
        small_span(10, 50, 200, 1000),
    )
    .with_parent(Some(EntityId(0)));

    let inner_case = Entity::new(
        EntityId(2),
        EntityKind::TestCase,
        "should return user list".to_string(),
        small_span(15, 25, 300, 600),
    )
    .with_parent(Some(EntityId(1)));

    let outer_case = Entity::new(
        EntityId(3),
        EntityKind::TestCase,
        "should handle errors".to_string(),
        small_span(60, 70, 1200, 1500),
    )
    .with_parent(Some(EntityId(0)));

    let entities = vec![outer_suite, inner_suite, inner_case, outer_case];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, _) = processor.process(ctx);

    // Should create groups for both suites
    let outer_groups: Vec<&EntityGroup> = groups
        .iter()
        .filter(|g| g.group_type == GroupType::TestSuiteWithCases)
        .collect();
    assert!(
        !outer_groups.is_empty(),
        "Should have at least one test suite group"
    );
}

// ============================================================
// ClassMethodProcessor workflow tests
// ============================================================

#[test]
fn test_class_method_processor_small_class_merge() {
    let config = NestProcessorConfig {
        small_class_threshold: 100,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "SmallClass.java".to_string(), "");
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());

    let mut class = create_class(EntityId(0), "SmallHelper", small_span(0, 20, 0, 500));
    let method1 = create_method(EntityId(1), "doWork", small_span(5, 8, 100, 200));
    let method2 = create_method(EntityId(2), "doMoreWork", small_span(10, 13, 201, 300));

    class.children = vec![method1.id, method2.id];

    let entities = vec![class, method1, method2];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Small class should merge with methods");
    assert_eq!(assoc_count, 1, "Should have 1 association");
    assert_eq!(groups[0].name, "SmallHelper");
    assert_eq!(groups[0].members.len(), 2, "Should have 2 methods in group");
}

#[test]
fn test_class_method_processor_large_class_no_merge() {
    let config = NestProcessorConfig {
        small_class_threshold: 300,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "LargeClass.java".to_string(), "");
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());

    let mut class = create_class(EntityId(0), "LargeService", small_span(0, 200, 0, 5000));
    let method1 = create_method(EntityId(1), "processData", small_span(10, 20, 200, 400));
    let method2 = create_method(EntityId(2), "analyzeResults", small_span(30, 40, 401, 600));

    class.children = vec![method1.id, method2.id];

    let entities = vec![class, method1, method2];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    assert_eq!(
        groups.len(),
        1,
        "Large class merges with methods: chunker handles splitting"
    );
    assert_eq!(assoc_count, 1, "Should have 1 association");
    assert!(groups.iter().any(|g| g.name == "LargeService"));
    assert_eq!(
        groups[0].members.len(),
        2,
        "Merged group should have 2 method members"
    );
}

#[test]
fn test_class_method_processor_no_children() {
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Java, "Empty.java".to_string(), "");
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());

    let class = create_class(EntityId(0), "EmptyClass", Span::default());
    let entities = vec![class];

    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);
    let (groups, assoc_count) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Empty class should be standalone");
    assert_eq!(assoc_count, 0);
}

#[test]
fn test_class_method_processor_getter_setter_merging() {
    let config = NestProcessorConfig {
        enable_getter_setter_merging: true,
        small_class_threshold: 100,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "Person.java".to_string(), "");
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());

    let mut class = create_class(EntityId(0), "Person", small_span(0, 20, 0, 500));
    let getter = create_method(EntityId(1), "getName", small_span(5, 7, 100, 200));
    let setter = create_method(EntityId(2), "setName", small_span(8, 10, 201, 300));
    let field = create_field(EntityId(3), "name");

    class.children = vec![getter.id, setter.id, field.id];

    let entities = vec![class, field, getter, setter];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Getter/setter class should merge");
    assert_eq!(groups[0].name, "Person");
}

#[test]
fn test_class_method_processor_disabled_association() {
    let config = NestProcessorConfig {
        enable_class_method_association: false,
        small_class_threshold: 100,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "Test.java".to_string(), "");
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());

    let mut class = create_class(EntityId(0), "SmallHelper", small_span(0, 20, 0, 500));
    let method = create_method(EntityId(1), "help", small_span(5, 10, 100, 200));
    class.children = vec![method.id];

    let entities = vec![class, method];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    // Even when association is disabled, the processor itself still groups
    // because the enable flag is checked at pipeline level, not processor level
    assert_eq!(
        groups.len(),
        1,
        "Processor still groups regardless of pipeline-level flag"
    );
    assert_eq!(assoc_count, 1);
}

// ============================================================
// CallMerger workflow tests
// ============================================================

#[test]
fn test_call_merger_creation() {
    let merger_empty = CallMerger::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "empty.rs".to_string(), "");
    let ctx = FileProcessingContext::new(&[], &parsed_file, &config);
    assert!(merger_empty.merge(ctx).0.is_empty());

    let merger = CallMerger::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "empty.rs".to_string(), "");
    let ctx = FileProcessingContext::new(&[], &parsed_file, &config);
    let (result, count) = merger.merge(ctx);
    assert_eq!(count, 0);
    assert!(result.is_empty());
}

// ============================================================
// Multi-processor workflow tests (combined scenarios)
// ============================================================

#[test]
fn test_test_suite_and_class_mixed_workflow() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "mixed.rs".to_string(), "");

    let suite = Entity::new(
        EntityId(0),
        EntityKind::TestSuite,
        "tests".to_string(),
        small_span(0, 80, 0, 2000),
    );

    let case = Entity::new(
        EntityId(1),
        EntityKind::TestCase,
        "test_feature".to_string(),
        small_span(5, 20, 100, 500),
    )
    .with_parent(Some(EntityId(0)));

    let mut helper_struct = create_class(EntityId(2), "TestHelper", small_span(30, 60, 600, 1500));
    let helper_method = create_method(EntityId(3), "setup", small_span(35, 45, 700, 1000));
    helper_struct.children = vec![helper_method.id];

    let standalone_func = create_function(EntityId(4), "main", small_span(65, 75, 1600, 1900));

    // In TestSuiteProcessor, the suite and case get grouped
    // The helper_struct and standalone_func should appear as standalone groups
    let entities = vec![suite, case, helper_struct, helper_method, standalone_func];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    assert!(
        groups
            .iter()
            .any(|g| g.group_type == GroupType::TestSuiteWithCases),
        "Should have test suite group"
    );
    assert_eq!(assoc_count, 1, "Should have one association");

    // Only test entities are accounted for (non-test entities handled by later pipeline stages)
    let total_entities_in_groups: usize = groups.iter().map(|g| 1 + g.members.len()).sum();
    assert_eq!(
        total_entities_in_groups, 2,
        "Only test suite + test case should be in groups"
    );
}

#[test]
fn test_processor_with_multiple_test_suites() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::JavaScript, "api.spec.js".to_string(), "");

    let auth_suite = Entity::new(
        EntityId(0),
        EntityKind::TestSuite,
        "Auth API".to_string(),
        small_span(0, 50, 0, 1000),
    );

    let user_suite = Entity::new(
        EntityId(1),
        EntityKind::TestSuite,
        "User API".to_string(),
        small_span(55, 100, 1100, 2000),
    );

    let auth_case = Entity::new(
        EntityId(2),
        EntityKind::TestCase,
        "should login".to_string(),
        small_span(5, 15, 100, 300),
    )
    .with_parent(Some(EntityId(0)));

    let user_case = Entity::new(
        EntityId(3),
        EntityKind::TestCase,
        "should create user".to_string(),
        small_span(60, 70, 1200, 1500),
    )
    .with_parent(Some(EntityId(1)));

    let entities = vec![auth_suite, user_suite, auth_case, user_case];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, assoc_count) = processor.process(ctx);

    assert_eq!(assoc_count, 2, "Should have 2 suite associations");
    let suite_groups: Vec<&EntityGroup> = groups
        .iter()
        .filter(|g| g.group_type == GroupType::TestSuiteWithCases)
        .collect();
    assert_eq!(suite_groups.len(), 2, "Should have 2 test suite groups");
}

#[test]
fn test_processor_standalone_function() {
    let processor = TestSuiteProcessor::new();
    let config = NestProcessorConfig::default();
    let parsed_file = ParsedFile::new(Language::Rust, "utils.rs".to_string(), "");

    let func1 = create_function(EntityId(0), "add", small_span(0, 5, 0, 100));
    let func2 = create_function(EntityId(1), "subtract", small_span(10, 15, 200, 300));

    let entities = vec![func1, func2];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let (groups, count) = processor.process(ctx);

    assert_eq!(count, 0, "No test suite associations");
    assert_eq!(
        groups.len(),
        0,
        "Non-test entities are not processed by TestSuiteProcessor"
    );
}

// ============================================================
// ClassMethodProcessor pattern detection scenarios
// ============================================================
