use super::*;
use cce_types::Span;
use cce_types::entity::EntityKind;

fn dummy_span() -> Span {
    Span::default()
}

fn make_key(q: &str, s: &str, f: &str) -> TypeKey {
    TypeKey::new(q.to_string(), s.to_string(), f.to_string())
}

#[test]
fn insert_and_retrieve_type_and_member() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    let entry = TypeEntry::new(
        EntityId(1),
        key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx.insert_type(key.clone(), entry);
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx.insert_member(&key, member.clone());
    assert_eq!(idx.get_members("a::Foo", "bar").unwrap().len(), 1);
    assert_eq!(idx.owner_of(EntityId(2)).unwrap().qualified, "a::Foo");
}

#[test]
fn resolve_qualified_filters_private() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    let entry = TypeEntry::new(
        EntityId(1),
        key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx.insert_type(key.clone(), entry);
    let member_private = MemberEntry {
        entity_id: EntityId(2),
        name: "secret".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Private,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx.insert_member(&key, member_private);
    let from_other = ScopeContext::with_module("src/b.rs", "pkg", "b");
    assert!(
        idx.resolve_qualified("a::Foo", "secret", &from_other, Language::Rust)
            .is_none()
    );
    let from_same = ScopeContext::with_module("src/a.rs", "pkg", "a");
    assert!(
        idx.resolve_qualified("a::Foo", "secret", &from_same, Language::Rust)
            .is_some()
    );
}

#[test]
fn rust_trait_impl_member_placeholder() {
    let mut idx = TypeMemberIndex::new();
    let placeholder_key = make_key("a::Foo", "Foo", "src/b.rs");
    idx.upsert_type_placeholder(placeholder_key.clone(), Language::Rust);
    let member = MemberEntry {
        entity_id: EntityId(5),
        name: "trait_method".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/b.rs".to_string(),
        module_path: Some("b".to_string()),
        package: "pkg".to_string(),
    };
    idx.insert_member(&placeholder_key, member);
    assert!(idx.get_type("a::Foo").is_some());
    assert!(idx.get_type("a::Foo").unwrap().is_placeholder);
    // now complete with real type
    idx.complete_placeholder(
        &placeholder_key,
        EntityId(1),
        EntityKind::Struct,
        Visibility::Public,
    );
    assert!(!idx.get_type("a::Foo").unwrap().is_placeholder);
}

#[test]
fn duplicate_member_counted_without_metrics() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    let entry = TypeEntry::new(
        EntityId(1),
        key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx.insert_type(key.clone(), entry);
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx.insert_member(&key, member.clone());
    idx.insert_member(&key, member);
    // No panic: duplicate path is safe without metrics attached.
    assert_eq!(idx.get_members("a::Foo", "bar").unwrap().len(), 1);
}

#[test]
fn merge_from_counts_duplicates() {
    let mut idx_a = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    let entry = TypeEntry::new(
        EntityId(1),
        key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx_a.insert_type(key.clone(), entry);
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx_a.insert_member(&key, member);

    let mut idx_b = TypeMemberIndex::new();
    let entry_b = TypeEntry::new(
        EntityId(1),
        key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx_b.insert_type(key.clone(), entry_b);
    let member_b = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx_b.insert_member(&key, member_b);

    idx_a.merge_from(&idx_b);
    assert_eq!(idx_a.get_members("a::Foo", "bar").unwrap().len(), 1);
}

#[test]
fn resolve_qualified_prefers_type_member_over_simple_fallback() {
    let mut idx = TypeMemberIndex::new();
    let type_key = make_key("my_pkg::Foo", "Foo", "src/foo.rs");
    let type_entry = TypeEntry::new(
        EntityId(1),
        type_key.clone(),
        EntityKind::Struct,
        Language::Rust,
        Visibility::Public,
    );
    idx.insert_type(type_key.clone(), type_entry);
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/foo.rs".to_string(),
        module_path: Some("my_pkg".to_string()),
        package: "my_pkg".to_string(),
    };
    idx.insert_member(&type_key, member);

    // Also register a bare-name symbol that the simple-name fallback
    // would otherwise match.
    let bare_key = make_key("my_pkg::bar", "bar", "src/bar.rs");
    let bare_entry = TypeEntry::new(
        EntityId(3),
        bare_key.clone(),
        EntityKind::Function,
        Language::Rust,
        Visibility::Public,
    );
    idx.insert_type(bare_key, bare_entry);

    let from_scope = ScopeContext::with_module("src/caller.rs", "my_pkg", "caller");
    // Qualified call should resolve to the type member, not the bare symbol.
    let resolved = idx.resolve_qualified("Foo", "bar", &from_scope, Language::Rust);
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().entity_id, EntityId(2));
}

#[test]
fn merge_and_remove_file_contribution() {
    let mut idx1 = TypeMemberIndex::new();
    let key1 = make_key("a::Foo", "Foo", "src/a.rs");
    idx1.insert_type(
        key1.clone(),
        TypeEntry::new(
            EntityId(1),
            key1.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    idx1.insert_member(
        &key1,
        MemberEntry {
            entity_id: EntityId(2),
            name: "bar".to_string(),
            kind: EntityKind::Method,
            visibility: Visibility::Public,
            is_static: false,
            is_associated: false,
            span: dummy_span(),
            file_path: "src/a.rs".to_string(),
            module_path: Some("a".to_string()),
            package: "pkg".to_string(),
        },
    );

    let mut idx2 = TypeMemberIndex::new();
    let key2 = make_key("b::Bar", "Bar", "src/b.rs");
    idx2.insert_type(
        key2.clone(),
        TypeEntry::new(
            EntityId(10),
            key2.clone(),
            EntityKind::Class,
            Language::Python,
            Visibility::Public,
        ),
    );

    idx1.merge_from(&idx2);
    assert_eq!(idx1.len_types(), 2);
    assert!(idx1.get_type("b::Bar").is_some());

    idx1.remove_file_contribution("src/a.rs");
    assert!(idx1.get_type("a::Foo").is_none(), "Foo removed");
    assert!(idx1.get_type("b::Bar").is_some(), "Bar still present");
    assert!(
        idx1.owner_of(EntityId(2)).is_none(),
        "member of removed type gone"
    );
}

#[test]
fn insert_member_returns_duplicate_event() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    idx.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    // First insert: no duplicate
    let result = idx.insert_member(&key, member.clone());
    assert!(result.is_none());
    // Second insert: duplicate detected
    let result = idx.insert_member(&key, member);
    assert!(result.is_some());
    let dup = result.unwrap();
    assert_eq!(dup.member_name, "bar");
    assert_eq!(dup.duplicate_entity_id, EntityId(2));
}

#[test]
fn insert_member_returns_none_for_new() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    idx.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    let m1 = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    let m2 = MemberEntry {
        entity_id: EntityId(3),
        name: "baz".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    assert!(idx.insert_member(&key, m1).is_none());
    assert!(idx.insert_member(&key, m2).is_none());
}

#[test]
fn merge_from_returns_all_duplicates() {
    let mut idx_a = TypeMemberIndex::new();
    let key = make_key("a::Foo", "Foo", "src/a.rs");
    idx_a.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx_a.insert_member(&key, member);

    let mut idx_b = TypeMemberIndex::new();
    idx_b.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    let member_b = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("a".to_string()),
        package: "pkg".to_string(),
    };
    idx_b.insert_member(&key, member_b);

    let dups = idx_a.merge_from(&idx_b);
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].member_name, "bar");
}

