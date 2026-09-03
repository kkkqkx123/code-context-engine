use super::processor::ClassMethodProcessor;
use crate::grouper::context::FileProcessingContext;
use crate::grouper::types::pattern::get_member_role;
use crate::grouper::types::{GroupType, MemberRole, PatternInfo};
use cce_config::NestProcessorConfig;
use cce_config::modules::pattern_detection::GetterSetterDetectionConfig;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
use cce_types::language::Language;

#[test]
fn test_getter_setter_merging() {
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "TestClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 20, column: 0 },
            start_byte: 0,
            end_byte: 500,
        },
    );

    let getter_id = EntityId(1);
    let getter = Entity::new(
        getter_id,
        EntityKind::Method,
        "getName".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 7, column: 4 },
            start_byte: 100,
            end_byte: 200,
        },
    );

    let setter_id = EntityId(2);
    let setter = Entity::new(
        setter_id,
        EntityKind::Method,
        "setName".to_string(),
        Span {
            start_position: cce_types::Position { row: 8, column: 4 },
            end_position: cce_types::Position { row: 10, column: 4 },
            start_byte: 201,
            end_byte: 300,
        },
    );

    let complex_method_id = EntityId(3);
    let complex_method = Entity::new(
        complex_method_id,
        EntityKind::Method,
        "doSomething".to_string(),
        Span {
            start_position: cce_types::Position { row: 12, column: 4 },
            end_position: cce_types::Position { row: 18, column: 4 },
            start_byte: 301,
            end_byte: 500,
        },
    );

    class.children = vec![getter_id, setter_id, complex_method_id];
    let entities = vec![class, getter, setter, complex_method];

    let config = NestProcessorConfig {
        enable_getter_setter_merging: true,
        small_class_threshold: 100,
        ..Default::default()
    };

    let parsed_file = ParsedFile::new(Language::Java, "TestClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Should have one group (merged)");
    assert_eq!(groups[0].name, "TestClass");

    // Getter/Setter pattern should be detected
    assert!(
        matches!(groups[0].pattern_info, PatternInfo::GetterSetter(_)),
        "Should have getter/setter pattern info, got {:?}",
        groups[0].pattern_info
    );
}

#[test]
fn test_large_class_no_merge() {
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "LargeClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position {
                row: 200,
                column: 0,
            },
            start_byte: 0,
            end_byte: 5000,
        },
    );

    let method_id = EntityId(1);
    let method = Entity::new(
        method_id,
        EntityKind::Method,
        "doSomething".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 20, column: 4 },
            start_byte: 200,
            end_byte: 400,
        },
    );

    class.children = vec![method_id];
    let entities = vec![class, method];

    let config = NestProcessorConfig {
        small_class_threshold: 300,
        ..Default::default()
    };

    let parsed_file = ParsedFile::new(Language::Java, "LargeClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(
        groups.len(),
        1,
        "Should have 1 merged group (class + method)"
    );
    assert!(groups.iter().any(|g| g.name == "LargeClass"));
    assert_eq!(
        groups[0].members.len(),
        1,
        "Merged group should have 1 method member"
    );
}

