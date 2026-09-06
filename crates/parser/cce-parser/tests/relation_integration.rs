//! Integration tests for the relation module
//!
//! Tests the full relation processing pipeline:
//! - IndexBuilder + RelationIndex construction
//! - Local and cross-file call resolution
//! - Call chain traversal (forward/backward/path-finding)
//! - Entity, file, import, export index operations
//! - File dependency graph tracking
//! - Hierarchy queries (inheritance, implementation)
//! - Frontend component queries

use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile, RawRelationData};
use cce_types::language::Language;
use cce_types::relation::{CallContext, ExternalCallType, RelationType, ResolvedRelation};

use cce_relation::{
    FileDependencyGraph, IndexError, RelationQueryError, UntypedDependency,
    index::{
        EntityIndexOps, ExportIndexOps, FileIndexOps, FileLevelOps, ImportIndexOps,
        LocalCallResolver, RelationQueryOps,
        builder::IndexBuilder,
        core::{CallChainNode, CallChainPath, ExportInfo, ExportType, RelationIndex},
        relation_query::{FrontendQueryOps, HierarchyQueryOps},
    },
    query::{CallChainQuery, CallChainTraverser, TraversalConfig, TraversalDirection},
};

use std::collections::{HashMap, HashSet};

// ============================================================
// Helper Functions
// ============================================================

fn create_entity(id: u64, kind: EntityKind, name: &str, start: usize, end: usize) -> Entity {
    Entity {
        id: EntityId(id),
        kind,
        name: name.to_string(),
        signature: String::new(),
        parameters: Vec::new(),
        return_type: None,
        span: Span {
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
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        stdlib_category: None,
        subtype: None,
    }
}

fn create_function(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::Function, name, start, end)
}

fn create_class(id: u64, name: &str, start: usize, end: usize) -> Entity {
    create_entity(id, EntityKind::Class, name, start, end)
}

fn create_resolved_relation(
    caller: u64,
    callee_id: Option<u64>,
    callee_name: &str,
    relation_type: RelationType,
) -> ResolvedRelation {
    ResolvedRelation {
        caller: EntityId(caller),
        callee_id: callee_id.map(EntityId),
        callee_name: callee_name.to_string(),
        relation_type,
        span: Span::default(),
        is_external: callee_id.is_none(),
        external_type: if callee_id.is_none() {
            Some(ExternalCallType::Unknown {
                raw_target: callee_name.to_string(),
            })
        } else {
            None
        },
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    }
}

/// Build a simple RelationIndex with entities and relations for testing
fn build_test_index() -> RelationIndex {
    let index = RelationIndex::new();

    // Add functions: main -> foo -> bar
    index.add_function_with_path(
        EntityId(1),
        create_function(1, "main", 0, 10),
        "main.rs".into(),
    );
    index.add_function_with_path(
        EntityId(2),
        create_function(2, "foo", 0, 10),
        "main.rs".into(),
    );
    index.add_function_with_path(
        EntityId(3),
        create_function(3, "bar", 0, 10),
        "utils.rs".into(),
    );

    // main calls foo, foo calls bar
    index.add_resolved_relation(create_resolved_relation(
        1,
        Some(2),
        "foo",
        RelationType::DirectCall,
    ));
    index.add_resolved_relation(create_resolved_relation(
        2,
        Some(3),
        "bar",
        RelationType::DirectCall,
    ));

    index
}

fn make_parsed_file(
    language: Language,
    path: &str,
    source: &str,
    entities: Vec<Entity>,
    raw_relations: Vec<RawRelationData>,
) -> ParsedFile {
    ParsedFile {
        path: path.to_string(),
        language,
        source: std::sync::Arc::from(source),
        entities,
        raw_relations,
        behavior: Default::default(),
        control_flow: Default::default(),
        local_symbols: HashMap::new(),
        import_table: None,
        reexports: Vec::new(),
        embedded_blocks: Vec::new(),
        block_relations: Vec::new(),
        file_doc_comment: None,
        file_doc_span: None,
        file_hash: None,
    }
}

fn make_raw_relation(src: u64, dst_name: &str, relation_type: RelationType) -> RawRelationData {
    RawRelationData {
        src: EntityId(src),
        level: cce_types::RelationLevel::Entity,
        dst_name: dst_name.to_string(),
        relation_type,
        span: Span::default(),
        stdlib_category: None,
    }
}

// ============================================================
// 1. IndexBuilder + RelationIndex Basics
// ============================================================

#[test]
fn test_index_builder_basic() {
    let builder = IndexBuilder::new();
    assert!(builder.is_empty());

    let index = builder.build();
    assert_eq!(index.function_count(), 0);
    assert_eq!(index.file_count(), 0);
}

#[test]
fn test_index_builder_add_single_file_via_process_file() {
    let builder = IndexBuilder::new();
    let file_info = cce_types::FileInfo {
        id: "test.rs".to_string(),
        path: "test.rs".to_string(),
        language: "Rust".to_string(),
        file_hash: String::new(),
        file_size: 0,
        modified_time: 0,
        parse_status: cce_types::entity::ParseStatus::Success,
        parse_errors: Vec::new(),
        parse_version: 0,
        entity_count: 2,
        relation_count: 1,
        export_count: 0,
        import_count: 0,
        depends_on: Vec::new(),
    };

    let functions = vec![
        (EntityId(1), create_function(1, "foo", 0, 5)),
        (EntityId(2), create_function(2, "bar", 6, 10)),
    ];

    let relations = vec![create_resolved_relation(
        1,
        Some(2),
        "bar",
        RelationType::DirectCall,
    )];

    builder.process_file(file_info, functions, relations, None, vec![]);
    let index = builder.build();

    assert!(!index.contains_file("nonexistent"));
    assert!(index.contains_file("test.rs"));
    assert_eq!(index.function_count(), 2);
    assert_eq!(index.call_count(), 1);
}

#[test]
fn test_index_from_existing() {
    let builder = IndexBuilder::from_index(RelationIndex::new());
    builder.add_function(EntityId(1), create_function(1, "hello", 0, 5));

    let index = builder.build();
    assert!(index.contains_function(EntityId(1)));
}

// ============================================================
// 2. EntityIndexOps
// ============================================================

