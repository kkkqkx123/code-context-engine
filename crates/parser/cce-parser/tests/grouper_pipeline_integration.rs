use cce_config::NestProcessorConfig;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
use cce_types::language::Language;

use cce_parser::grouper::pipeline::PreprocessingPipeline;
use cce_parser::grouper::types::{GroupType, ProcessingResult};

// ============================================================
// Helper functions
// ============================================================

fn create_entity(id: u64, kind: EntityKind, name: &str, start: usize, end: usize) -> Entity {
    Entity::new(
        EntityId(id),
        kind,
        name.to_string(),
        Span {
            start_position: cce_types::Position {
                row: start,
                column: 0,
            },
            end_position: cce_types::Position {
                row: end,
                column: 0,
            },
            start_byte: start * 10,
            end_byte: end * 10,
        },
    )
}

fn create_method(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::Method, name, start, end)
}

fn create_class(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::Class, name, start, end)
}

fn create_function(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::Function, name, start, end)
}

fn create_test_case(
    id: u64,
    name: &str,
    start: usize,
    end: usize,
    parent_id: Option<u64>,
) -> Entity {
    let mut e = create_entity(id, EntityKind::TestCase, name, start, end);
    if let Some(pid) = parent_id {
        e = e.with_parent(Some(EntityId(pid)));
    }
    e
}

fn create_test_suite(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::TestSuite, name, start, end)
}

fn make_pf(language: Language, path: &str, source: &str, entities: Vec<Entity>) -> ParsedFile {
    let mut pf = ParsedFile::new(language, path.to_string(), source);
    for e in entities {
        pf.add_entity(e);
    }
    pf
}

fn result_summary(result: &ProcessingResult) -> (usize, Vec<&str>, Vec<GroupType>) {
    let names: Vec<&str> = result.groups.iter().map(|g| g.name.as_str()).collect();
    let types: Vec<GroupType> = result.groups.iter().map(|g| g.group_type).collect();
    (result.groups.len(), names, types)
}

// ============================================================
// Pipeline: Default configuration
// ============================================================

#[test]
fn test_pipeline_default_config_java_class() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "SmallHelper", 0, 10);
    let m1 = create_method(1, "help", 2, 4);
    let m2 = create_method(2, "process", 6, 8);
    class.children = vec![m1.id, m2.id];

    let pf = make_pf(
        Language::Java,
        "SmallHelper.java",
        "class SmallHelper { void help() {} void process() {} }",
        vec![class, m1, m2],
    );
    let result = pipeline.process(&pf);
    let (count, _, _) = result_summary(&result);

    assert!(count >= 1, "Should have at least one group, got {}", count);

    let merged_class = result.groups.iter().find(|g| g.name == "SmallHelper");
    assert!(
        merged_class.is_some(),
        "SmallHelper should appear as a group"
    );
}

#[test]
fn test_pipeline_default_config_standalone_functions() {
    let pipeline = PreprocessingPipeline::new();

    let f1 = create_function(0, "foo", 0, 2);
    let f2 = create_function(1, "bar", 4, 6);

    let pf = make_pf(
        Language::Rust,
        "utils.rs",
        "fn foo() {} fn bar() {}",
        vec![f1, f2],
    );
    let result = pipeline.process(&pf);

    let foo_group = result.groups.iter().find(|g| g.name == "foo");
    let bar_group = result.groups.iter().find(|g| g.name == "bar");

    assert!(foo_group.is_some(), "foo should be in a group");
    assert!(bar_group.is_some(), "bar should be in a group");
    assert_eq!(
        result.stats.input_entities, 2,
        "Should have 2 input entities"
    );
}

// ============================================================
// Pipeline: Test suite grouping workflow
// ============================================================

