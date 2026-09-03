//! Integration tests for the relation module
//!
//! Tests the full relation processing pipeline:
//! - IndexBuilder pipeline (FileInfo/ParsedFile → RelationIndex)
//! - Cross-file call resolution and symbol table building
//! - Multi-language symbol resolution
//! - Complex call chain scenarios
//! - TypeMemberIndex delta operations

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile, RawRelationData};
use cce_types::language::Language;
use cce_types::relation::{CallContext, ExternalCallType, RelationType, ResolvedRelation};

use cce_relation::{
    index::{
        EntityIndexOps, ExportIndexOps, FileIndexOps, ImportIndexOps, RelationQueryOps,
        builder::IndexBuilder,
        core::{ExportInfo, ExportType, RelationIndex},
    },
    query::{CallChainTraverser, TraversalConfig},
    type_inference::InferenceContext,
};

use cce_relation::index::builder::SymbolTableBuilder;
use cce_relation::symbol_table::ResolutionContext;
use cce_relation::symbol_table::type_index::{TypeKey, TypeMemberIndex};

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
    }
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

fn make_rust_entity(
    id: u64,
    kind: EntityKind,
    name: &str,
    parent: Option<EntityId>,
    modifiers: Vec<String>,
    params: Vec<(String, Option<String>)>,
    meta: HashMap<String, String>,
) -> Entity {
    Entity {
        id: EntityId(id),
        kind,
        name: name.to_string(),
        parent,
        parameters: params,
        modifiers,
        metadata: meta,
        ..Default::default()
    }
}

// ============================================================
// 1. IndexBuilder Pipeline (FileInfo / ParsedFile → RelationIndex)
// ============================================================

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

    assert!(index.contains_file("test.rs"));
    assert_eq!(index.function_count(), 2);
    assert_eq!(index.call_count(), 1);
}

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

    builder.add_parsed_file(&parsed);

    let index = builder.build();

    assert!(index.contains_function(EntityId(0)));
    assert!(index.contains_function(EntityId(1)));

    let relations = index.get_resolved_relations_by_caller(EntityId(0));
    assert!(relations.is_some());
    assert_eq!(relations.unwrap().len(), 1);
}

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
// 2. Complex Call Chain Scenarios
// ============================================================

#[test]
fn test_complex_call_chain_with_branches() {
    let index = RelationIndex::new();

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

    let callers = index.get_callers_by_callee_entity(EntityId(4));
    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&EntityId(2)));
    assert!(callers.contains(&EntityId(3)));

    let to_baz = index.get_relations_to_entity(EntityId(4));
    assert_eq!(to_baz.len(), 2);
}

// ============================================================
// 3. Cross-File Symbol Table Resolution
// ============================================================

#[test]
fn test_rust_struct_impl_cross_file_type_member_resolution() {
    let rust_type = make_rust_entity(
        1,
        EntityKind::Struct,
        "Foo",
        None,
        vec!["pub".to_string()],
        vec![],
        HashMap::new(),
    );
    let mut impl_meta = HashMap::new();
    impl_meta.insert("impl_for_type".to_string(), "Foo".to_string());
    let rust_impl = make_rust_entity(
        2,
        EntityKind::InherentImpl,
        "Foo",
        None,
        vec![],
        vec![],
        impl_meta,
    );
    let rust_method = make_rust_entity(
        3,
        EntityKind::Function,
        "bar",
        Some(EntityId(2)),
        vec!["pub".to_string()],
        vec![("self".to_string(), None)],
        HashMap::new(),
    );
    let rust_secret = make_rust_entity(
        4,
        EntityKind::Function,
        "secret",
        Some(EntityId(2)),
        vec![],
        vec![("self".to_string(), None)],
        HashMap::new(),
    );

    let file_a = make_parsed_file(Language::Rust, "src/a.rs", "", vec![rust_type], vec![]);
    let file_b = make_parsed_file(
        Language::Rust,
        "src/b.rs",
        "",
        vec![rust_impl, rust_method, rust_secret],
        vec![],
    );

    let builder = SymbolTableBuilder::new(PathBuf::from("/"));
    let project = builder.build(&[&file_a, &file_b]);

    let g = project.global_type_index();
    assert!(g.len_types() > 0, "should have type entries");

    let from_other =
        cce_relation::symbol::ScopeContext::with_module("src/other.rs", "default", "other");
    let from_same = cce_relation::symbol::ScopeContext::with_module("src/a.rs", "default", "a");

    let hit = g.resolve_qualified("a::Foo", "bar", &from_other, Language::Rust);
    assert!(hit.is_some(), "pub method should resolve from other file");
    assert_eq!(hit.unwrap().name, "bar");

    let secret_same = g.resolve_qualified("a::Foo", "secret", &from_same, Language::Rust);
    assert!(
        secret_same.is_some(),
        "private method should resolve from same module"
    );
    let secret_other = g.resolve_qualified("a::Foo", "secret", &from_other, Language::Rust);
    assert!(
        secret_other.is_none(),
        "private method should NOT resolve from other module"
    );

    let ctx = ResolutionContext {
        file_path: "src/other.rs".to_string(),
        module_path: vec![],
        scope_chain: vec![],
    };
    let resolved = project.resolve_enhanced("a::Foo::bar", &ctx);
    assert!(resolved.is_some(), "resolve_enhanced should find Foo::bar");
    assert_eq!(resolved.unwrap().name(), "bar");

    let resolved_simple = project.resolve_enhanced("Foo::bar", &ctx);
    assert!(
        resolved_simple.is_some(),
        "resolve_enhanced with simple name should find Foo::bar"
    );
}