#[test]
fn test_entity_index_operations() {
    let index = RelationIndex::new();

    index.add_function(EntityId(1), create_function(1, "func_a", 0, 5));
    index.add_function_with_path(
        EntityId(2),
        create_function(2, "func_b", 0, 5),
        "file.rs".into(),
    );
    index.add_functions(vec![
        (EntityId(3), create_function(3, "func_c", 0, 5)),
        (EntityId(4), create_function(4, "func_d", 0, 5)),
    ]);

    assert!(index.contains_function(EntityId(1)));
    assert!(!index.contains_function(EntityId(99)));
    assert_eq!(index.function_count(), 4);

    let ids = index.get_function_ids_by_name("func_a");
    assert_eq!(ids, vec![EntityId(1)]);

    let path = index.get_file_path_by_entity(EntityId(2));
    assert_eq!(path, Some("file.rs".to_string()));

    let entity = index.get_function_by_entity_id(EntityId(1));
    assert!(entity.is_some());
    assert_eq!(entity.unwrap().name, "func_a");
}

// ============================================================
// 3. RelationQueryOps (Forward/Reverse Lookups)
// ============================================================

#[test]
fn test_relation_query_forward_and_reverse() {
    let index = build_test_index();

    // Forward: main -> foo
    let relations = index.get_resolved_relations_by_caller(EntityId(1));
    assert!(relations.is_some());
    let relations = relations.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].callee_name, "foo");

    // Reverse: bar's callers = [foo]
    let callers = index.get_callers_by_callee_entity(EntityId(3));
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0], EntityId(2));

    // foo's callers = [main]
    let callers = index.get_callers_by_callee_entity(EntityId(2));
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0], EntityId(1));

    // No callers for main
    let callers = index.get_callers_by_callee_entity(EntityId(1));
    assert!(callers.is_empty());

    // Relations to entity: bar
    let to_bar = index.get_relations_to_entity(EntityId(3));
    assert_eq!(to_bar.len(), 1);
    assert_eq!(to_bar[0].caller, EntityId(2));
}

#[test]
fn test_relation_query_checked() {
    let index = build_test_index();

    // Checked lookup for existing entity
    let result = index.get_resolved_relations_by_caller_checked(EntityId(1));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);

    // Checked lookup for non-existing entity
    let result = index.get_resolved_relations_by_caller_checked(EntityId(99));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), IndexError::EntityNotFound(_)));

    // Callee checked
    let callers = index.get_callers_by_callee_entity_checked(EntityId(3));
    assert!(callers.is_ok());
    assert_eq!(callers.unwrap().len(), 1);

    // Unknown callee returns empty (not error)
    let callers = index.get_callers_by_callee_entity_checked(EntityId(999));
    assert!(callers.is_ok());
    assert!(callers.unwrap().is_empty());
}

#[test]
fn test_relation_query_by_type() {
    let index = RelationIndex::new();

    index.add_function(EntityId(1), create_function(1, "caller", 0, 5));
    index.add_function(EntityId(2), create_function(2, "callee", 0, 5));

    // Add DirectCall
    index.add_resolved_relation(create_resolved_relation(
        1,
        Some(2),
        "callee",
        RelationType::DirectCall,
    ));

    // Add TypeReference
    index.add_resolved_relation(ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(2)),
        callee_name: "callee".to_string(),
        relation_type: RelationType::TypeReference,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    });

    // Query by type
    let direct = index.get_relations_from_entity_by_type(EntityId(1), RelationType::DirectCall);
    assert_eq!(direct.len(), 1);

    let refs = index.get_relations_from_entity_by_type(EntityId(1), RelationType::TypeReference);
    assert_eq!(refs.len(), 1);

    let callers = index.get_callers_by_callee_and_type(EntityId(2), RelationType::DirectCall);
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0], EntityId(1));

    // No Implementation relations
    let impls = index.get_relations_from_entity_by_type(EntityId(1), RelationType::Implementation);
    assert!(impls.is_empty());
}

// ============================================================
// 4. HierarchyQueryOps
// ============================================================

#[test]
fn test_hierarchy_queries_integration() {
    let index = RelationIndex::new();

    // Setup: Animal <- Dog <- Puppy, Animal <- Cat
    index.add_function(EntityId(1), create_class(1, "Animal", 0, 5));
    index.add_function(EntityId(2), create_class(2, "Dog", 0, 5));
    index.add_function(EntityId(3), create_class(3, "Puppy", 0, 5));
    index.add_function(EntityId(4), create_class(4, "Cat", 0, 5));

    // Dog extends Animal, Puppy extends Dog, Cat extends Animal
    index.add_resolved_relation(create_resolved_relation(
        2,
        Some(1),
        "Animal",
        RelationType::Inheritance,
    ));
    index.add_resolved_relation(create_resolved_relation(
        3,
        Some(2),
        "Dog",
        RelationType::Inheritance,
    ));
    index.add_resolved_relation(create_resolved_relation(
        4,
        Some(1),
        "Animal",
        RelationType::Inheritance,
    ));

    // Derived classes of Animal: [Dog, Cat]
    let derived = index.get_derived_classes(EntityId(1));
    assert_eq!(derived.len(), 2);
    assert!(derived.contains(&EntityId(2)));
    assert!(derived.contains(&EntityId(4)));

    // Base classes of Puppy: [Dog]
    let bases = index.get_base_classes(EntityId(3));
    assert_eq!(bases.len(), 1);
    assert_eq!(bases[0], EntityId(2));

    // Base classes of Dog: [Animal]
    let bases = index.get_base_classes(EntityId(2));
    assert_eq!(bases.len(), 1);
    assert_eq!(bases[0], EntityId(1));

    // Base classes of Animal: []
    let bases = index.get_base_classes(EntityId(1));
    assert!(bases.is_empty());
}

// ============================================================
// 5. FileIndexOps / ImportIndexOps / ExportIndexOps
// ============================================================

#[test]
fn test_file_index_operations() {
    let index = RelationIndex::new();

    let file_info = cce_types::FileInfo {
        id: "src/main.rs".to_string(),
        path: "src/main.rs".to_string(),
        language: "Rust".to_string(),
        file_hash: "abc".to_string(),
        file_size: 100,
        modified_time: 12345,
        parse_status: cce_types::entity::ParseStatus::Success,
        parse_errors: Vec::new(),
        parse_version: 1,
        entity_count: 5,
        relation_count: 3,
        export_count: 2,
        import_count: 1,
        depends_on: vec!["utils.rs".to_string()],
    };

    index.add_file(file_info.clone());

    assert!(index.contains_file("src/main.rs"));
    assert!(!index.contains_file("nonexistent.rs"));

    let retrieved = index.get_file("src/main.rs");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().file_size, 100);

    assert_eq!(index.file_count(), 1);
}