#[test]
fn test_large_class_with_methods_no_merge() {
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "LargeService".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position {
                row: 500,
                column: 0,
            },
            start_byte: 0,
            end_byte: 15000,
        },
    );

    let method1_id = EntityId(1);
    let method1 = Entity::new(
        method1_id,
        EntityKind::Method,
        "getName".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 15, column: 4 },
            start_byte: 300,
            end_byte: 500,
        },
    );

    let method2_id = EntityId(2);
    let method2 = Entity::new(
        method2_id,
        EntityKind::Method,
        "setName".to_string(),
        Span {
            start_position: cce_types::Position { row: 16, column: 4 },
            end_position: cce_types::Position { row: 20, column: 4 },
            start_byte: 501,
            end_byte: 700,
        },
    );

    let method3_id = EntityId(3);
    let method3 = Entity::new(
        method3_id,
        EntityKind::Method,
        "processData".to_string(),
        Span {
            start_position: cce_types::Position { row: 25, column: 4 },
            end_position: cce_types::Position {
                row: 100,
                column: 4,
            },
            start_byte: 701,
            end_byte: 3000,
        },
    );

    class.children = vec![method1_id, method2_id, method3_id];
    let entities = vec![class, method1, method2, method3];

    let config = NestProcessorConfig {
        enable_getter_setter_merging: true,
        small_class_threshold: 600,
        ..Default::default()
    };

    let parsed_file = ParsedFile::new(Language::Java, "LargeService.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Should have 1 merged group (class + 3 methods) - chunker handles splitting
    assert_eq!(
        groups.len(),
        1,
        "Should have 1 merged group (class + 3 methods)"
    );
    assert!(groups.iter().any(|g| g.name == "LargeService"));
    assert_eq!(
        groups[0].members.len(),
        3,
        "Merged group should have 3 method members"
    );
}

#[test]
fn test_getter_setter_with_field_name() {
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "Person".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 20, column: 0 },
            start_byte: 0,
            end_byte: 500,
        },
    );

    // Create a field
    let field_id = EntityId(1);
    let field = Entity::new(
        field_id,
        EntityKind::Field,
        "name".to_string(),
        Span {
            start_position: cce_types::Position { row: 3, column: 4 },
            end_position: cce_types::Position { row: 3, column: 20 },
            start_byte: 50,
            end_byte: 80,
        },
    );

    let getter_id = EntityId(2);
    let mut getter = Entity::new(
        getter_id,
        EntityKind::Method,
        "getName".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 7, column: 4 },
            start_byte: 100,
            end_byte: 200,
        },
    );
    getter
        .metadata
        .insert("getter_for".to_string(), "name".to_string());

    let setter_id = EntityId(3);
    let mut setter = Entity::new(
        setter_id,
        EntityKind::Method,
        "setName".to_string(),
        Span {
            start_position: cce_types::Position { row: 8, column: 4 },
            end_position: cce_types::Position { row: 10, column: 4 },
            start_byte: 201,
            end_byte: 300,
        },
    );
    setter
        .metadata
        .insert("setter_for".to_string(), "name".to_string());

    class.children = vec![field_id, getter_id, setter_id];
    let entities = vec![class, field, getter, setter];

    let config = NestProcessorConfig {
        enable_getter_setter_merging: true,
        small_class_threshold: 100,
        ..Default::default()
    };

    let parsed_file = ParsedFile::new(Language::Java, "Person.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Should have one group (class merged with getters/setters)
    assert_eq!(groups.len(), 1, "Should have one merged group");
    assert_eq!(groups[0].name, "Person");

    // Getter/Setter pattern should be detected
    assert!(
        matches!(groups[0].pattern_info, PatternInfo::GetterSetter(_)),
        "Pattern should be GetterSetter, got {:?}",
        groups[0].pattern_info
    );
}

#[test]
fn test_nested_entity_group_extraction() {
    let config = NestProcessorConfig {
        enable_nested_entity_grouping: true,
        max_nesting_depth: 2,
        min_nested_size: 5,
        ..Default::default()
    };

    // Create outer class
    let outer_class_id = EntityId(0);
    let mut outer_class = Entity::new(
        outer_class_id,
        EntityKind::Class,
        "OuterClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 50, column: 0 },
            start_byte: 0,
            end_byte: 1000,
        },
    );

    // Create inner class (nested)
    let inner_class_id = EntityId(1);
    let inner_class = Entity::new(
        inner_class_id,
        EntityKind::Class,
        "InnerBuilder".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 40, column: 4 },
            start_byte: 200,
            end_byte: 800,
        },
    );

    outer_class.children = vec![inner_class_id];

    let entities = vec![outer_class, inner_class];
    let parsed_file = ParsedFile::new(Language::Java, "OuterClass.java".to_string(), "");

    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Should have one group for the outer class
    assert_eq!(groups.len(), 1, "Should have one group for outer class");

    let outer_group = &groups[0];
    assert_eq!(outer_group.name, "OuterClass");
    assert_eq!(
        outer_group.group_type,
        GroupType::ClassWithNestedClasses,
        "Should be ClassWithNestedClasses type"
    );
    assert!(
        outer_group.has_significant_nested,
        "Should have significant nested"
    );
    assert_eq!(
        outer_group.nested_groups.len(),
        1,
        "Should have one nested group"
    );

    let nested = &outer_group.nested_groups[0];
    assert_eq!(nested.name, "InnerBuilder");
    assert_eq!(nested.nesting_level, 2);
    assert_eq!(
        nested.parent_group_id,
        Some(compact_str::CompactString::from("OuterClass")),
        "Should have parent group ID"
    );
}