#[test]
fn test_python_class_static_method_resolution() {
    let class = make_rust_entity(
        10,
        EntityKind::Class,
        "Foo",
        None,
        vec![],
        vec![],
        HashMap::new(),
    );
    let method = make_rust_entity(
        11,
        EntityKind::Method,
        "m",
        Some(EntityId(10)),
        vec!["staticmethod".to_string()],
        vec![],
        HashMap::new(),
    );
    let file_py = make_parsed_file(
        Language::Python,
        "myapp/mod.py",
        "",
        vec![class, method],
        vec![],
    );

    let builder = SymbolTableBuilder::new(PathBuf::from("/"));
    let project = builder.build(&[&file_py]);

    let g = project.global_type_index();
    assert_eq!(g.len_types(), 1);

    let ctx = ResolutionContext {
        file_path: "other.py".to_string(),
        module_path: vec![],
        scope_chain: vec![],
    };
    let r = project.resolve_enhanced("myapp.mod.Foo.m", &ctx);
    assert!(r.is_some(), "Python static method should resolve");
    assert_eq!(r.unwrap().name(), "m");
}

#[test]
fn test_go_receiver_type_member_resolution() {
    let s = make_rust_entity(
        20,
        EntityKind::Struct,
        "S",
        None,
        vec![],
        vec![],
        HashMap::new(),
    );
    let mut m1 = make_rust_entity(
        21,
        EntityKind::Method,
        "M",
        None,
        vec![],
        vec![],
        HashMap::new(),
    );
    m1.signature = "func (s S) M()".to_string();
    m1.metadata
        .insert("receiver_type".to_string(), "S".to_string());
    let mut m2 = make_rust_entity(
        22,
        EntityKind::Method,
        "N",
        None,
        vec![],
        vec![],
        HashMap::new(),
    );
    m2.signature = "func (s *S) N()".to_string();
    m2.metadata
        .insert("receiver_type".to_string(), "S".to_string());

    let file_go = make_parsed_file(Language::Go, "pkg/a.go", "", vec![s, m1, m2], vec![]);
    let builder = SymbolTableBuilder::new(PathBuf::from("/"));
    let project = builder.build(&[&file_go]);

    let g = project.global_type_index();
    assert!(g.len_types() >= 1, "Go type should be indexed");

    let ctx = ResolutionContext {
        file_path: "pkg/b.go".to_string(),
        module_path: vec![],
        scope_chain: vec![],
    };
    let r = project.resolve_enhanced("pkg.S.M", &ctx);
    assert!(r.is_some(), "Go receiver method M should resolve");
    assert_eq!(r.unwrap().name(), "M");

    let r2 = project.resolve_enhanced("S.M", &ctx);
    assert!(
        r2.is_some(),
        "Go simple-name receiver method should resolve"
    );
}

#[test]
fn test_typescript_class_member_resolution() {
    let class = make_rust_entity(
        30,
        EntityKind::Class,
        "C",
        None,
        vec![],
        vec![],
        HashMap::new(),
    );
    let method = make_rust_entity(
        31,
        EntityKind::Method,
        "m",
        Some(EntityId(30)),
        vec!["public".to_string()],
        vec![],
        HashMap::new(),
    );
    let file_ts = make_parsed_file(
        Language::TypeScript,
        "src/a.ts",
        "",
        vec![class, method],
        vec![],
    );
    let builder = SymbolTableBuilder::new(PathBuf::from("/"));
    let project = builder.build(&[&file_ts]);

    let g = project.global_type_index();
    assert!(g.len_types() >= 1, "TS type should be indexed");

    let ctx = ResolutionContext {
        file_path: "src/b.ts".to_string(),
        module_path: vec![],
        scope_chain: vec![],
    };
    let r = project.resolve_enhanced("src/a.C.m", &ctx);
    assert!(r.is_some(), "TS class method should resolve");
}