#[test]
fn test_import_export_index_operations() {
    let index = RelationIndex::new();

    // Imports
    let import_table = cce_types::ImportTable {
        file_id: "main.rs".to_string(),
        standardized_imports: vec![cce_types::import::StandardizedImport {
            kind: cce_types::import::ImportKind::SymbolImport,
            source: "std::collections".to_string(),
            target: cce_types::import::ImportTarget::default(),
            alias: None,
            is_wildcard: false,
            is_default: false,
            is_system_header: false,
            is_relative: false,
            span: None,
        }],
        source_stats: cce_types::import::ImportSourceStats::default(),
    };

    index.add_import_table("main.rs".to_string(), import_table.clone());
    assert!(index.has_imports("main.rs"));
    assert!(!index.has_imports("other.rs"));
    assert_eq!(index.import_count(), 1);

    let retrieved = index.get_import_table("main.rs");
    assert!(retrieved.is_some());

    // Exports
    let exports = vec![
        ExportInfo {
            function_id: EntityId(1),
            function_name: "pub_func".to_string(),
            export_type: ExportType::Named,
        },
        ExportInfo {
            function_id: EntityId(2),
            function_name: "default_export".to_string(),
            export_type: ExportType::Default,
        },
    ];

    index.add_exports("main.rs".to_string(), exports);

    let found = index.find_export_by_name("main.rs", "pub_func");
    assert!(found.is_some());
    assert_eq!(found.unwrap().export_type, ExportType::Named);

    let not_found = index.find_export_by_name("main.rs", "nonexistent");
    assert!(not_found.is_none());

    // Add single export
    index.add_export(
        "utils.rs",
        ExportInfo {
            function_id: EntityId(3),
            function_name: "helper".to_string(),
            export_type: ExportType::Named,
        },
    );

    let utils_exports = index.get_exports("utils.rs");
    assert!(utils_exports.is_some());
    assert_eq!(utils_exports.unwrap().len(), 1);
}

// ============================================================
// 6. FileLevelOps (Entity Relations by File)
// ============================================================

#[test]
fn test_file_level_operations() {
    let index = build_test_index();

    // Get entities by file
    let main_entities = index.get_entity_ids_by_file("main.rs");
    assert_eq!(main_entities.len(), 2);
    assert!(main_entities.contains(&EntityId(1)));
    assert!(main_entities.contains(&EntityId(2)));

    let utils_entities = index.get_entity_ids_by_file("utils.rs");
    assert_eq!(utils_entities.len(), 1);
    assert_eq!(utils_entities[0], EntityId(3));

    // Get full entities
    let main_full = index.get_entities_by_file("main.rs");
    assert_eq!(main_full.len(), 2);

    // Get relations by file
    let relations = index.get_resolved_relations_by_file("main.rs");
    assert_eq!(relations.len(), 2); // main -> foo, and foo has no outgoing in main.rs... wait

    // main (id=1) has 1 relation, foo (id=2) has 1 relation -> bar
    // But bar is in utils.rs -> the relation from foo to bar is still stored by caller (foo in main.rs)
    // So both entities in main.rs have 1 relation each -> 2 entries
    let main_relations: Vec<_> = relations
        .iter()
        .filter(|(id, _)| *id == EntityId(1))
        .collect();
    assert_eq!(main_relations.len(), 1);

    let foo_relations: Vec<_> = relations
        .iter()
        .filter(|(id, _)| *id == EntityId(2))
        .collect();
    assert_eq!(foo_relations.len(), 1);
}

// ============================================================
// 7. CallChainTraverser (Forward/Backward/Path Finding)
// ============================================================

#[test]
fn test_call_chain_forward_traversal() {
    let index = build_test_index();

    let config = TraversalConfig::default()
        .with_max_depth(5)
        .with_include_start_node(true)
        .with_direction(TraversalDirection::Forward);

    let traverser = CallChainTraverser::new(&index, config);
    let nodes = traverser
        .traverse_from(EntityId(1))
        .expect("Traversal should succeed");

    // main (depth 0) -> foo (depth 1) -> bar (depth 2)
    assert!(
        nodes.len() >= 3,
        "Expected at least 3 nodes, got {}",
        nodes.len()
    );

    let main_node = nodes.iter().find(|n| n.function_name == "main");
    let foo_node = nodes.iter().find(|n| n.function_name == "foo");
    let bar_node = nodes.iter().find(|n| n.function_name == "bar");

    assert!(main_node.is_some());
    assert!(foo_node.is_some());
    assert!(bar_node.is_some());

    assert_eq!(main_node.unwrap().depth, 0);
    assert_eq!(foo_node.unwrap().depth, 1);
    assert_eq!(bar_node.unwrap().depth, 2);
}

#[test]
fn test_call_chain_backward_traversal() {
    let index = build_test_index();

    let config = TraversalConfig::default()
        .with_max_depth(5)
        .with_direction(TraversalDirection::Backward);

    let traverser = CallChainTraverser::new(&index, config);
    let nodes = traverser
        .traverse_from(EntityId(3))
        .expect("Backward traversal should succeed");

    // bar's callers: foo -> main
    assert!(
        nodes.len() >= 2,
        "Expected at least 2 callers, got {}",
        nodes.len()
    );

    let foo_found = nodes.iter().any(|n| n.function_name == "foo");
    let main_found = nodes.iter().any(|n| n.function_name == "main");

    assert!(foo_found, "foo should be found as caller of bar");
    assert!(main_found, "main should be found as transitive caller");
}

#[test]
fn test_call_chain_path_finding() {
    let index = build_test_index();

    let config = TraversalConfig::default().with_max_depth(10);

    let traverser = CallChainTraverser::new(&index, config);
    let path = traverser
        .find_path(EntityId(1), EntityId(3))
        .expect("Path finding should succeed");

    assert!(path.is_some(), "Path from main to bar should exist");
    let path = path.unwrap();

    assert!(path.len() >= 3, "Path should have at least 3 nodes");
    assert_eq!(path[0].function_name, "main");
    assert_eq!(path[path.len() - 1].function_name, "bar");
}

#[test]
fn test_call_chain_path_not_found() {
    let index = build_test_index();

    // Create a standalone entity not connected to the graph
    index.add_function(EntityId(10), create_function(10, "isolated", 0, 5));

    let config = TraversalConfig::default().with_max_depth(10);

    let traverser = CallChainTraverser::new(&index, config);
    let path = traverser
        .find_path(EntityId(1), EntityId(10))
        .expect("Path finding should not error");

    assert!(
        path.is_none(),
        "No path should exist between disconnected entities"
    );
}

#[test]
fn test_call_chain_empty_result_for_missing_entity() {
    let index = RelationIndex::new();

    let config = TraversalConfig::default()
        .with_max_depth(5)
        .with_include_start_node(false)
        .with_direction(TraversalDirection::Forward);

    let traverser = CallChainTraverser::new(&index, config);
    let result = traverser.traverse_from(EntityId(999));

    // Traverser validates start entity existence for forward traversal
    // and returns NotFound when the entity doesn't exist
    assert!(result.is_err(), "Should error for non-existent entity");
    match &result {
        Err(RelationQueryError::NotFound(msg)) => {
            assert!(
                msg.contains("999"),
                "Error should reference the missing entity ID: {}",
                msg
            );
        }
        _ => panic!("Expected NotFound error, got: {:?}", result),
    }
}

