use super::*;

#[test]
fn test_funcref_ignores_larger_foreign_static() {
    // The foreign static has internal linkage and can never be the target
    // of another file's reference, even though its body is larger than the
    // visible definition's.
    let (resolved, index) = resolve_funcref_across_files();
    let callee = resolved
        .callee_id
        .expect("funcref must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("def.c"),
        "foreign static must not win over the visible definition, got {file}"
    );
}

/// Caller file holds its own small static `foo` plus a file-level reference;
/// a larger extern `foo` lives elsewhere. Same-file linkage wins over span.
#[test]
fn test_funcref_prefers_same_file_static_over_larger_foreign() {
    let mut caller = ParsedFile::new(Language::C, "caller.c".to_string(), "");
    caller.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "foo".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut own_entity = create_test_entity(1, "foo", EntityKind::Function);
    own_entity.span = Span::new(0, 50, 0, 0, 2, 0);
    own_entity.modifiers = vec!["static".to_string()];
    caller.add_entity(own_entity);
    let mut foreign_file = ParsedFile::new(Language::C, "def.c".to_string(), "");
    let mut foreign_entity = create_test_entity(2, "foo", EntityKind::Function);
    foreign_entity.span = Span::new(0, 900, 0, 0, 30, 0);
    foreign_file.add_entity(foreign_entity);

    let files = [&caller, &foreign_file];
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
        .expect("funcref must resolve to the same-file definition");
    let callee = resolved
        .callee_id
        .expect("funcref must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("caller.c"),
        "same-file static must win over the larger foreign definition, got {file}"
    );
}

#[test]
fn test_scoped_funcref_resolves_single_owner() {
    // Exactly one candidate owned by the named scope resolves.
    let (user_file, def_file) = cpp_single_owner_project();
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    let callee = resolver
        .resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols)
        .expect("single owner must resolve");
    let entity = index
        .get_function_by_entity_id(callee)
        .expect("callee must exist");
    assert_eq!(entity.name, "bar");
}

#[test]
fn test_scoped_funcref_resolves_end_to_end() {
    // The file-level member-pointer reference lands on the member method.
    let (user_file, def_file) = cpp_single_owner_project();
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&user_file.raw_relations, &user_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("scoped funcref must produce a relation");
    assert!(
        !resolved.is_external,
        "scoped funcref must resolve internally"
    );
    assert_eq!(resolved.callee_name, "bar");
}

#[test]
fn test_scoped_funcref_drops_overload_ambiguity() {
    // Two same-owner overloads cannot be told apart without arity: drop.
    let (user_file, mut def_file) = cpp_single_owner_project();
    let mut second = create_test_entity(52, "bar", EntityKind::Method);
    second.parent = Some(EntityId(50));
    second.span = Span::new(200, 320, 0, 6, 10, 0);
    def_file.add_entity(second);
    if let Some(class_s) = def_file.entities.iter_mut().find(|e| e.name == "S") {
        class_s.children.push(EntityId(52));
    }
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols),
        None,
        "overload ambiguity must be dropped"
    );
}

#[test]
fn test_scoped_funcref_drops_wrong_scope() {
    // A member of another scope is never the target of this reference.
    let (user_file, mut def_file) = cpp_single_owner_project();
    for entity in def_file.entities.iter_mut() {
        if entity.name == "S" {
            entity.name = "Other".to_string();
        }
    }
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols),
        None,
        "foreign scope must not resolve"
    );
}
