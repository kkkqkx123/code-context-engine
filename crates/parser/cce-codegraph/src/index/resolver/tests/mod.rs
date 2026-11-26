// Relation resolver tests grouped by resolution concern.
use super::*;

use crate::index::builder::SymbolTableBuilder;

use cce_types::{Entity, EntityId, EntityKind, Language, RawRelationData, RelationType, Span};

use std::collections::HashMap;

use std::path::{Path, PathBuf};

/// Helper function to create a test entity of any kind
fn create_test_entity(id: u32, name: &str, kind: EntityKind) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind,
        name: name.to_string(),
        signature: name.to_string(),
        parameters: Vec::new(),
        return_type: None,
        span: Span::default(),
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

/// Build a one-file project: `caller_name()` contains a bare `dst_name()`
/// call recorded as `DirectCall`, and resolve it.
fn resolve_bare_call(
    caller_name: &str,
    dst_name: &str,
    callees: Vec<Entity>,
) -> cce_types::ResolvedRelation {
    let mut file = ParsedFile::new(Language::Rust, "lib.rs".to_string(), "");
    let caller_id = 100u32;
    file.add_entity(create_test_entity(
        caller_id,
        caller_name,
        EntityKind::Function,
    ));
    for callee in callees {
        file.add_entity(callee);
    }
    file.add_relation(RawRelationData {
        src: EntityId(caller_id.into()),
        level: cce_types::RelationLevel::Entity,
        dst_name: dst_name.to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        stdlib_category: None,
    });

    let files = [&file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolver = RelationResolver::new();
    resolver
        .resolve_batch(&file.raw_relations, &file, &symbols, &index)
        .into_iter()
        .next()
        .expect("bare call must produce a relation")
}

/// Helper function to create a test function entity
fn create_test_function_entity(id: u32, name: &str) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind: EntityKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        parameters: Vec::new(),
        return_type: None,
        span: Span::default(),
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

/// Build a three-file project: `caller.c` holds a file-level function
/// reference to `foo`, `def.c` defines it, and `other.c` holds a larger
/// same-name static definition with internal linkage.
fn resolve_funcref_across_files() -> (
    cce_types::ResolvedRelation,
    crate::index::core::RelationIndex,
) {
    let mut caller = ParsedFile::new(Language::C, "caller.c".to_string(), "");
    caller.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "foo".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::C, "def.c".to_string(), "");
    let mut def_entity = create_test_entity(2, "foo", EntityKind::Function);
    def_entity.span = Span::new(0, 100, 0, 0, 4, 0);
    def_file.add_entity(def_entity);
    let mut static_file = ParsedFile::new(Language::C, "other.c".to_string(), "");
    let mut static_entity = create_test_entity(3, "foo", EntityKind::Function);
    static_entity.span = Span::new(0, 900, 0, 0, 30, 0);
    static_entity.modifiers = vec!["static".to_string()];
    static_file.add_entity(static_entity);

    let files = [&caller, &def_file, &static_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&caller.raw_relations, &caller, &symbols, &index)
        .into_iter()
        .next()
        .expect("funcref must resolve to the visible definition");
    (resolved, index)
}