#[test]
fn string_pool_builder_dedup() {
    let mut pool = StringPoolBuilder::new();
    let i0 = pool.intern("hello");
    let i1 = pool.intern("world");
    let i2 = pool.intern("hello");
    assert_eq!(i0, 0);
    assert_eq!(i1, 1);
    assert_eq!(i2, 0); // deduped
    assert_eq!(pool.len(), 2);
    let strings = pool.into_pool();
    assert_eq!(strings[0], "hello");
    assert_eq!(strings[1], "world");
}

#[test]
fn snapshot_roundtrip_preserves_data() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("my_pkg::Foo", "Foo", "src/foo.rs");
    idx.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    let member = MemberEntry {
        entity_id: EntityId(2),
        name: "bar".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/foo.rs".to_string(),
        module_path: Some("my_pkg".to_string()),
        package: "my_pkg".to_string(),
    };
    idx.insert_member(&key, member);

    let snapshot = idx.to_snapshot();
    let restored = TypeMemberIndex::from_snapshot(snapshot);

    assert_eq!(restored.len_types(), 1);
    let foo = restored.get_type("my_pkg::Foo").unwrap();
    assert_eq!(foo.entity_id, EntityId(1));
    assert_eq!(foo.kind, EntityKind::Struct);
    let members = restored.get_members("my_pkg::Foo", "bar").unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].entity_id, EntityId(2));
}