#[test]
fn test_call_chain_traversal_with_max_depth_limit() {
    let index = build_test_index();

    let config = TraversalConfig::default()
        .with_max_depth(1)
        .with_include_start_node(true)
        .with_direction(TraversalDirection::Forward);

    let traverser = CallChainTraverser::new(&index, config);
    let nodes = traverser
        .traverse_from(EntityId(1))
        .expect("Traversal should succeed");

    // Only main (depth 0, included) and foo (depth 1) should be present
    let main_found = nodes.iter().any(|n| n.function_name == "main");
    let foo_found = nodes.iter().any(|n| n.function_name == "foo");
    let bar_found = nodes.iter().any(|n| n.function_name == "bar");

    assert!(main_found, "main should be included");
    assert!(foo_found, "foo should be included (depth 1)");
    assert!(!bar_found, "bar should be excluded (depth 2 > max_depth 1)");
}

// ============================================================
// 8. CallChainQuery (High-level API)
// ============================================================

#[test]
fn test_call_chain_query_get_callees_and_callers() {
    let index = build_test_index();
    let query = CallChainQuery::from_index(index);

    // Get callees of main
    let callees = query
        .get_callees_by_entity(EntityId(1))
        .expect("Should find callees");
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0].callee_name, "foo");

    // Get callers of bar
    let callers = query.get_callers_by_entity(EntityId(3));
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0], EntityId(2));

    // Safe get for non-existing entity
    let safe = query.get_callees_by_entity_safe(EntityId(999));
    assert!(safe.is_empty());

    // Error for non-existing entity in checked API
    let result = query.get_callees_by_entity(EntityId(999));
    assert!(result.is_err());
}

#[test]
fn test_call_chain_query_inheritance() {
    let index = RelationIndex::new();

    index.add_function(EntityId(1), create_class(1, "Base", 0, 5));
    index.add_function(EntityId(2), create_class(2, "Derived", 0, 5));
    index.add_resolved_relation(create_resolved_relation(
        2,
        Some(1),
        "Base",
        RelationType::Inheritance,
    ));

    let query = CallChainQuery::from_index(index);

    let derived = query.get_derived_classes(EntityId(1));
    assert_eq!(derived.len(), 1);

    let bases = query.get_base_classes(EntityId(2));
    assert_eq!(bases.len(), 1);

    // Inheritance hierarchy (ancestors)
    let ancestors = query.get_inheritance_hierarchy(EntityId(2), 5);
    assert_eq!(ancestors.len(), 1);
    assert_eq!(ancestors[0], EntityId(1));

    // All derived classes
    let all_derived = query.get_all_derived_classes(EntityId(1), 5);
    assert_eq!(all_derived.len(), 1);
    assert_eq!(all_derived[0], EntityId(2));
}

// ============================================================
// 9. LocalCallResolver
// ============================================================

#[test]
fn test_local_call_resolver_from_parsed_file() {
    let resolver = LocalCallResolver::new();

    let entities = vec![
        create_function(0, "caller_func", 0, 10),
        create_function(1, "local_target", 0, 5),
    ];

    let raw_relations = vec![make_raw_relation(
        0,
        "local_target",
        RelationType::DirectCall,
    )];

    let parsed = make_parsed_file(
        Language::Rust,
        "test.rs",
        "fn caller() { local_target(); }",
        entities,
        raw_relations,
    );

    let local_calls = resolver.resolve_from_parsed_file(&parsed);

    assert_eq!(local_calls.len(), 1);
    assert_eq!(local_calls[0].caller, EntityId(0));
    assert_eq!(local_calls[0].callee, EntityId(1));
    assert_eq!(local_calls[0].callee_name, "local_target");
}

#[test]
fn test_local_call_resolver_skips_cross_file() {
    let resolver = LocalCallResolver::new();

    let entities = vec![create_function(0, "caller_func", 0, 10)];

    let raw_relations = vec![make_raw_relation(
        0,
        "external_func",
        RelationType::DirectCall,
    )];

    let parsed = make_parsed_file(
        Language::Rust,
        "test.rs",
        "fn caller() { external_func(); }",
        entities,
        raw_relations,
    );

    let local_calls = resolver.resolve_from_parsed_file(&parsed);

    assert!(local_calls.is_empty(), "Cross-file calls should be skipped");
}

#[test]
fn test_local_call_resolver_with_signature_matching_config() {
    let config = cce_relation::index::LocalCallResolverConfig {
        enable_signature_matching: true,
        skip_cross_file_calls: true,
        log_unresolved_calls: true,
    };
    let resolver = LocalCallResolver::with_config(config);

    let entities = vec![
        create_function(0, "foo", 0, 5),
        create_function(1, "bar", 6, 10),
    ];

    let raw_relations = vec![make_raw_relation(0, "bar", RelationType::DirectCall)];

    let parsed = make_parsed_file(
        Language::Rust,
        "test.rs",
        "fn foo() { bar(); }",
        entities,
        raw_relations,
    );

    let local_calls = resolver.resolve_from_parsed_file(&parsed);

    assert_eq!(local_calls.len(), 1);
    assert_eq!(local_calls[0].caller, EntityId(0));
    assert_eq!(local_calls[0].callee, EntityId(1));
    assert_eq!(local_calls[0].callee_name, "bar");
}

// ============================================================
// 10. FileDependencyGraph
// ============================================================

#[test]
fn test_dependency_graph_basic() {
    let graph = FileDependencyGraph::new();

    graph.add_dependency("main.rs", "utils.rs");
    graph.add_dependency("main.rs", "models.rs");
    graph.add_dependency("utils.rs", "helpers.rs");

    assert!(graph.has_dependency("main.rs", "utils.rs"));
    assert!(graph.has_dependency("main.rs", "models.rs"));
    assert!(graph.has_dependency("utils.rs", "helpers.rs"));
    assert!(!graph.has_dependency("main.rs", "helpers.rs"));

    let deps = graph.get_dependencies("main.rs");
    assert_eq!(deps.len(), 2);
    assert!(deps.contains(&"utils.rs".to_string()));
    assert!(deps.contains(&"models.rs".to_string()));

    let dependents = graph.get_dependents("utils.rs");
    assert_eq!(dependents.len(), 1);
    assert_eq!(dependents[0], "main.rs");
}