#[test]
fn test_nested_entity_group_depth_limit() {
    let config = NestProcessorConfig {
        enable_nested_entity_grouping: true,
        max_nesting_depth: 1, // Only 1 level
        min_nested_size: 5,
        ..Default::default()
    };

    // Create outer class
    let outer_class_id = EntityId(0);
    let mut outer_class = Entity::new(
        outer_class_id,
        EntityKind::Class,
        "OuterClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 50, column: 0 },
            start_byte: 0,
            end_byte: 1000,
        },
    );

    // Create inner class
    let inner_class_id = EntityId(1);
    let mut inner_class = Entity::new(
        inner_class_id,
        EntityKind::Class,
        "InnerClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 40, column: 4 },
            start_byte: 200,
            end_byte: 800,
        },
    );

    // Create deeply nested class (should not be extracted due to depth limit)
    let deep_nested_id = EntityId(2);
    let deep_nested = Entity::new(
        deep_nested_id,
        EntityKind::Class,
        "DeepNested".to_string(),
        Span {
            start_position: cce_types::Position { row: 15, column: 8 },
            end_position: cce_types::Position { row: 35, column: 8 },
            start_byte: 300,
            end_byte: 700,
        },
    );

    inner_class.children = vec![deep_nested_id];
    outer_class.children = vec![inner_class_id];

    let entities = vec![outer_class, inner_class, deep_nested];
    let parsed_file = ParsedFile::new(Language::Java, "OuterClass.java".to_string(), "");

    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1);
    let outer_group = &groups[0];

    // Should have one nested group (InnerClass)
    assert_eq!(outer_group.nested_groups.len(), 1);

    // InnerClass should NOT have nested groups due to depth limit
    let inner = &outer_group.nested_groups[0];
    assert_eq!(
        inner.nested_groups.len(),
        0,
        "Should not extract deep nested due to depth limit"
    );
}

#[test]
fn test_nested_entity_group_size_filter() {
    let config = NestProcessorConfig {
        enable_nested_entity_grouping: true,
        max_nesting_depth: 2,
        min_nested_size: 10, // Require at least 10 lines
        ..Default::default()
    };

    // Create outer class
    let outer_class_id = EntityId(0);
    let mut outer_class = Entity::new(
        outer_class_id,
        EntityKind::Class,
        "OuterClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 50, column: 0 },
            start_byte: 0,
            end_byte: 1000,
        },
    );

    // Create small inner class (only 3 lines, should be filtered)
    let small_inner_id = EntityId(1);
    let small_inner = Entity::new(
        small_inner_id,
        EntityKind::Class,
        "SmallInner".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 12, column: 4 },
            start_byte: 200,
            end_byte: 300,
        },
    );

    outer_class.children = vec![small_inner_id];

    let entities = vec![outer_class, small_inner];
    let parsed_file = ParsedFile::new(Language::Java, "OuterClass.java".to_string(), "");

    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);
    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1);
    let outer_group = &groups[0];

    // Should NOT have nested groups due to size filter
    assert_eq!(
        outer_group.nested_groups.len(),
        0,
        "Should not extract small nested class"
    );
    assert!(
        !outer_group.has_significant_nested,
        "Should not have significant nested"
    );
}

