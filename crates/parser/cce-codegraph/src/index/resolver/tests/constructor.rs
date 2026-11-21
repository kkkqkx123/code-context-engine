use super::*;

#[test]
fn test_bare_call_to_struct_upgrades_to_constructor() {
    // `OnceCell(Imp::new())` inside `new()` parses as a bare-identifier
    // call, but the callee resolves to the struct: the edge must carry
    // construction semantics.
    let resolved = resolve_bare_call(
        "new",
        "OnceCell",
        vec![create_test_entity(0, "OnceCell", EntityKind::Struct)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.callee_name, "OnceCell");
    assert_eq!(resolved.relation_type, RelationType::ConstructorCall);
}

#[test]
fn test_bare_call_to_class_upgrades_to_constructor() {
    let resolved = resolve_bare_call(
        "make",
        "Widget",
        vec![create_test_entity(0, "Widget", EntityKind::Class)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::ConstructorCall);
}

#[test]
fn test_bare_call_to_function_keeps_direct_call() {
    let resolved = resolve_bare_call(
        "main",
        "helper",
        vec![create_test_entity(0, "helper", EntityKind::Function)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::DirectCall);
}

#[test]
fn test_bare_call_to_unresolved_keeps_direct_call() {
    let resolved = resolve_bare_call("main", "missing_type", vec![]);
    assert!(resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::DirectCall);
}

#[test]
fn test_resolver_new() {
    let resolver = RelationResolver::new();
    assert!(resolver.filter_stdlib_calls);
    assert_eq!(resolver.filtered_count(), 0);
}
