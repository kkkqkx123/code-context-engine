use super::*;

#[test]
fn test_string_argument_call_is_not_require_import() {
    // A call with a string literal argument must not produce
    // an ImportStandard edge to that literal.
    for language in [Language::JavaScript, Language::TypeScript] {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
function createUser(name, id) {
  return { kind: 'user', name, id };
}
const user = createUser('Alice', 1);
"#;

        let tree = ast_parser
            .parse_with_tree(code, &language)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &language)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &language, &entities, Some(1))
            .expect("Failed to extract relations");

        assert!(
            relations
                .iter()
                .filter(|r| r.relation_type == RelationType::ImportStandard)
                .all(|r| !r.dst_name().contains("Alice")),
            "string literal must not become an import edge in {language:?}: {relations:?}"
        );
        assert!(
            relations.iter().any(|r| r.dst_name() == "createUser"),
            "real call edge must be preserved in {language:?}: {relations:?}"
        );
    }
}

#[test]
fn test_real_require_import_still_detected() {
    // The predicate fix must not break genuine require() detection.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "const { loadUser } = require(\"./models\");\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
        .expect("Failed to extract relations");

    assert!(
        relations
            .iter()
            .any(|r| r.relation_type == RelationType::ImportStandard
                && r.dst_name().contains("models")),
        "genuine require() import must still be detected: {relations:?}"
    );
}

#[test]
fn test_assigned_require_yields_single_import_edge() {
    // The bare-call require pattern fires on the inner call_expression
    // regardless of its parent, so no declarator-level pattern may exist:
    // such a pattern would emit a second edge with a different span that
    // survives span-grouped dedup. `const x = require()` must yield
    // exactly one ImportStandard edge.
    for (language, code) in [
        (Language::JavaScript, "const x = require(\"./m\");\n"),
        (Language::JavaScript, "var y = require(\"./m\");\n"),
        (Language::TypeScript, "const x: any = require(\"./m\");\n"),
    ] {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let tree = ast_parser
            .parse_with_tree(code, &language)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &language)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &language, &entities, Some(1))
            .expect("Failed to extract relations");

        let imports: Vec<_> = relations
            .iter()
            .filter(|r| {
                r.relation_type == RelationType::ImportStandard && r.dst_name().contains("./m")
            })
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "assigned require() must yield exactly one import edge in {language:?}: {relations:?}"
        );
    }
}

#[test]
fn test_shadowed_require_param_yields_no_import() {
    // A shadowed builtin produces no import edge, while a top-level
    // call in the same file is still kept.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();
    let code = "function f(require) { return require(\"./x\"); }\nrequire(\"./y\");\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("extract");
    let relations = relation_extractor
        .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
        .expect("relations");
    let imports: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::ImportStandard)
        .collect();
    assert!(
        imports.iter().all(|r| !r.dst_name().contains("./x")),
        "shadowed require must not import: {relations:?}"
    );
    assert!(
        imports.iter().any(|r| r.dst_name().contains("./y")),
        "top-level require must still import: {relations:?}"
    );
}

#[test]
fn test_typescript_import_require_clause_detected() {
    // `import x = require("m")` parses as `import_require_clause`, not a
    // `call_expression`, so it needs its own dependency pattern.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "import x = require(\"./m\");\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::TypeScript)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::TypeScript)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::TypeScript, &entities, Some(1))
        .expect("Failed to extract relations");

    assert!(
        relations.iter().any(
            |r| r.relation_type == RelationType::ImportStandard && r.dst_name().contains("./m")
        ),
        "TS import-require must be detected as an import edge: {relations:?}"
    );
}