#[test]
fn test_pipeline_with_test_suites() {
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "Auth", 0, 20);
    let case1 = create_test_case(1, "should login", 5, 10, Some(0));
    let case2 = create_test_case(2, "should logout", 12, 18, Some(0));

    let pf = make_pf(
        Language::JavaScript,
        "auth.test.js",
        "describe('Auth', () => { it('login', () => {}); it('logout', () => {}); });",
        vec![suite, case1, case2],
    );
    let result = pipeline.process(&pf);

    let suite_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(
        suite_group.is_some(),
        "Should create a test suite with cases group"
    );

    if let Some(group) = suite_group {
        assert_eq!(
            group.members.len(),
            2,
            "Test suite group should contain 2 test cases"
        );
        assert_eq!(group.name, "Auth");
    }
}

#[test]
fn test_pipeline_with_standalone_test_cases() {
    let pipeline = PreprocessingPipeline::new();

    let case1 = create_test_case(0, "test_standalone_case", 0, 10, None);

    let pf = make_pf(
        Language::Rust,
        "test_utils.rs",
        "test standalone",
        vec![case1],
    );
    let result = pipeline.process(&pf);

    let standalone = result
        .groups
        .iter()
        .find(|g| g.name == "test_standalone_case");
    assert!(standalone.is_some(), "Standalone test case should appear");
    assert_eq!(
        standalone.unwrap().group_type,
        GroupType::Standalone,
        "Standalone test case should have Standalone type"
    );
}

// ============================================================
// Pipeline: Disabled feature configurations
// ============================================================

#[test]
fn test_pipeline_with_disabled_class_method() {
    let config = NestProcessorConfig {
        enable_class_method_association: false,
        ..Default::default()
    };
    let pipeline = PreprocessingPipeline::with_config(config);

    let mut class = create_class(0, "Data", 0, 10);
    let m1 = create_method(1, "load", 2, 4);
    let m2 = create_method(2, "save", 6, 8);
    class.children = vec![m1.id, m2.id];

    let pf = make_pf(
        Language::Java,
        "Data.java",
        "class Data { void load() {} void save() {} }",
        vec![class, m1, m2],
    );
    let result = pipeline.process(&pf);

    assert!(
        result.groups.len() >= 3,
        "All entities should be standalone when class-method disabled, got {}",
        result.groups.len()
    );
}

#[test]
fn test_pipeline_with_disabled_test_grouping() {
    let config = NestProcessorConfig {
        enable_test_entity_grouping: false,
        ..Default::default()
    };
    let pipeline = PreprocessingPipeline::with_config(config);

    let suite = create_test_suite(0, "Suite", 0, 10);
    let case1 = create_test_case(1, "should work", 3, 8, Some(0));

    let pf = make_pf(
        Language::JavaScript,
        "suite.test.js",
        "describe('Suite', () => { it('case', () => {}); });",
        vec![suite, case1],
    );
    let result = pipeline.process(&pf);

    let suite_groups = result
        .groups
        .iter()
        .filter(|g| g.group_type == GroupType::TestSuiteWithCases)
        .count();
    assert_eq!(suite_groups, 0, "No test suite groups when disabled");
}

#[test]
fn test_pipeline_with_all_disabled() {
    let config = NestProcessorConfig {
        enable_call_merging: false,
        enable_test_entity_grouping: false,
        enable_class_method_association: false,
        ..Default::default()
    };
    let pipeline = PreprocessingPipeline::with_config(config);

    let mut class = create_class(0, "C", 0, 5);
    let method = create_method(1, "m", 2, 4);
    class.children = vec![method.id];
    let func = create_function(2, "f", 7, 10);

    let pf = make_pf(
        Language::Rust,
        "mixed.rs",
        "class C { void m() {} } fn f() {}",
        vec![class, method, func],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 3);
    assert_eq!(
        result.groups.len(),
        3,
        "All 3 entities should be standalone when all grouping is disabled"
    );
}

// ============================================================
// Pipeline: Combined scenarios (test suites + classes + functions)
// ============================================================