/// Two-file Go project: `Engine` embeds `RouterGroup`, which owns `Use`.
/// The caller file holds `setup` calling `engine.Use` on a variable typed
/// `*Engine`.
fn go_engine_project() -> (ParsedFile, ParsedFile) {
    let mut engine_file = ParsedFile::new(Language::Go, "engine.go".to_string(), "");
    let mut engine = create_test_entity(10, "Engine", EntityKind::Struct);
    engine.children = vec![EntityId(11)];
    engine_file.add_entity(engine);
    let mut embedded = create_test_entity(11, "RouterGroup", EntityKind::Field);
    embedded.parent = Some(EntityId(10));
    engine_file.add_entity(embedded);
    engine_file.add_entity(create_test_entity(12, "setup", EntityKind::Function));
    let mut engine_var = create_test_entity(13, "engine", EntityKind::Variable);
    engine_var.set_metadata("type_annotation", "*Engine");
    engine_file.add_entity(engine_var);
    engine_file.add_relation(RawRelationData {
        src: EntityId(12),
        level: cce_types::RelationLevel::Entity,
        dst_name: "engine.Use".to_string(),
        relation_type: RelationType::InstanceMethodCall,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut router_file = ParsedFile::new(Language::Go, "router.go".to_string(), "");
    router_file.add_entity(create_test_entity(20, "RouterGroup", EntityKind::Struct));
    let mut use_method = create_test_entity(21, "Use", EntityKind::Method);
    use_method.span = Span::new(0, 200, 0, 0, 6, 0);
    use_method.set_metadata("receiver_type", "*RouterGroup");
    router_file.add_entity(use_method);
    (engine_file, router_file)
}

fn build_go_engine_project() -> (
    ParsedFile,
    ProjectSymbolTable,
    crate::index::core::RelationIndex,
) {
    let (engine_file, router_file) = go_engine_project();
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    (engine_file, symbols, index)
}

/// Single-owner C++ project: class `S` owns one `bar`, referenced as
/// `S::bar` from another file.
fn cpp_single_owner_project() -> (ParsedFile, ParsedFile) {
    let mut user_file = ParsedFile::new(Language::Cpp, "user.cpp".to_string(), "");
    user_file.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "S::bar".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::Cpp, "s.cpp".to_string(), "");
    let mut class_s = create_test_entity(50, "S", EntityKind::Class);
    class_s.children = vec![EntityId(51)];
    def_file.add_entity(class_s);
    let mut bar = create_test_entity(51, "bar", EntityKind::Method);
    bar.parent = Some(EntityId(50));
    bar.span = Span::new(0, 120, 0, 0, 4, 0);
    def_file.add_entity(bar);
    (user_file, def_file)
}

fn build_cpp_project(
    user_file: &ParsedFile,
    def_file: &ParsedFile,
) -> (ProjectSymbolTable, crate::index::core::RelationIndex) {
    let files = [user_file, def_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    (symbols, builder.build())
}

/// Build a two-file JavaScript project: `app.js` holds a `handle` method
/// containing a `this.get` call, `response.js` holds a same-name `get`
/// method. When `with_local_get` is set, `app.js` also owns a `get` method.
/// Resolves the call end to end.
fn resolve_this_call_against_foreign_file(with_local_get: bool) -> cce_types::ResolvedRelation {
    // Mirrors the real extraction shape: `call.method` records
    // `this.get('env')` as bare `get` with a span covering only the
    // property name, the receiver surviving solely in the source text.
    let source = "var x = this.get('env');\n";
    let mut caller_file = ParsedFile::new(Language::JavaScript, "app.js".to_string(), source);
    caller_file.add_entity(create_test_entity(100, "handle", EntityKind::Method));
    if with_local_get {
        caller_file.add_entity(create_test_entity(101, "get", EntityKind::Method));
    }
    caller_file.add_relation(RawRelationData {
        src: EntityId(100u32.into()),
        level: cce_types::RelationLevel::Entity,
        dst_name: "get".to_string(),
        relation_type: RelationType::InstanceMethodCall,
        span: Span {
            start_byte: 13,
            end_byte: 16,
            ..Default::default()
        },
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::JavaScript, "response.js".to_string(), "");
    def_file.add_entity(create_test_entity(200, "get", EntityKind::Method));
    let files = [&caller_file, &def_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    resolver
        .resolve_batch(&caller_file.raw_relations, &caller_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("this.get must produce a relation")
}

#[test]
fn streamed_symbol_build_keeps_cross_file_resolution_equivalent() {
    let (engine_file, router_file) = go_engine_project();
    let files = [&engine_file, &router_file];
    let root = PathBuf::from(".");
    let expected = SymbolTableBuilder::new(root.clone()).build(&files);

    let builder = SymbolTableBuilder::new(root.clone());
    let streamed = cce_codegraph_symbol_table(&builder, &files, &root);
    for file in files {
        let module_path = crate::index::file_module_path(file, &root);
        let package_name = crate::index::project_package_name(&root);
        let type_index = crate::index::build_file_type_index(file, &module_path, &package_name);
        let type_ctx = crate::index::infer_file_type_context(file, &type_index);
        streamed.set_type_inference_context(&file.path, type_ctx);
        streamed.insert_cross_file_for_file(file);
        streamed.propagate_cross_file_variables_for_file(file);
        streamed.forget_type_inference_context(&file.path);
    }

    for file in files {
        let module_path = crate::index::file_module_path(file, &root);
        let package_name = crate::index::project_package_name(&root);
        let type_index = crate::index::build_file_type_index(file, &module_path, &package_name);
        let type_ctx = crate::index::infer_file_type_context(file, &type_index);
        streamed.set_type_inference_context(&file.path, type_ctx);
    }

    let index_builder = crate::index::builder::IndexBuilder::new();
    for file in files {
        index_builder.register_file_entities(file);
    }
    let index = index_builder.build();
    let resolver = RelationResolver::new();
    let expected_relation = resolver
        .resolve_batch(&engine_file.raw_relations, &engine_file, &expected, &index)
        .into_iter()
        .next()
        .expect("embedded call must produce a relation");
    let streamed_relation = resolver
        .resolve_batch(&engine_file.raw_relations, &engine_file, &streamed, &index)
        .into_iter()
        .next()
        .expect("streamed symbol build must produce a relation");
    assert_eq!(expected_relation.callee_id, streamed_relation.callee_id);
    assert_eq!(expected_relation.is_external, streamed_relation.is_external);
}

fn cce_codegraph_symbol_table(
    builder: &SymbolTableBuilder,
    files: &[&cce_types::ParsedFile],
    root: &Path,
) -> crate::symbol_table::ProjectSymbolTable {
    let project = crate::symbol_table::ProjectSymbolTable::new(root.to_path_buf());
    let mut session = builder.begin_streaming_build();
    for file in files {
        session
            .add_file(file, &project)
            .expect("test symbol build must persist local tables");
    }
    session.finish(&project);
    project
}

mod alias;
mod constructor;
mod dispatch;
mod embedded;
mod filter;
mod funcref;
mod metrics;
mod misc;
mod stdlib;