// ============================================================
// 4. TypeMemberIndex Delta Operations
// ============================================================

#[test]
fn test_apply_type_delta_for_file_increments_correctly() {
    let file_a = make_parsed_file(
        Language::Rust,
        "src/a.rs",
        "",
        vec![make_rust_entity(
            1,
            EntityKind::Struct,
            "Foo",
            None,
            vec!["pub".to_string()],
            vec![],
            HashMap::new(),
        )],
        vec![],
    );
    let file_b = make_parsed_file(
        Language::Rust,
        "src/b.rs",
        "",
        vec![make_rust_entity(
            10,
            EntityKind::Struct,
            "Bar",
            None,
            vec!["pub".to_string()],
            vec![],
            HashMap::new(),
        )],
        vec![],
    );

    let builder = SymbolTableBuilder::new(PathBuf::from("/"));
    let project = builder.build(&[&file_a, &file_b]);

    {
        let g = project.global_type_index();
        assert_eq!(g.len_types(), 2, "both types should be present");
    }

    let mut new_type_index = TypeMemberIndex::new();
    let key = TypeKey::new(
        "a::Baz".to_string(),
        "Baz".to_string(),
        "src/a.rs".to_string(),
    );
    new_type_index.insert_type(
        key.clone(),
        cce_relation::symbol_table::type_index::TypeEntry::new(
            EntityId(2),
            key,
            EntityKind::Enum,
            Language::Rust,
            cce_relation::symbol::Visibility::Public,
        ),
    );
    project.apply_type_delta_for_file("src/a.rs", &new_type_index);

    {
        let g = project.global_type_index();
        assert!(
            g.get_type("a::Foo").is_none(),
            "Foo should be removed after delta"
        );
        assert!(
            g.get_type("a::Baz").is_some(),
            "Baz should be present after delta"
        );
        assert!(
            g.get_type("b::Bar").is_some(),
            "Bar should still be present"
        );
    }
}

// ============================================================
// Type Inference Integration Tests
// ============================================================

#[cfg(test)]
mod type_inference_tests {
    use super::*;
    use cce_relation::type_inference::TypeInferenceEngine;

    fn make_variable_entity(id: u64, name: &str, metadata: HashMap<String, String>) -> Entity {
        let mut entity = create_entity(id, EntityKind::Variable, name, 0, 1);
        entity.metadata = metadata;
        entity
    }