// =============================================
// New tests: extract_builder_fields edge cases
// =============================================

// =============================================
// New tests: process() edge cases
// =============================================

#[test]
fn test_process_empty_entities() {
    let config = NestProcessorConfig {
        small_class_threshold: 50,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "empty.java".to_string(), "");
    let entities: Vec<Entity> = vec![];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert!(groups.is_empty(), "Empty entities should produce no groups");
}

#[test]
fn test_process_free_functions_only() {
    // Entities without a type-definition parent (free functions, no class/struct)
    // are added as standalone groups via the "remaining entities" fallback loop.
    let function_entity = Entity::new(
        EntityId(0),
        EntityKind::Function,
        "main".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 10, column: 0 },
            start_byte: 0,
            end_byte: 200,
        },
    );

    let config = NestProcessorConfig {
        small_class_threshold: 100,
        enable_getter_setter_merging: true,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Rust, "main.rs".to_string(), "");
    let entities = vec![function_entity];
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Free functions are not processed by the type-definition loop,
    // but they ARE added as standalone groups by the fallback loop
    assert_eq!(
        groups.len(),
        1,
        "Free function should become a standalone group"
    );
    assert_eq!(groups[0].name, "main");
    assert_eq!(
        groups[0].group_type,
        crate::grouper::types::GroupType::Standalone,
        "Should be standalone"
    );
}

#[test]
fn test_constructor_method_included() {
    // Constructor method (EntityKind::Constructor) should be included and marked significant
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "MyService".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 20, column: 0 },
            start_byte: 0,
            end_byte: 400,
        },
    );

    let ctor_id = EntityId(1);
    let ctor = Entity::new(
        ctor_id,
        EntityKind::Constructor,
        "MyService".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 10, column: 4 },
            start_byte: 100,
            end_byte: 250,
        },
    );

    let regular_method_id = EntityId(2);
    let regular_method = Entity::new(
        regular_method_id,
        EntityKind::Method,
        "doSomething".to_string(),
        Span {
            start_position: cce_types::Position { row: 12, column: 4 },
            end_position: cce_types::Position { row: 18, column: 4 },
            start_byte: 251,
            end_byte: 400,
        },
    );

    class.children = vec![ctor_id, regular_method_id];
    let entities = vec![class, ctor, regular_method];

    let config = NestProcessorConfig {
        small_class_threshold: 100,
        enable_getter_setter_merging: true,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "MyService.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Should have one merged group
    assert_eq!(groups.len(), 1, "Should have one group");
    assert_eq!(groups[0].name, "MyService");

    // Constructor should be included
    assert!(
        groups[0].members.iter().any(|m| m.name == "MyService"),
        "Constructor should be a member"
    );

    // Constructor should be marked as significant
    let ctor_role = get_member_role(&groups[0].member_roles, &ctor_id);
    assert_eq!(
        ctor_role,
        Some(&MemberRole::SignificantMethod),
        "Constructor should be marked as significant"
    );
}

#[test]
fn test_process_struct_entity() {
    // Test that Struct entities are also processed (not just Class)
    let struct_id = EntityId(0);
    let mut my_struct = Entity::new(
        struct_id,
        EntityKind::Struct,
        "Config".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 15, column: 0 },
            start_byte: 0,
            end_byte: 300,
        },
    );

    let getter_id = EntityId(1);
    let getter = Entity::new(
        getter_id,
        EntityKind::Method,
        "get_url".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 7, column: 4 },
            start_byte: 60,
            end_byte: 120,
        },
    );

    let setter_id = EntityId(2);
    let setter = Entity::new(
        setter_id,
        EntityKind::Method,
        "set_url".to_string(),
        Span {
            start_position: cce_types::Position { row: 8, column: 4 },
            end_position: cce_types::Position { row: 10, column: 4 },
            start_byte: 121,
            end_byte: 180,
        },
    );

    my_struct.children = vec![getter_id, setter_id];
    let entities = vec![my_struct, getter, setter];

    let config = NestProcessorConfig {
        small_class_threshold: 100,
        enable_getter_setter_merging: true,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Rust, "config.rs".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Struct should be processed into a group");
    assert_eq!(groups[0].name, "Config");
}