#[test]
fn test_dependency_graph_no_self_dependency() {
    let graph = FileDependencyGraph::new();
    graph.add_dependency("self.rs", "self.rs");

    assert!(!graph.has_dependency("self.rs", "self.rs"));
    assert!(!graph.has_dependencies("self.rs"));
}

#[test]
fn test_dependency_graph_remove_file() {
    let graph = FileDependencyGraph::new();

    graph.add_dependency("main.rs", "utils.rs");
    graph.add_dependency("app.rs", "utils.rs");

    assert!(graph.has_dependents("utils.rs"));
    assert_eq!(graph.get_dependents("utils.rs").len(), 2);

    graph.remove_file("utils.rs");

    assert!(!graph.has_dependents("utils.rs"));
    assert_eq!(graph.get_dependents("utils.rs").len(), 0);
    assert!(!graph.has_dependency("main.rs", "utils.rs"));
}

#[test]
fn test_dependency_graph_multiple_dependencies() {
    let graph = FileDependencyGraph::new();

    graph.add_dependencies(
        "main.rs",
        &[
            "lib.rs".to_string(),
            "config.rs".to_string(),
            "utils.rs".to_string(),
        ],
    );

    let deps = graph.get_dependencies("main.rs");
    assert_eq!(deps.len(), 3);

    let all_files = graph.get_all_files();
    assert!(all_files.contains(&"main.rs".to_string()));
    assert!(all_files.contains(&"lib.rs".to_string()));

    // Topological sort with no cycles
    let sorted = graph.topological_sort(&[
        "main.rs".to_string(),
        "lib.rs".to_string(),
        "utils.rs".to_string(),
        "config.rs".to_string(),
    ]);
    assert!(sorted.is_ok());
    let sorted = sorted.unwrap();
    // main.rs should come after its dependencies
    let main_pos = sorted.iter().position(|f| f == "main.rs");
    let lib_pos = sorted.iter().position(|f| f == "lib.rs");
    assert!(main_pos.is_some() && lib_pos.is_some());
    assert!(main_pos > lib_pos, "main.rs should come after lib.rs");
}

#[test]
fn test_dependency_graph_cycle_detection() {
    let graph = FileDependencyGraph::new();

    graph.add_dependency("a.rs", "b.rs");
    graph.add_dependency("b.rs", "c.rs");
    graph.add_dependency("c.rs", "a.rs");

    let sorted =
        graph.topological_sort(&["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()]);
    assert!(sorted.is_err());
    match sorted {
        Err(cce_relation::DependencyGraphError::CycleDetected(msg)) => {
            assert!(
                msg.contains("a.rs") || msg.contains("cycle"),
                "Cycle should be detected"
            );
        }
        _ => panic!("Expected CycleDetected error"),
    }
}

// ============================================================
// 11. FrontendQueryOps
// ============================================================

#[test]
fn test_frontend_component_queries() {
    let index = RelationIndex::new();

    index.add_function(EntityId(1), create_function(1, "ParentComponent", 0, 5));
    index.add_function(EntityId(2), create_function(2, "ChildComponent", 0, 5));
    index.add_function(EntityId(3), create_function(3, "handleClick", 0, 5));

    // ElementContains: Parent contains Child
    index.add_resolved_relation(ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(2)),
        callee_name: "ChildComponent".to_string(),
        relation_type: RelationType::ElementContains,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    });

    // EventCallback: Parent binds to handleClick
    index.add_resolved_relation(ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(3)),
        callee_name: "handleClick".to_string(),
        relation_type: RelationType::EventCallback,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    });

    // ParameterBinding: Parent passes props to Child
    index.add_resolved_relation(ResolvedRelation {
        caller: EntityId(1),
        callee_id: Some(EntityId(2)),
        callee_name: "ChildComponent".to_string(),
        relation_type: RelationType::ParameterBinding,
        span: Span::default(),
        is_external: false,
        external_type: None,
        callee_symbol: None,
        stdlib_category: None,
        owner_type: None,
        call_context: CallContext::Direct,
        overload_signature: None,
    });

    // Query child elements
    let children = index.get_child_elements(EntityId(1));
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], EntityId(2));

    // Query parent element
    let parents = index.get_parent_element(EntityId(2));
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0], EntityId(1));

    // Query event handlers
    let handlers = index.get_event_handlers(EntityId(1));
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].callee_name, "handleClick");

    // Query elements by handler
    let elements = index.get_elements_by_handler(EntityId(3));
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0], EntityId(1));

    // Query parameter bindings
    let bindings = index.get_parameter_bindings(EntityId(1));
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].callee_name, "ChildComponent");
}

// ============================================================
// 12. RelationIndex Clear and Reset
// ============================================================

#[test]
fn test_relation_index_clear() {
    let index = build_test_index();

    assert_eq!(index.function_count(), 3);
    assert_eq!(index.call_count(), 2);

    index.clear();

    assert_eq!(index.function_count(), 0);
    assert_eq!(index.call_count(), 0);
    assert_eq!(index.file_count(), 0);
}

// ============================================================
// 13. Error Types
// ============================================================

#[test]
fn test_index_error_types() {
    let err = IndexError::entity_not_found(EntityId(42));
    assert!(format!("{}", err).contains("42"));

    let err = IndexError::file_not_found("missing.rs");
    assert!(format!("{}", err).contains("missing.rs"));

    let err = IndexError::inconsistent_state("mismatch");
    assert!(format!("{}", err).contains("mismatch"));
}

#[test]
fn test_query_error_types() {
    let err = RelationQueryError::not_found("missing entity");
    assert!(format!("{}", err).contains("missing entity"));

    let err = RelationQueryError::invalid_query("bad params");
    assert!(format!("{}", err).contains("bad params"));

    let err = RelationQueryError::path_not_found("A", "B", 5);
    let msg = format!("{}", err);
    assert!(msg.contains("A"));
    assert!(msg.contains("B"));
}

// ============================================================
// 14. IndexBuilder Batch Processing
// ============================================================