    fn make_function_entity(
        id: u64,
        name: &str,
        return_type: Option<String>,
        params: Vec<(String, Option<String>)>,
    ) -> Entity {
        Entity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            return_type,
            parameters: params,
            ..Default::default()
        }
    }

    #[test]
    fn test_type_inference_reads_constructor_type() {
        let mut metadata = HashMap::new();
        metadata.insert("constructor_type".to_string(), "MyClass".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(
            Language::Python,
            "test.py",
            "x = MyClass()",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
        assert!(
            cce_relation::type_inference::origin_priority(binding.origin) <= 4
                && cce_relation::type_inference::origin_priority(binding.origin) > 0
        );
    }

    #[test]
    fn test_type_inference_reads_literal_type() {
        let mut metadata = HashMap::new();
        metadata.insert("literal_type".to_string(), "number".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(Language::Python, "test.py", "x = 42", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "number");
        assert!(
            cce_relation::type_inference::origin_priority(binding.origin) <= 4
                && cce_relation::type_inference::origin_priority(binding.origin) > 0
        );
    }

    #[test]
    fn test_type_inference_reads_type_annotation() {
        let mut metadata = HashMap::new();
        metadata.insert("type_annotation".to_string(), "int".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(Language::Python, "test.py", "x: int = 5", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_function_return_type() {
        let entities = vec![make_function_entity(
            1,
            "get_name",
            Some("String".to_string()),
            vec![],
        )];

        let file = make_parsed_file(
            Language::Python,
            "test.py",
            "def get_name() -> str: ...",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_return_type(EntityId(1)).unwrap();
        assert_eq!(binding.type_name, "String");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_function_parameter_types() {
        let entities = vec![make_function_entity(
            1,
            "add",
            Some("int".to_string()),
            vec![
                ("a".to_string(), Some("int".to_string())),
                ("b".to_string(), Some("int".to_string())),
            ],
        )];

        let file = make_parsed_file(
            Language::Python,
            "test.py",
            "def add(a: int, b: int) -> int: ...",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let params = ctx.get_parameter_types(EntityId(1)).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_name, "int");
        assert_eq!(params[1].type_name, "int");
    }

    #[test]
    fn test_type_inference_go_var_type_annotation() {
        let mut metadata = HashMap::new();
        metadata.insert("type_annotation".to_string(), "string".to_string());

        let entities = vec![make_variable_entity(1, "name", metadata)];

        let file = make_parsed_file(
            Language::Go,
            "test.go",
            "var name string = \"hello\"",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "string");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_go_inferred_type() {
        let mut metadata = HashMap::new();
        metadata.insert("inferred_type".to_string(), "int".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(Language::Go, "test.go", "x := 42", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert!(
            cce_relation::type_inference::origin_priority(binding.origin) <= 4
                && cce_relation::type_inference::origin_priority(binding.origin) > 0
        );
    }

    #[test]
    fn test_type_inference_java_var_type() {
        let mut metadata = HashMap::new();
        metadata.insert("var_type".to_string(), "String".to_string());

        let entities = vec![make_variable_entity(1, "name", metadata)];

        let file = make_parsed_file(
            Language::Java,
            "Test.java",
            "var name = \"hello\";",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("name").unwrap();
        assert_eq!(binding.type_name, "String");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_csharp_var_type() {
        let mut metadata = HashMap::new();
        metadata.insert("var_type".to_string(), "int".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(Language::CSharp, "Test.cs", "var x = 42;", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_csharp_explicit_type() {
        let mut metadata = HashMap::new();
        metadata.insert("explicit_type".to_string(), "int".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(Language::CSharp, "Test.cs", "int x = 42;", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "int");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_rust_impl_self_type() {
        let mut impl_entity = create_entity(10, EntityKind::InherentImpl, "impl_MyStruct", 0, 5);
        impl_entity
            .metadata
            .insert("self_type".to_string(), "MyStruct".to_string());

        let mut method_entity = create_entity(20, EntityKind::Method, "get_value", 1, 3);
        method_entity.parent = Some(EntityId(10));
        method_entity.return_type = Some("i32".to_string());

        let entities = vec![impl_entity, method_entity];

        let file = make_parsed_file(
            Language::Rust,
            "test.rs",
            "impl MyStruct { fn get_value() -> i32 { 42 } }",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("Self").unwrap();
        assert_eq!(binding.type_name, "MyStruct");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_go_receiver_type() {
        let mut method_entity = create_entity(1, EntityKind::Method, "GetValue", 0, 3);
        method_entity.signature = "func (s *MyStruct) GetValue() int".to_string();
        method_entity
            .metadata
            .insert("receiver_type".to_string(), "*MyStruct".to_string());

        let entities = vec![method_entity];

        let file = make_parsed_file(
            Language::Go,
            "test.go",
            "func (s *MyStruct) GetValue() int { return 0 }",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("s").unwrap();
        assert_eq!(binding.type_name, "*MyStruct");
        assert!(cce_relation::type_inference::origin_priority(binding.origin) >= 5);
    }

    #[test]
    fn test_type_inference_constructor_priority_over_literal() {
        // When both constructor_type and literal_type are present,
        // constructor_type should take precedence (it's set first in the extractor)
        let mut metadata = HashMap::new();
        metadata.insert("constructor_type".to_string(), "MyClass".to_string());
        metadata.insert("literal_type".to_string(), "object".to_string());

        let entities = vec![make_variable_entity(1, "x", metadata)];

        let file = make_parsed_file(
            Language::Python,
            "test.py",
            "x = MyClass()",
            entities,
            vec![],
        );

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "MyClass");
    }

    #[test]
    fn test_type_inference_empty_context() {
        let entities = vec![];
        let file = make_parsed_file(Language::Python, "test.py", "", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_type_inference_unknown_language() {
        let entities = vec![make_variable_entity(1, "x", HashMap::new())];

        let file = make_parsed_file(Language::Unknown, "test.txt", "", entities, vec![]);

        let ctx = TypeInferenceEngine::infer_types(&file, &InferenceContext::new());
        // Unknown language should produce empty context (no inferer)
        assert!(ctx.is_empty());
    }
}