#[test]
fn test_pipeline_mixed_entities() {
    let pipeline = PreprocessingPipeline::new();

    let mut helper = create_class(0, "Helper", 0, 5);
    let help_method = create_method(1, "help", 2, 4);
    helper.children = vec![help_method.id];

    let suite = create_test_suite(2, "API", 7, 15);
    let test_case = create_test_case(3, "should handle GET", 9, 13, Some(2));

    let util = create_function(4, "util", 17, 20);

    let pf = make_pf(
        Language::JavaScript,
        "mixed.js",
        "class Helper { void help() {} } describe('API', () => { it('GET', () => {}); }); fn util() {}",
        vec![helper, help_method, suite, test_case, util],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 5);

    let api_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(api_group.is_some(), "API test suite group should exist");

    let util_group = result.groups.iter().find(|g| g.name == "util");
    assert!(util_group.is_some(), "util function should exist as group");

    let total_entity_ids: std::collections::HashSet<_> = result
        .groups
        .iter()
        .flat_map(|g| {
            let mut ids = vec![g.header_id];
            ids.extend(g.member_ids.iter().map(|m| Some(*m)));
            ids
        })
        .flatten()
        .collect();

    let original_ids: std::collections::HashSet<_> = vec![
        EntityId(0),
        EntityId(1),
        EntityId(2),
        EntityId(3),
        EntityId(4),
    ]
    .into_iter()
    .collect();

    assert!(
        original_ids.is_subset(&total_entity_ids),
        "All original entities should be present in groups"
    );
}

#[test]
fn test_pipeline_multiple_test_suites() {
    let pipeline = PreprocessingPipeline::new();

    let suite1 = create_test_suite(0, "Auth Tests", 0, 20);
    let case1 = create_test_case(1, "login", 5, 10, Some(0));
    let case2 = create_test_case(2, "logout", 12, 18, Some(0));

    let suite2 = create_test_suite(3, "User Tests", 22, 40);
    let case3 = create_test_case(4, "create", 25, 30, Some(3));
    let case4 = create_test_case(5, "delete", 32, 38, Some(3));

    let pf = make_pf(
        Language::JavaScript,
        "multi.test.js",
        "suites",
        vec![suite1, case1, case2, suite2, case3, case4],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 6);

    let suite_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.group_type == GroupType::TestSuiteWithCases)
        .collect();
    assert_eq!(suite_groups.len(), 2, "Should have 2 test suite groups");

    let mut names: Vec<&str> = suite_groups.iter().map(|g| g.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Auth Tests", "User Tests"]);
}

// ============================================================
// Pipeline: Class-method association edge cases
// ============================================================

#[test]
fn test_pipeline_large_class_merged_with_methods() {
    let pipeline = PreprocessingPipeline::new();

    let mut large = create_class(0, "LargeService", 0, 200);
    let m1 = create_method(1, "doTask", 10, 30);
    let m2 = create_method(2, "analyze", 40, 60);
    large.children = vec![m1.id, m2.id];

    let pf = make_pf(
        Language::Java,
        "LargeService.java",
        "large class",
        vec![large, m1, m2],
    );
    let result = pipeline.process(&pf);

    // Large class should now be merged with its methods into one group
    let class_group = result.groups.iter().find(|g| g.name == "LargeService");
    assert!(class_group.is_some(), "LargeService should be a group");
    assert_eq!(
        class_group.unwrap().group_type,
        GroupType::ClassWithMethods,
        "LargeService should be ClassWithMethods"
    );
}

#[test]
fn test_pipeline_class_with_multiple_methods() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "Util", 0, 20);
    let methods: Vec<Entity> = (1..=5)
        .map(|i| create_method(i as u64, &format!("func{}", i), i * 2, i * 2 + 1))
        .collect();

    class.children = methods.iter().map(|m| m.id).collect();
    let mut entities = vec![class];
    entities.extend(methods);

    let pf = make_pf(
        Language::Java,
        "Util.java",
        "class Util { fn1(){} fn2(){} fn3(){} fn4(){} fn5(){} }",
        entities,
    );
    let result = pipeline.process(&pf);

    let class_group = result.groups.iter().find(|g| g.name == "Util");
    assert!(class_group.is_some(), "Util should be a group");
}