#[test]
fn test_index_builder_process_file_with_imports_and_exports() {
    let builder = IndexBuilder::new();

    let file_info = cce_types::FileInfo {
        id: "module.rs".to_string(),
        path: "module.rs".to_string(),
        language: "Rust".to_string(),
        file_hash: String::new(),
        file_size: 0,
        modified_time: 0,
        parse_status: cce_types::entity::ParseStatus::Success,
        parse_errors: Vec::new(),
        parse_version: 0,
        entity_count: 1,
        relation_count: 0,
        export_count: 1,
        import_count: 1,
        depends_on: vec![],
    };

    let functions = vec![(EntityId(1), create_function(1, "greet", 0, 5))];

    let import_table = cce_types::ImportTable {
        file_id: "module.rs".to_string(),
        standardized_imports: vec![cce_types::import::StandardizedImport {
            kind: cce_types::import::ImportKind::SymbolImport,
            source: "std::io".to_string(),
            target: cce_types::import::ImportTarget::default(),
            alias: None,
            is_wildcard: false,
            is_default: false,
            is_system_header: false,
            is_relative: false,
            span: None,
        }],
        source_stats: cce_types::import::ImportSourceStats::default(),
    };

    let exports = vec![ExportInfo {
        function_id: EntityId(1),
        function_name: "greet".to_string(),
        export_type: ExportType::Named,
    }];

    builder.process_file(file_info, functions, vec![], Some(import_table), exports);

    let index = builder.build();

    assert!(index.contains_file("module.rs"));
    assert!(index.has_imports("module.rs"));
    assert_eq!(index.import_count(), 1);

    let exports = index.get_exports("module.rs");
    assert!(exports.is_some());
    assert_eq!(exports.unwrap().len(), 1);
}

// ============================================================
// 15. Cross-File Resolution via IndexBuilder add_parsed_files
// ============================================================

#[test]
fn test_index_builder_add_parsed_files_single_file() {
    let builder = IndexBuilder::new();

    let entities = vec![
        create_function(0, "top_level", 0, 10),
        create_function(1, "helper", 11, 20),
    ];

    let raw_relations = vec![make_raw_relation(0, "helper", RelationType::DirectCall)];

    let parsed = make_parsed_file(
        Language::Rust,
        "src/lib.rs",
        "fn top_level() { helper(); }",
        entities,
        raw_relations,
    );

    // Use add_parsed_file for single-file processing
    builder.add_parsed_file(&parsed);

    let index = builder.build();

    assert!(index.contains_function(EntityId(0)));
    assert!(index.contains_function(EntityId(1)));

    // Check that the local call was resolved
    let relations = index.get_resolved_relations_by_caller(EntityId(0));
    assert!(relations.is_some());
    assert_eq!(relations.unwrap().len(), 1);
}

// ============================================================
// 16. IndexBuilder External Package Classification
// ============================================================

#[test]
fn test_index_builder_with_external_packages() {
    let mut builder = IndexBuilder::new();

    let mut packages = HashMap::new();
    let mut rust_packages = HashSet::new();
    rust_packages.insert("serde".to_string());
    rust_packages.insert("tokio".to_string());
    packages.insert(Language::Rust, rust_packages);

    builder.set_all_external_packages(packages);

    let external_packages = builder.get_external_packages();
    assert!(external_packages.is_some());

    let packages_ref = external_packages.unwrap();
    let rust_pkgs = packages_ref.get(&Language::Rust);
    assert!(rust_pkgs.is_some());
    assert!(rust_pkgs.unwrap().contains("serde"));
}

// ============================================================
// 17. Complex Call Chain Scenarios
// ============================================================

#[test]
fn test_complex_call_chain_with_branches() {
    let index = RelationIndex::new();

    // Setup: main -> [foo, bar], foo -> baz, bar -> baz
    index.add_function(EntityId(1), create_function(1, "main", 0, 10));
    index.add_function(EntityId(2), create_function(2, "foo", 0, 10));
    index.add_function(EntityId(3), create_function(3, "bar", 0, 10));
    index.add_function(EntityId(4), create_function(4, "baz", 0, 10));

    index.add_resolved_relation(create_resolved_relation(
        1,
        Some(2),
        "foo",
        RelationType::DirectCall,
    ));
    index.add_resolved_relation(create_resolved_relation(
        1,
        Some(3),
        "bar",
        RelationType::DirectCall,
    ));
    index.add_resolved_relation(create_resolved_relation(
        2,
        Some(4),
        "baz",
        RelationType::DirectCall,
    ));
    index.add_resolved_relation(create_resolved_relation(
        3,
        Some(4),
        "baz",
        RelationType::DirectCall,
    ));

    // Forward traversal from main
    let config = TraversalConfig::default()
        .with_max_depth(5)
        .with_include_start_node(true);
    let traverser = CallChainTraverser::new(&index, config);
    let nodes = traverser
        .traverse_from(EntityId(1))
        .expect("Traversal should succeed");

    assert!(nodes.iter().any(|n| n.function_name == "main"));
    assert!(nodes.iter().any(|n| n.function_name == "foo"));
    assert!(nodes.iter().any(|n| n.function_name == "bar"));
    assert!(nodes.iter().any(|n| n.function_name == "baz"));

    // Callee index for baz should have 2 callers
    let callers = index.get_callers_by_callee_entity(EntityId(4));
    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&EntityId(2)));
    assert!(callers.contains(&EntityId(3)));

    // Relations to baz
    let to_baz = index.get_relations_to_entity(EntityId(4));
    assert_eq!(to_baz.len(), 2);
}

// ============================================================
// 18. TraversalConfig Validation
// ============================================================

#[test]
fn test_traversal_config_validation() {
    let config = TraversalConfig::default().with_max_depth(0);
    assert!(config.validate().is_err());

    let config = TraversalConfig::default().with_max_nodes(0);
    assert!(config.validate().is_err());

    let config = TraversalConfig::default().with_max_depth(5);
    assert!(config.validate().is_ok());
}

// ============================================================
// 19. DepedencyIndex
// ============================================================

#[test]
fn test_dependency_index_prefix_matching() {
    use cce_relation::index::DependencyIndex;

    let mut deps = HashMap::new();
    deps.insert(
        Language::Python,
        vec![
            UntypedDependency::external("numpy"),
            UntypedDependency::external("django"),
        ],
    );
    deps.insert(
        Language::Rust,
        vec![
            UntypedDependency::external("serde"),
            UntypedDependency::dev("tokio"),
        ],
    );

    let index = DependencyIndex::build(&deps);

    // Exact match
    let found = index.find_dependency(Language::Python, "numpy");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "numpy");

    // Prefix match (numpy.core should match numpy)
    let found = index.find_dependency(Language::Python, "numpy.core");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "numpy");

    // No match
    let found = index.find_dependency(Language::Python, "flask");
    assert!(found.is_none());

    // Different language
    let found = index.find_dependency(Language::Rust, "serde");
    assert!(found.is_some());

    // Stats
    let stats = index.stats();
    assert_eq!(stats.total, 4);
    assert_eq!(stats.external_count, 3); // serde, numpy, django
    assert_eq!(stats.dev_count, 1); // tokio
}

// ============================================================
// 20. CallChainNode Structure
// ============================================================