#[test]
fn snapshot_string_pool_dedup() {
    let mut idx = TypeMemberIndex::new();
    // Two types from the same file
    let key1 = make_key("pkg::Foo", "Foo", "src/lib.rs");
    let key2 = make_key("pkg::Bar", "Bar", "src/lib.rs");
    idx.insert_type(
        key1.clone(),
        TypeEntry::new(
            EntityId(1),
            key1,
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );
    idx.insert_type(
        key2.clone(),
        TypeEntry::new(
            EntityId(2),
            key2,
            EntityKind::Struct,
            Language::Rust,
            Visibility::Public,
        ),
    );

    let snapshot = idx.to_snapshot();
    // "src/lib.rs" should appear only once in the pool
    let lib_rs_count = snapshot
        .string_pool
        .iter()
        .filter(|s| s.as_str() == "src/lib.rs")
        .count();
    assert_eq!(lib_rs_count, 1, "file_path should be deduplicated");
}

#[test]
fn snapshot_preserves_lookup() {
    let mut idx = TypeMemberIndex::new();
    let key = make_key("pkg::Foo", "Foo", "src/a.rs");
    idx.insert_type(
        key.clone(),
        TypeEntry::new(
            EntityId(1),
            key.clone(),
            EntityKind::Class,
            Language::Python,
            Visibility::Public,
        ),
    );
    let member = MemberEntry {
        entity_id: EntityId(10),
        name: "method_a".to_string(),
        kind: EntityKind::Method,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: "src/a.rs".to_string(),
        module_path: Some("pkg".to_string()),
        package: "pkg".to_string(),
    };
    idx.insert_member(&key, member);

    let snapshot = idx.to_snapshot();
    let restored = TypeMemberIndex::from_snapshot(snapshot);

    // qualified lookup
    let foo = restored.get_type("pkg::Foo").unwrap();
    assert_eq!(foo.entity_id, EntityId(1));
    // member lookup
    let members = restored.get_members("pkg::Foo", "method_a").unwrap();
    assert_eq!(members[0].entity_id, EntityId(10));
    // simple name lookup
    let by_simple = restored.get_type_by_simple("Foo");
    assert_eq!(by_simple.len(), 1);
    assert_eq!(by_simple[0].entity_id, EntityId(1));
}

fn make_field(name: &str, id: u64, file: &str) -> MemberEntry {
    MemberEntry {
        entity_id: EntityId(id),
        name: name.to_string(),
        kind: EntityKind::Field,
        visibility: Visibility::Public,
        is_static: false,
        is_associated: false,
        span: dummy_span(),
        file_path: file.to_string(),
        module_path: None,
        package: String::new(),
    }
}

fn insert_class(
    idx: &mut TypeMemberIndex,
    qualified: &str,
    simple: &str,
    file: &str,
    id: u64,
    supertypes: &[&str],
    fields: &[(&str, u64)],
) {
    let key = make_key(qualified, simple, file);
    let mut entry = TypeEntry::new(
        EntityId(id),
        key.clone(),
        EntityKind::Class,
        Language::CSharp,
        Visibility::Public,
    );
    entry.supertypes = supertypes.iter().map(|s| s.to_string()).collect();
    idx.insert_type(key.clone(), entry);
    for (field_name, field_id) in fields {
        idx.insert_member(&key, make_field(field_name, *field_id, file));
    }
}

fn shape_hierarchy() -> TypeMemberIndex {
    let mut idx = TypeMemberIndex::new();
    insert_class(
        &mut idx,
        "app.Shape",
        "Shape",
        "app.cs",
        1,
        &[],
        &[("Kind", 11)],
    );
    insert_class(
        &mut idx,
        "app.Circle",
        "Circle",
        "app.cs",
        2,
        &["Shape"],
        &[("Kind", 21), ("Radius", 22)],
    );
    insert_class(
        &mut idx,
        "app.Rectangle",
        "Rectangle",
        "app.cs",
        3,
        &["Shape"],
        &[("Kind", 31)],
    );
    insert_class(
        &mut idx,
        "app.MiniCircle",
        "MiniCircle",
        "app.cs",
        4,
        &["Circle"],
        &[],
    );
    idx
}

#[test]
fn subclasses_of_direct_and_transitive() {
    let idx = shape_hierarchy();
    let subs = idx.subclasses_of("Shape", "app.Shape");
    let names: Vec<&str> = subs.iter().map(|e| e.key.simple.as_str()).collect();
    assert_eq!(names, vec!["Circle", "MiniCircle", "Rectangle"]);
}

#[test]
fn subclasses_of_ignores_placeholders() {
    let mut idx = shape_hierarchy();
    let ghost_key = make_key("app.Ghost", "Ghost", "app.cs");
    idx.upsert_type_placeholder(ghost_key, Language::CSharp);
    let subs = idx.subclasses_of("Shape", "app.Shape");
    assert_eq!(subs.len(), 3);
}

#[test]
fn visible_field_names_includes_inherited() {
    let idx = shape_hierarchy();
    let mini = idx
        .get_type("app.MiniCircle")
        .expect("MiniCircle must be indexed");
    let visible = idx.visible_field_names(mini);
    assert!(visible.contains("Kind"));
    assert!(visible.contains("Radius"));
    assert!(!visible.contains("Width"));
}

#[test]
fn hierarchy_walk_terminates_on_cycles() {
    let mut idx = TypeMemberIndex::new();
    insert_class(&mut idx, "p.A", "A", "p.cs", 1, &["B"], &[]);
    insert_class(&mut idx, "p.B", "B", "p.cs", 2, &["A"], &[]);
    let a = idx.get_type("p.A").expect("A must be indexed");
    assert!(idx.visible_field_names(a).is_empty());
    let subs = idx.subclasses_of("A", "p.A");
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].key.simple, "B");
}

#[test]
fn set_supertypes_by_entity() {
    let mut idx = TypeMemberIndex::new();
    insert_class(&mut idx, "p.Foo", "Foo", "p.cs", 7, &[], &[]);
    idx.set_supertypes_by_entity(EntityId(7), vec!["Bar".to_string()]);
    let foo = idx.get_type("p.Foo").expect("Foo must be indexed");
    assert_eq!(foo.supertypes, vec!["Bar".to_string()]);
    // Unknown entity id is a no-op, never a panic.
    idx.set_supertypes_by_entity(EntityId(999), vec!["Baz".to_string()]);
}

#[test]
fn snapshot_preserves_supertypes() {
    let idx = shape_hierarchy();
    let restored = TypeMemberIndex::from_snapshot(idx.to_snapshot());
    let circle = restored
        .get_type("app.Circle")
        .expect("Circle must survive snapshot round-trip");
    assert_eq!(circle.supertypes, vec!["Shape".to_string()]);
    let subs = restored.subclasses_of("Shape", "app.Shape");
    let names: Vec<&str> = subs.iter().map(|e| e.key.simple.as_str()).collect();
    assert_eq!(names, vec!["Circle", "MiniCircle", "Rectangle"]);
}