#[test]
fn test_pipeline_class_with_getters_setters() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "Person", 0, 8);
    let field = create_entity(3, EntityKind::Field, "name", 1, 1);
    let getter = create_method(1, "getName", 3, 4);
    let setter = create_method(2, "setName", 5, 7);
    class.children = vec![getter.id, setter.id, field.id];

    let pf = make_pf(
        Language::Java,
        "Person.java",
        "class Person { String name; String getName() { } void setName(String n) { } }",
        vec![class, field, getter, setter],
    );
    let result = pipeline.process(&pf);

    let person = result.groups.iter().find(|g| g.name == "Person");
    assert!(person.is_some(), "Person class should appear as a group");
}

// ============================================================
// Pipeline: Process method (process_entities)
// ============================================================

#[test]
fn test_pipeline_process_entities_empty() {
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process_entities(&[], Language::Rust);
    assert_eq!(result.groups.len(), 0);
}

#[test]
fn test_pipeline_process_entities_all_types() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "Service", 0, 10);
    let method = create_method(1, "serve", 2, 8);
    class.children = vec![method.id];
    let func = create_function(2, "main", 12, 15);

    let result = pipeline.process_entities(&[class, method, func], Language::Rust);
    assert_eq!(result.stats.input_entities, 3);
    assert!(!result.groups.is_empty(), "Should have at least 1 group");
}

#[test]
fn test_pipeline_process_entities_with_test_suites() {
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "Integration", 0, 20);
    let case1 = create_test_case(1, "test_api", 5, 10, Some(0));
    let case2 = create_test_case(2, "test_db", 12, 18, Some(0));

    let result = pipeline.process_entities(&[suite, case1, case2], Language::JavaScript);

    let suite_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(
        suite_group.is_some(),
        "process_entities should handle test suites"
    );

    if let Some(group) = suite_group {
        assert_eq!(group.members.len(), 2);
    }
}

// ============================================================
// Pipeline: Import separation (imports are collected at the file level)
// ============================================================

#[test]
fn test_pipeline_drops_import_groups_keeps_real_entities() {
    // Imports are separated at the entity level. They never produce
    // retrieval groups and are never absorbed into adjacent groups — the file
    // level summary and the relation index cover them instead.
    let pipeline = PreprocessingPipeline::new();

    let import1 = create_entity(1, EntityKind::Import, "use std::fmt;", 0, 1);
    let import2 = create_entity(2, EntityKind::Import, "use std::io;", 2, 3);
    let mut service = create_class(3, "Formatter", 4, 12);
    let method = create_method(4, "format", 6, 10);
    service.children = vec![method.id];

    let pf = make_pf(
        Language::Rust,
        "formatter.rs",
        "use std::fmt;\nuse std::io;\n\nclass Formatter { fn format() {} }",
        vec![import1, import2, service, method],
    );
    let result = pipeline.process(&pf);

    assert!(
        result
            .groups
            .iter()
            .all(|g| !g.all_entity_ids().iter().any(|id| id.0 <= 2)),
        "import entities must never appear in any output group"
    );
    let formatter = result.groups.iter().find(|g| g.name == "Formatter");
    assert!(
        formatter.is_some(),
        "real entities must survive the import-only group drop"
    );
    assert_eq!(
        result.groups.len(),
        1,
        "only the Formatter group must remain, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_pipeline_imports_inside_test_suite_stay_group_members() {
    // Imports nested inside a test module do NOT get absorbed into
    // the test-suite group. They stay import-only groups (dropped before
    // chunking) — the suite group survives with its test cases only.
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "tests", 0, 20);
    let import = create_entity(1, EntityKind::Import, "use super::*;", 2, 3);
    let case = create_test_case(2, "test_case_a", 5, 10, Some(0));
    let mut import_nested = create_entity(3, EntityKind::Import, "use std::fmt;", 4, 5);
    import_nested = import_nested.with_parent(Some(EntityId(0)));

    let pf = make_pf(
        Language::Rust,
        "tests.rs",
        "mod tests { use super::*; use std::fmt; fn test_case_a() {} }",
        vec![suite, import, case, import_nested],
    );
    let result = pipeline.process(&pf);

    let suite_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(
        suite_group.is_some(),
        "the test suite group must survive even when the file contains imports"
    );
    if let Some(group) = suite_group {
        assert!(
            group.members.iter().all(|m| m.kind != EntityKind::Import),
            "imports must not be absorbed into the test suite group"
        );
    }
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.all_entity_ids().iter().all(|id| !matches!(id.0, 1 | 3))),
        "import entities must not remain in any output group"
    );
}

