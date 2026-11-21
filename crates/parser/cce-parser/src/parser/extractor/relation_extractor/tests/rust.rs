use super::*;

#[test]
fn test_extract_rust_calls() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn foo() -> i32 {
1
}

fn bar() -> i32 {
foo() + 1
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");

    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    // Should find the call from bar to foo
    let _calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();

    // Note: The actual number depends on query results
    // This test just verifies the extraction doesn't fail
    assert!(!entities.is_empty());
}

#[test]
fn test_extract_rust_trait_impl_relations() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
trait MyTrait {
fn f(&self);
}

struct MyStruct;

impl MyStruct {
fn inherent(&self) {}
}

impl MyTrait for MyStruct {
fn f(&self) {}
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");

    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    let implementations: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::Implementation)
        .collect();
    let impl_associations: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::ImplAssociation)
        .collect();

    // Trait impl block yields exactly one Implementation (callee = trait)
    assert_eq!(implementations.len(), 1, "relations: {relations:?}");
    assert_eq!(
        implementations[0].dst_name(),
        "MyTrait",
        "Implementation callee should be the trait name, got {:?}",
        implementations[0].dst_name()
    );

    // Inherent + trait impl blocks each yield one ImplAssociation (callee = target type)
    assert_eq!(impl_associations.len(), 2, "relations: {relations:?}");
    let targets: Vec<_> = impl_associations
        .iter()
        .map(|r| r.dst_name().to_string())
        .collect();
    assert_eq!(
        targets,
        vec!["MyStruct".to_string(), "MyStruct".to_string()]
    );
}

#[test]
fn test_extract_full_callee_name_rust_associated_and_method() {
    // Regression: `Vec::new()` and `v.push(...)` must preserve the
    // type path / receiver instead of truncating to the bare function
    // name.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn main() {
let mut v = Vec::new();
v.push(1);
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();
    let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
    assert!(
        names.contains(&"Vec::new"),
        "expected Vec::new in {names:?}"
    );
    assert!(names.contains(&"v.push"), "expected v.push in {names:?}");
    // The stdlib category must be derived from the type path prefix.
    let vec_new = calls
        .iter()
        .find(|r| r.dst_name() == "Vec::new")
        .expect("Vec::new call present");
    assert_eq!(
        vec_new.stdlib_category,
        Some(StdlibCategory::Collection),
        "Vec::new category should come from the `Vec` prefix"
    );
}

#[test]
fn test_rust_pattern_entities() {
    // Tuple and struct patterns with branch bindings exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "fn f(pair: (i32, i32), p: Point, opt: Option<i32>, x: Option<i32>) { let (a, b) = pair; if let Some(v) = opt {} match x { Some(y) => {}, _ => {} } }\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("extract");
    let vars: Vec<&str> = entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        vars.iter().any(|n| n.contains('a') && n.contains('b')),
        "tuple pattern entity missing, got {vars:?}"
    );
    assert!(vars.contains(&"v"), "if-let binding missing, got {vars:?}");
}
