use super::*;

#[test]
fn test_go_embedded_promotion_rewrites_candidate() {
    // The two-segment call must gain an owner-qualified candidate before the
    // raw literal, so type-member resolution reaches the embedded owner.
    let (engine_file, symbols, index) = build_go_engine_project();
    let resolver = RelationResolver::new();
    let names = resolver.ordered_resolution_names(
        "engine.Use",
        &engine_file,
        &symbols,
        &index,
        Some("*Engine"),
    );
    assert_eq!(
        names.first().map(String::as_str),
        Some("RouterGroup.Use"),
        "embedded owner must lead the candidates, got {names:?}"
    );
}

#[test]
fn test_go_embedded_call_resolves_to_owner() {
    // End to end: `engine.Use` on an `*Engine` receiver resolves to the
    // `RouterGroup.Use` method instead of degrading to Unknown.
    let (engine_file, symbols, index) = build_go_engine_project();
    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&engine_file.raw_relations, &engine_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("embedded call must produce a relation");
    assert!(
        !resolved.is_external,
        "embedded call must resolve internally, got {:?}",
        resolved.external_type
    );
    assert_eq!(resolved.callee_name, "Use");
    let callee = resolved.callee_id.expect("must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("router.go"),
        "call must land on the embedded owner, got {file}"
    );
}

#[test]
fn test_embedded_walk_stops_when_outer_owns_method() {
    // Promotion only fills the gap the receiver's own type leaves: when the
    // outer type owns the method, no embedded candidate may be produced.
    let (mut engine_file, router_file) = go_engine_project();
    let mut own_use = create_test_entity(14, "Use", EntityKind::Method);
    own_use.set_metadata("receiver_type", "*Engine");
    engine_file.add_entity(own_use);
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*Engine", "Use", &index, &symbols),
        None,
        "outer owner must suppress promotion"
    );
}

#[test]
fn test_embedded_walk_ignores_typed_named_fields() {
    // A named field carrying a type annotation is not an anonymous embedded
    // field and must never open a promotion path, even when its type owns
    // the called method.
    let (mut engine_file, router_file) = go_engine_project();
    for entity in engine_file.entities.iter_mut() {
        if entity.name == "RouterGroup" && entity.kind == EntityKind::Field {
            entity.name = "group".to_string();
            entity.set_metadata("type_annotation", "RouterGroup");
        }
    }
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*Engine", "Use", &index, &symbols),
        None,
        "named fields must not promote"
    );
}

#[test]
fn test_embedded_walk_terminates_on_cycle() {
    // Mutual embedding with no owner must return empty instead of looping.
    let mut a_file = ParsedFile::new(Language::Go, "a.go".to_string(), "");
    let mut struct_a = create_test_entity(30, "A", EntityKind::Struct);
    struct_a.children = vec![EntityId(31)];
    a_file.add_entity(struct_a);
    let mut field_b = create_test_entity(31, "B", EntityKind::Field);
    field_b.parent = Some(EntityId(30));
    a_file.add_entity(field_b);
    let mut b_file = ParsedFile::new(Language::Go, "b.go".to_string(), "");
    let mut struct_b = create_test_entity(40, "B", EntityKind::Struct);
    struct_b.children = vec![EntityId(41)];
    b_file.add_entity(struct_b);
    let mut field_a = create_test_entity(41, "A", EntityKind::Field);
    field_a.parent = Some(EntityId(40));
    b_file.add_entity(field_a);
    let files = [&a_file, &b_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*A", "Ping", &index, &symbols),
        None,
        "cyclic embedding must terminate empty"
    );
}

#[test]
fn test_self_receiver_does_not_hijack_foreign_same_name() {
    // `this.get(...)` with no local `get` must not bind another file's
    // `get`: a self-rooted receiver names the caller's own object graph,
    // and the foreign same-name method is a guess, not evidence.
    let resolved = resolve_this_call_against_foreign_file(false);
    assert!(
        resolved.is_external,
        "foreign same-name method must not hijack a self-receiver call, got {:?}",
        resolved.callee_name
    );
}

#[test]
fn test_self_receiver_prefers_caller_file_candidate() {
    // The guard only removes the cross-file guess: a same-file `get` still
    // wins for `this.get`.
    let resolved = resolve_this_call_against_foreign_file(true);
    assert!(
        !resolved.is_external,
        "caller-file candidate must still resolve, got {:?}",
        resolved.callee_name
    );
    assert_eq!(resolved.callee_name, "get");
}
