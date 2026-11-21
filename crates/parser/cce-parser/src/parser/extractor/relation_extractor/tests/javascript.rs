use super::*;

#[test]
fn test_extract_full_callee_name_javascript_method() {
    // Regression: `console.log(...)` and `Math.max(...)` must keep
    // their receivers so stdlib detection sees the qualified name.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
function main() {
console.log("hello");
Math.max(1, 2);
}
"#;

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

    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();
    let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
    assert!(
        names.contains(&"console.log"),
        "expected console.log in {names:?}"
    );
    assert!(
        names.contains(&"Math.max"),
        "expected Math.max in {names:?}"
    );
    let console_log = calls
        .iter()
        .find(|r| r.dst_name() == "console.log")
        .expect("console.log call present");
    assert_eq!(
        console_log.stdlib_category,
        Some(StdlibCategory::Io),
        "console.log category should come from the `console` prefix"
    );
}

#[test]
fn javascript_hof_and_method_matches_collapse_to_one_edge() {
    // Regression: `app.use(function(){})` is matched by both `call.method`
    // (span = property identifier) and `call.hof` (span = whole member
    // expression). The call site must yield exactly one edge, keeping the
    // shorter property span so downstream span matching stays consistent.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "function main() {\n  app.use(function(){});\n}\n";
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

    let app_use: Vec<_> = relations
        .iter()
        .filter(|r| r.dst_name() == "app.use")
        .collect();
    assert_eq!(app_use.len(), 1, "expected one app.use edge");
    let text = &code[app_use[0].span.start_byte..app_use[0].span.end_byte];
    assert_eq!(text, "use", "callee span must cover the property name");
}