#[test]
fn test_is_stdlib_marked_boilerplate() {
    // Test that stdlib methods are marked as boilerplate
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "Wrapper".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 20, column: 0 },
            start_byte: 0,
            end_byte: 400,
        },
    );

    let stdlib_method_id = EntityId(1);
    let mut stdlib_method = Entity::new(
        stdlib_method_id,
        EntityKind::Method,
        "toString".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 7, column: 4 },
            start_byte: 100,
            end_byte: 180,
        },
    );
    stdlib_method.is_stdlib = true;

    let regular_method_id = EntityId(2);
    let regular_method = Entity::new(
        regular_method_id,
        EntityKind::Method,
        "process".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 18, column: 4 },
            start_byte: 181,
            end_byte: 400,
        },
    );

    class.children = vec![stdlib_method_id, regular_method_id];
    let entities = vec![class, stdlib_method, regular_method];

    let config = NestProcessorConfig {
        small_class_threshold: 100,
        enable_getter_setter_merging: false,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "Wrapper.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // The class should be merged with its methods
    assert_eq!(groups.len(), 1, "Should have one group");

    // stdlib method should be marked as boilerplate
    let stdlib_role = get_member_role(&groups[0].member_roles, &stdlib_method_id);
    assert_eq!(
        stdlib_role,
        Some(&MemberRole::BoilerplateMethod),
        "Stdlib method should be marked as boilerplate"
    );

    // regular method should be marked as significant (or at least not boilerplate)
    let regular_role = get_member_role(&groups[0].member_roles, &regular_method_id);
    assert_ne!(
        regular_role,
        Some(&MemberRole::BoilerplateMethod),
        "Non-stdlib method should not be boilerplate"
    );
}

#[test]
fn test_getter_setter_merging_disabled() {
    // When getter/setter merging is disabled but class is small,
    // the class still merges with all methods (no getters/setters are filtered).
    // The difference is that ALL methods stay in the group as members
    // (getters/setters are NOT excluded from the method list).
    let class_id = EntityId(0);
    let mut class = Entity::new(
        class_id,
        EntityKind::Class,
        "SmallClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 12, column: 0 },
            start_byte: 0,
            end_byte: 300,
        },
    );

    let getter_id = EntityId(1);
    let getter = Entity::new(
        getter_id,
        EntityKind::Method,
        "getName".to_string(),
        Span {
            start_position: cce_types::Position { row: 5, column: 4 },
            end_position: cce_types::Position { row: 7, column: 4 },
            start_byte: 80,
            end_byte: 150,
        },
    );

    let setter_id = EntityId(2);
    let setter = Entity::new(
        setter_id,
        EntityKind::Method,
        "setName".to_string(),
        Span {
            start_position: cce_types::Position { row: 8, column: 4 },
            end_position: cce_types::Position { row: 10, column: 4 },
            start_byte: 151,
            end_byte: 220,
        },
    );

    class.children = vec![getter_id, setter_id];
    let entities = vec![class, getter, setter];

    let config = NestProcessorConfig {
        enable_getter_setter_merging: false,
        small_class_threshold: 100,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "SmallClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // With getter_setter_merging disabled but class is small,
    // the class still merges (should_merge = true because methods are not empty).
    // All methods become members without pattern info.
    assert_eq!(
        groups.len(),
        1,
        "Small class with methods should still merge into one group"
    );
    assert_eq!(groups[0].name, "SmallClass");
    // When disabled, no GetterSetter pattern info should be set
    assert!(
        !matches!(groups[0].pattern_info, PatternInfo::GetterSetter(_)),
        "No GetterSetter pattern info should be set when getter/setter merging is disabled"
    );
    // All methods (including getters/setters) should be in the group
    assert!(
        groups[0].members.iter().any(|m| m.name == "getName"),
        "Getter should be included when merging is disabled"
    );
    assert!(
        groups[0].members.iter().any(|m| m.name == "setName"),
        "Setter should be included when merging is disabled"
    );
}

