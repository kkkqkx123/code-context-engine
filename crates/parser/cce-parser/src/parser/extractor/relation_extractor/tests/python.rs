use super::*;

#[test]
fn test_extract_python_calls() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
def foo():
return 1

def bar():
return foo() + 1
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract entities");

    let _relations = relation_extractor
        .extract(&tree, code, &Language::Python, &entities, Some(1))
        .expect("Failed to extract relations");

    // This test just verifies the extraction doesn't fail
    assert!(!entities.is_empty());
}

#[test]
fn test_python_tuple_unpacking_entities() {
    // Tuple unpacking yields one comma-separated variable entity
    // carrying the right-hand side as its source.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "def f(pair):\n    first, second = pair\n    return first\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("extract");
    let unpacked = entities
        .iter()
        .find(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name.contains("first"))
        .expect("tuple unpacking entity should exist");
    assert_eq!(unpacked.name, "first, second");
    assert_eq!(
        unpacked.metadata.get("source_type").map(String::as_str),
        Some("pair")
    );
}

#[test]
fn test_python_pattern_binding_entities() {
    // Loop and exception bindings exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "for k, v in items:\n    pass\ntry:\n    pass\nexcept ValueError as e:\n    pass\nwith open('f') as fh:\n    pass\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("extract");
    let names: Vec<&str> = entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["k", "v", "e", "fh"] {
        assert!(
            names.contains(&expected),
            "pattern binding '{expected}' should exist, got {names:?}"
        );
    }
    let except_entity = entities
        .iter()
        .find(|e| e.name == "e")
        .expect("except binding should exist");
    assert_eq!(
        except_entity
            .metadata
            .get("source_type")
            .map(String::as_str),
        Some("ValueError")
    );

    let case_code = "def f(value):\n    match value:\n        case (x, 0):\n            return x\n        case (x, y):\n            return y\n";
    let case_tree = ast_parser
        .parse_with_tree(case_code, &Language::Python)
        .expect("parse")
        .0;
    let case_entities = entity_extractor
        .extract(&case_tree, case_code, &Language::Python)
        .expect("extract");
    let case_names: Vec<&str> = case_entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["x", "y"] {
        assert!(
            case_names.contains(&expected),
            "case binding '{expected}' should exist, got {case_names:?}"
        );
    }
}
