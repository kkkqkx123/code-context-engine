use super::*;

#[test]
fn test_resolver_with_filter() {
    let mut resolver = RelationResolver::new();
    resolver.with_filter(false);
    assert!(!resolver.filter_stdlib_calls);
}

#[test]
fn test_resolver_with_external_packages() {
    let mut resolver = RelationResolver::new();
    let mut packages = HashMap::new();
    packages.insert(Language::Rust, HashSet::from(["serde".to_string()]));
    resolver.with_external_packages(packages);
    assert!(resolver.external_packages.is_some());
}