// =============================================
// New tests: nested group edge cases
// =============================================

#[test]
fn test_nested_grouping_disabled_config() {
    // When nested grouping is disabled, inner classes are not extracted as nested groups.
    // Instead, the inner class (a type-definition) is iterated separately
    // and becomes its own top-level group.
    let outer_class_id = EntityId(0);
    let mut outer_class = Entity::new(
        outer_class_id,
        EntityKind::Class,
        "OuterClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 50, column: 0 },
            start_byte: 0,
            end_byte: 1000,
        },
    );

    let inner_class_id = EntityId(1);
    let inner_class = Entity::new(
        inner_class_id,
        EntityKind::Class,
        "InnerBuilder".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 40, column: 4 },
            start_byte: 200,
            end_byte: 800,
        },
    );

    outer_class.children = vec![inner_class_id];
    let entities = vec![outer_class, inner_class];

    let config = NestProcessorConfig {
        enable_nested_entity_grouping: false,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "OuterClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    // Outer class gets one group; inner class (also a type-definition) gets its own group
    assert_eq!(
        groups.len(),
        2,
        "Each type-definition becomes a separate group"
    );
    assert_eq!(
        groups[0].name, "OuterClass",
        "Outer class should be first group"
    );
    assert_eq!(
        groups[0].nested_groups.len(),
        0,
        "Should NOT have nested groups when disabled"
    );
    assert!(
        groups.iter().any(|g| g.name == "InnerBuilder"),
        "Inner class should be a separate top-level group"
    );
}

#[test]
fn test_nested_group_without_children() {
    // Parent entity with no children should still produce a group with no nested groups
    let outer_class = Entity::new(
        EntityId(0),
        EntityKind::Class,
        "EmptyClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 10, column: 0 },
            start_byte: 0,
            end_byte: 200,
        },
    );

    let entities = vec![outer_class];

    let config = NestProcessorConfig {
        enable_nested_entity_grouping: true,
        max_nesting_depth: 2,
        min_nested_size: 1,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "EmptyClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1, "Should have one group");
    assert_eq!(groups[0].name, "EmptyClass");
    assert_eq!(
        groups[0].nested_groups.len(),
        0,
        "Should have no nested groups when no children"
    );
}

#[test]
fn test_nested_group_max_depth_zero() {
    // max_nesting_depth=0 means children should be marked as processed but no nested groups created
    let outer_class_id = EntityId(0);
    let mut outer_class = Entity::new(
        outer_class_id,
        EntityKind::Class,
        "OuterClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 0, column: 0 },
            end_position: cce_types::Position { row: 50, column: 0 },
            start_byte: 0,
            end_byte: 1000,
        },
    );

    let inner_class_id = EntityId(1);
    let inner_class = Entity::new(
        inner_class_id,
        EntityKind::Class,
        "InnerClass".to_string(),
        Span {
            start_position: cce_types::Position { row: 10, column: 4 },
            end_position: cce_types::Position { row: 40, column: 4 },
            start_byte: 200,
            end_byte: 800,
        },
    );

    outer_class.children = vec![inner_class_id];
    let entities = vec![outer_class, inner_class];

    let config = NestProcessorConfig {
        enable_nested_entity_grouping: true,
        max_nesting_depth: 0,
        min_nested_size: 1,
        ..Default::default()
    };
    let parsed_file = ParsedFile::new(Language::Java, "OuterClass.java".to_string(), "");
    let ctx = FileProcessingContext::new(&entities, &parsed_file, &config);

    let processor = ClassMethodProcessor::new(&GetterSetterDetectionConfig::default());
    let (groups, _) = processor.process(ctx);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].nested_groups.len(),
        0,
        "Max depth 0 should produce no nested groups"
    );
}