#[test]
fn test_call_chain_node_structure() {
    let node = CallChainNode {
        function_id: EntityId(1),
        function_name: "my_func".to_string(),
        file_path: "src/lib.rs".to_string(),
        depth: 3,
        relation_type: RelationType::DirectCall,
        call_line: Some(42),
        owner_type: None,
        call_context: CallContext::Direct,
    };

    assert_eq!(node.function_id, EntityId(1));
    assert_eq!(node.function_name, "my_func");
    assert_eq!(node.depth, 3);
    assert_eq!(node.call_line, Some(42));

    // Create a path from nodes
    let path = CallChainPath {
        nodes: vec![node],
        length: 1,
    };

    assert_eq!(path.length, 1);
    assert_eq!(path.nodes[0].function_name, "my_func");
}

// ============================================================
// Source-Parsing Integration Tests
//
// These tests parse actual source code through ParseCoordinator
// and verify that entity extraction and relation extraction work
// end-to-end for Java, C#, Kotlin, PHP, and Ruby.
// ============================================================

use cce_parser::parser::coordinator::ParseCoordinator;
use cce_types::language::{FileType, LanguageInfo};

fn lang_info_for(lang: cce_types::language::Language, ext: &str) -> LanguageInfo {
    LanguageInfo {
        language: lang,
        file_type: FileType::Source,
        extensions: vec![ext.to_string()],
    }
}

fn parse_source_with_lang(
    code: &str,
    file_path: &str,
    lang: cce_types::language::Language,
    ext: &str,
) -> ParsedFile {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(lang, ext);
    coordinator
        .parse_with_language_info(file_path, code, &info)
        .unwrap_or_else(|e| panic!("Failed to parse {file_path}: {e}"))
}

// --------------------------------------------------
// Java
// --------------------------------------------------

#[test]
fn test_java_method_calls_extracted() {
    let code = r#"
class Calculator {
    int add(int a, int b) {
        return a + b;
    }

    int multiply(int a, int b) {
        return a * b;
    }

    int compute() {
        int x = add(1, 2);
        int y = multiply(x, 3);
        return y;
    }
}
"#;
    let parsed = parse_source_with_lang(code, "src/main.java", Language::Java, "java");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"compute"),
        "should extract compute method, got: {entity_names:?}"
    );

    let call_relations: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            r.relation_type == RelationType::DirectCall
                || r.relation_type == RelationType::InstanceMethodCall
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        call_relations.contains(&"add"),
        "should detect call to add(), got: {call_relations:?}"
    );
    assert!(
        call_relations.contains(&"multiply"),
        "should detect call to multiply(), got: {call_relations:?}"
    );
}

#[test]
fn test_java_import_dependency_extracted() {
    let code = r#"
import java.util.List;
import java.util.ArrayList;

class Foo {
    List<String> items = new ArrayList<>();
}
"#;
    let parsed = parse_source_with_lang(code, "src/Foo.java", Language::Java, "java");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"Foo"),
        "should extract Foo class, got: {entity_names:?}"
    );

    let imports: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::ImportStandard | RelationType::ImportNamed | RelationType::Use
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        imports.iter().any(|i| i.contains("List")),
        "should detect import of List, got: {imports:?}"
    );
    assert!(
        imports.iter().any(|i| i.contains("ArrayList")),
        "should detect import of ArrayList, got: {imports:?}"
    );
}

// --------------------------------------------------
// C#
// --------------------------------------------------

#[test]
fn test_csharp_method_calls_extracted() {
    let code = r#"
using System;

class Logger {
    void LogMessage(string msg) {
        Console.WriteLine(msg);
    }

    void Process() {
        LogMessage("hello");
    }
}
"#;
    let parsed = parse_source_with_lang(code, "src/Logger.cs", Language::CSharp, "cs");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"Process"),
        "should extract Process method, got: {entity_names:?}"
    );

    let call_relations: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::DirectCall
                    | RelationType::InstanceMethodCall
                    | RelationType::StaticMethodCall
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        call_relations.contains(&"LogMessage"),
        "should detect call to LogMessage(), got: {call_relations:?}"
    );
}

#[test]
fn test_csharp_using_directive_extracted() {
    let code = r#"
using System;
using System.Collections.Generic;

class App {
    List<int> numbers = new List<int>();
}
"#;
    let parsed = parse_source_with_lang(code, "src/App.cs", Language::CSharp, "cs");

    let imports: Vec<String> = parsed
        .import_table
        .as_ref()
        .map(|it| {
            it.standardized_imports
                .iter()
                .map(|si| si.source.clone())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        imports.iter().any(|i| i == "System"),
        "should detect using System, got: {imports:?}"
    );
    assert!(
        imports
            .iter()
            .any(|i| i.contains("Collections") && i.contains("Generic")),
        "should detect using System.Collections.Generic, got: {imports:?}"
    );
}

// --------------------------------------------------
// Kotlin
// --------------------------------------------------

#[test]
fn test_kotlin_function_calls_extracted() {
    let code = r#"
fun greet(name: String): String {
    return "Hello, $name"
}

fun main() {
    val msg = greet("World")
    println(msg)
}
"#;
    let parsed = parse_source_with_lang(code, "src/main.kt", Language::Kotlin, "kt");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"main"),
        "should extract main function, got: {entity_names:?}"
    );
    assert!(
        entity_names.contains(&"greet"),
        "should extract greet function, got: {entity_names:?}"
    );

    let call_relations: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::DirectCall | RelationType::InstanceMethodCall
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        call_relations.contains(&"greet"),
        "should detect call to greet(), got: {call_relations:?}"
    );
    assert!(
        call_relations.contains(&"println"),
        "should detect call to println(), got: {call_relations:?}"
    );
}

#[test]
fn test_kotlin_import_extracted() {
    let code = r#"
import kotlin.collections.MutableList
import kotlin.io.println

class Example {
    fun run() {
        println("hello")
    }
}
"#;
    let parsed = parse_source_with_lang(code, "src/Example.kt", Language::Kotlin, "kt");

    let imports: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::ImportStandard | RelationType::ImportNamed | RelationType::Use
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        imports.iter().any(|i| i.contains("println")),
        "should detect import of println, got: {imports:?}"
    );
}

// --------------------------------------------------
// PHP
// --------------------------------------------------

#[test]
fn test_php_method_calls_extracted() {
    let code = r#"<?php
class MathHelper {
    public function add(int $a, int $b): int {
        return $a + $b;
    }

    public function compute(): int {
        return $this->add(1, 2);
    }
}
"#;
    let parsed = parse_source_with_lang(code, "src/MathHelper.php", Language::Php, "php");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"compute"),
        "should extract compute method, got: {entity_names:?}"
    );

    let call_relations: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::DirectCall | RelationType::InstanceMethodCall
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        call_relations.iter().any(|c| c.contains("add")),
        "should detect call to add(), got: {call_relations:?}"
    );
}