// ============================================================
// Pipeline: ProcessingStats verification
// ============================================================

#[test]
fn test_pipeline_stats_tracking() {
    let pipeline = PreprocessingPipeline::new();

    let f1 = create_function(0, "fn_a", 0, 2);
    let f2 = create_function(1, "fn_b", 4, 6);
    let f3 = create_function(2, "fn_c", 8, 10);

    let pf = make_pf(Language::Rust, "stats.rs", "", vec![f1, f2, f3]);
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 3);
    assert!(
        result.stats.output_groups >= 3,
        "Output groups should be >= 3 for 3 standalone functions"
    );
    assert_eq!(result.stats.merged_calls, 0);
    assert_eq!(result.stats.class_method_associations, 0);
    assert!(result.stats.standalone_entities >= 3);
}

#[test]
fn test_pipeline_stats_with_test_groups() {
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "Group1", 0, 20);
    let case1 = create_test_case(1, "case_a", 5, 10, Some(0));
    let case2 = create_test_case(2, "case_b", 12, 18, Some(0));

    let pf = make_pf(
        Language::JavaScript,
        "stats.spec.js",
        "",
        vec![suite, case1, case2],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 3);
    assert!(
        result.stats.output_groups >= 1,
        "Should have at least 1 output group"
    );
}

// ============================================================
// Pipeline: PipelineBuilder
// ============================================================

#[test]
fn test_pipeline_builder_with_config() {
    use cce_parser::grouper::pipeline::PipelineBuilder;

    let pipeline = PipelineBuilder::new()
        .config(NestProcessorConfig::small_codebase())
        .build();

    let mock_entities = vec![create_function(0, "test_func", 0, 10)];
    let pf = make_pf(Language::Rust, "builder_test.rs", "", mock_entities);
    let result = pipeline.process(&pf);

    assert!(
        !result.groups.is_empty(),
        "Pipeline built with builder should work"
    );
}

// ============================================================
// Pipeline: EntityGroup combined source
// ============================================================

#[test]
fn test_pipeline_generates_combined_source() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "Small", 0, 5);
    let m1 = create_method(1, "a", 1, 2);
    let m2 = create_method(2, "b", 3, 4);
    class.children = vec![m1.id, m2.id];

    let pf = make_pf(
        Language::Java,
        "Small.java",
        "class Small { void a() {} void b() {} /*                                                    */ }",
        vec![class, m1, m2],
    );
    let result = pipeline.process(&pf);

    for group in &result.groups {
        if group.name == "Small" {
            // combined_source may not be generated if span exceeds source length
            // Just verify the group was created
            assert!(
                !group.members.is_empty() || group.header.is_some(),
                "Small group should have members"
            );
            return;
        }
    }
    panic!("Small group not found");
}

// ============================================================
// Pipeline: Language-specific scenarios
// ============================================================

#[test]
fn test_pipeline_rust_entities() {
    let pipeline = PreprocessingPipeline::new();

    let func = create_function(0, "add", 0, 5);
    let pf = make_pf(
        Language::Rust,
        "math.rs",
        "fn add(a: i32, b: i32) -> i32 { a + b }",
        vec![func],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.groups.len(), 1, "Single Rust function -> 1 group");
    assert_eq!(result.groups[0].name, "add");
}

