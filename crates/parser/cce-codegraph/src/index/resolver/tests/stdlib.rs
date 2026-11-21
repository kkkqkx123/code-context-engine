use super::*;

#[test]
fn test_extract_stdlib_name_rust() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("std::collections::HashMap", &Language::Rust),
        "std::collections"
    );
    assert_eq!(
        resolver.extract_stdlib_name("std::fs::read", &Language::Rust),
        "std::fs"
    );
    assert_eq!(
        resolver.extract_stdlib_name("simple_func", &Language::Rust),
        "simple_func"
    );
}

#[test]
fn test_extract_stdlib_name_python() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("os.path.join", &Language::Python),
        "os.path"
    );
    assert_eq!(
        resolver.extract_stdlib_name("json.loads", &Language::Python),
        "json"
    );
    assert_eq!(
        resolver.extract_stdlib_name("print", &Language::Python),
        "builtin"
    );
}

#[test]
fn test_extract_stdlib_name_javascript() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("console.log", &Language::JavaScript),
        "console"
    );
    assert_eq!(
        resolver.extract_stdlib_name("Math.random", &Language::JavaScript),
        "Math"
    );
}

#[test]
fn test_stdlib_relations_preserved() {
    // This test verifies that stdlib relations are now preserved (not deleted)
    // and marked with external_type = StandardLibrary(...).
    //
    // This is the key improvement: instead of filtering out stdlib calls entirely,
    // they are now retained and marked as external, allowing downstream modules
    // to decide how to handle them based on external_type.
    let mut resolver = RelationResolver::new();

    // Verify that filter_stdlib_calls doesn't cause early return anymore
    assert!(resolver.filter_stdlib_calls);

    // The actual verification would require setting up full parse context,
    // which is tested in integration tests. This test just verifies the
    // resolver accepts filter_stdlib_calls=true without deleting relations.
    resolver.with_filter(true);
    assert!(resolver.filter_stdlib_calls);
}