#[test]
fn test_php_use_statement_extracted() {
    let code = r#"<?php
use App\Models\User;
use App\Services\AuthService;

class Controller {
    public function index() {}
}
"#;
    let parsed = parse_source_with_lang(code, "src/Controller.php", Language::Php, "php");

    let imports: Vec<String> = parsed
        .import_table
        .as_ref()
        .map(|it| {
            it.standardized_imports
                .iter()
                .map(|si| si.source.clone())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        imports.iter().any(|i| i.contains("User")),
        "should detect use App\\Models\\User, got: {imports:?}"
    );
    assert!(
        imports.iter().any(|i| i.contains("AuthService")),
        "should detect use App\\Services\\AuthService, got: {imports:?}"
    );
}

// --------------------------------------------------
// Ruby
// --------------------------------------------------

#[test]
fn test_ruby_method_calls_extracted() {
    let code = r#"
class Greeting
  def hello(name)
    "Hello, #{name}"
  end

  def greet
    hello("World")
  end
end
"#;
    let parsed = parse_source_with_lang(code, "src/greeting.rb", Language::Ruby, "rb");

    let entity_names: Vec<&str> = parsed.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        entity_names.contains(&"greet"),
        "should extract greet method, got: {entity_names:?}"
    );

    let call_relations: Vec<&str> = parsed
        .raw_relations
        .iter()
        .filter(|r| {
            matches!(
                r.relation_type,
                RelationType::DirectCall | RelationType::InstanceMethodCall
            )
        })
        .map(|r| r.dst_name.as_str())
        .collect();
    assert!(
        call_relations.contains(&"hello"),
        "should detect call to hello(), got: {call_relations:?}"
    );
}

#[test]
fn test_ruby_require_extracted() {
    let code = r#"
require 'json'
require_relative 'helpers'

class Parser
  def parse(data)
    JSON.parse(data)
  end
end
"#;
    let parsed = parse_source_with_lang(code, "src/parser.rb", Language::Ruby, "rb");

    let imports: Vec<String> = parsed
        .import_table
        .as_ref()
        .map(|it| {
            it.standardized_imports
                .iter()
                .map(|si| si.source.clone())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        imports.iter().any(|i| i == "json"),
        "should detect require 'json', got: {imports:?}"
    );
    assert!(
        imports.iter().any(|i| i == "helpers"),
        "should detect require_relative 'helpers', got: {imports:?}"
    );
}

#[test]
fn test_typescript_cross_file_import_resolves_to_local_symbol() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::TypeScript, "ts");
    let exported = coordinator
        .parse_with_language_info(
            "src/user.ts",
            "export function greet(): string { return 'hello'; }",
            &info,
        )
        .expect("exported TypeScript file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/main.ts",
            "import { greet } from './user'; function main() { return greet(); }",
            &info,
        )
        .expect("caller TypeScript file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/main.ts");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name == "greet")
        .expect("greet call should be indexed");

    assert!(!resolved.is_external);
    assert!(resolved.callee_id.is_some());
}

#[test]
fn test_jvm_cross_file_import_resolves_to_local_symbol() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::Java, "java");
    let exported = coordinator
        .parse_with_language_info(
            "src/demo/Helper.java",
            "package demo; public class Helper { public static void greet() {} }",
            &info,
        )
        .expect("exported Java file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/demo/Main.java",
            "package demo; import demo.Helper; class Main { void run() { Helper.greet(); } }",
            &info,
        )
        .expect("caller Java file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/demo/Main.java");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name.contains("greet"))
        .expect("greet call should be indexed");

    assert!(!resolved.is_external);
    assert!(resolved.callee_id.is_some());
}

#[test]
fn test_php_cross_file_import_resolves_to_local_symbol() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::Php, "php");
    let exported = coordinator
        .parse_with_language_info(
            "src/Helper.php",
            "<?php namespace App; class Helper { public static function greet() {} }",
            &info,
        )
        .expect("exported PHP file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/Main.php",
            "<?php namespace App; use App\\Helper; class Main { public function run() { Helper::greet(); } }",
            &info,
        )
        .expect("caller PHP file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/Main.php");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name.contains("greet"))
        .expect("greet call should be indexed");

    assert!(!resolved.is_external);
    assert!(resolved.callee_id.is_some());
}

#[test]
fn test_csharp_cross_file_import_resolves_to_local_symbol() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::CSharp, "cs");
    let exported = coordinator
        .parse_with_language_info(
            "src/Helper.cs",
            "namespace Demo { public static class Helper { public static void Greet() {} } }",
            &info,
        )
        .expect("exported C# file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/Main.cs",
            "using Demo; class Main { void Run() { Helper.Greet(); } }",
            &info,
        )
        .expect("caller C# file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/Main.cs");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name.contains("Greet"))
        .expect("Greet call should be indexed");

    assert!(!resolved.is_external);
    assert!(resolved.callee_id.is_some());
}

#[test]
fn test_kotlin_cross_file_import_resolves_to_local_symbol() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::Kotlin, "kt");
    let exported = coordinator
        .parse_with_language_info("src/demo/Helper.kt", "package demo\nfun greet() {}", &info)
        .expect("exported Kotlin file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/demo/Main.kt",
            "package demo\nimport demo.greet\nfun run() { greet() }",
            &info,
        )
        .expect("caller Kotlin file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/demo/Main.kt");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name == "greet")
        .expect("greet call should be indexed");

    assert!(!resolved.is_external);
    assert!(resolved.callee_id.is_some());
}

#[test]
fn test_ruby_cross_file_require_keeps_internal_call_resolvable() {
    let mut coordinator = ParseCoordinator::new();
    let info = lang_info_for(Language::Ruby, "rb");
    let exported = coordinator
        .parse_with_language_info(
            "src/helper.rb",
            "class Helper\n  def greet\n  end\nend",
            &info,
        )
        .expect("exported Ruby file should parse");
    let caller = coordinator
        .parse_with_language_info(
            "src/main.rb",
            "require_relative 'helper'\nclass Main\n  def run\n    Helper.new.greet\n  end\nend",
            &info,
        )
        .expect("caller Ruby file should parse");

    let builder = IndexBuilder::new();
    builder.add_parsed_files(&[&exported, &caller]);
    let relations = builder
        .index()
        .get_resolved_relations_by_file("src/main.rb");
    let resolved = relations
        .iter()
        .flat_map(|(_, file_relations)| file_relations)
        .find(|relation| relation.callee_name == "greet");

    assert!(resolved.is_some(), "greet call should be indexed");
    assert!(caller.import_table.as_ref().is_some_and(|table| {
        table
            .standardized_imports
            .iter()
            .any(|import| import.source == "helper" && import.is_relative)
    }));
}