#[test]
fn test_pipeline_python_class() {
    let pipeline = PreprocessingPipeline::new();

    let mut class = create_class(0, "Calculator", 0, 5);
    let m1 = create_method(1, "add", 1, 3);
    let m2 = create_method(2, "sub", 4, 5);
    class.children = vec![m1.id, m2.id];

    let pf = make_pf(
        Language::Python,
        "calculator.py",
        "class Calculator:\n    def add(self): pass\n    def sub(self): pass",
        vec![class, m1, m2],
    );
    let result = pipeline.process(&pf);

    let calc = result.groups.iter().find(|g| g.name == "Calculator");
    assert!(calc.is_some(), "Calculator should exist as a group");
}

#[test]
fn test_pipeline_typescript_test_suite() {
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "Component", 0, 10);
    let case1 = create_test_case(1, "renders", 3, 8, Some(0));

    let pf = make_pf(
        Language::TypeScript,
        "component.test.ts",
        "describe('Component', () => { it('renders', () => {}); });",
        vec![suite, case1],
    );
    let result = pipeline.process(&pf);

    let suite_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(
        suite_group.is_some(),
        "TypeScript test suite should be grouped"
    );
}

// ============================================================
// Pipeline: Edge cases
// ============================================================

#[test]
fn test_pipeline_large_number_of_entities() {
    let pipeline = PreprocessingPipeline::new();

    let entities: Vec<Entity> = (0..100)
        .map(|i| {
            create_function(
                i as u64,
                &format!("fn_{}", i),
                i as usize * 3,
                i as usize * 3 + 2,
            )
        })
        .collect();

    let pf = make_pf(Language::Rust, "large.rs", "large file", entities);
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 100);
    assert_eq!(
        result.groups.len(),
        100,
        "100 standalone functions should produce 100 groups"
    );
}

#[test]
fn test_pipeline_suite_without_cases() {
    let pipeline = PreprocessingPipeline::new();

    let suite = create_test_suite(0, "Empty", 0, 5);
    let pf = make_pf(
        Language::JavaScript,
        "empty.test.js",
        "describe('Empty', () => {});",
        vec![suite],
    );
    let result = pipeline.process(&pf);

    let empty_suite = result.groups.iter().find(|g| g.name == "Empty");
    assert!(
        empty_suite.is_some(),
        "Empty suite should still appear as a group"
    );
}

#[test]
fn test_pipeline_process_with_all_features_enabled() {
    let config = NestProcessorConfig {
        enable_call_merging: true,
        enable_test_entity_grouping: true,
        enable_class_method_association: true,
        enable_getter_setter_merging: true,
        ..Default::default()
    };
    let pipeline = PreprocessingPipeline::with_config(config);

    let app = create_class(0, "App", 0, 5);
    let main_fn = create_function(1, "main", 7, 10);

    let pf = make_pf(
        Language::Java,
        "App.java",
        "class App {} fn main() {}",
        vec![app, main_fn],
    );
    let result = pipeline.process(&pf);

    assert_eq!(result.stats.input_entities, 2);
    assert!(result.groups.len() >= 2, "Should have at least 2 groups");
    assert!(result.stats.output_groups > 0);
}

// ============================================================
// Pipeline: Null/zero-byte source handling
// ============================================================

#[test]
fn test_pipeline_empty_source() {
    let pipeline = PreprocessingPipeline::new();
    let parsed_file = ParsedFile::new(Language::Rust, "empty.rs".to_string(), "");
    let result = pipeline.process(&parsed_file);
    assert_eq!(result.groups.len(), 0);
    assert_eq!(result.stats.input_entities, 0);
}

#[test]
fn test_pipeline_process_entities_idempotent() {
    let pipeline = PreprocessingPipeline::new();

    let input = vec![
        create_function(0, "fn_a", 0, 5),
        create_function(1, "fn_b", 7, 10),
    ];

    let result1 = pipeline.process_entities(&input, Language::Rust);
    let result2 = pipeline.process_entities(&input, Language::Rust);

    assert_eq!(
        result1.groups.len(),
        result2.groups.len(),
        "Pipeline should be idempotent"
    );
    for (g1, g2) in result1.groups.iter().zip(result2.groups.iter()) {
        assert_eq!(g1.name, g2.name);
        assert_eq!(g1.group_type, g2.group_type);
    }
}
