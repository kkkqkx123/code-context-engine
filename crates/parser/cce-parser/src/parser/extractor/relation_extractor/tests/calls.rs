use super::*;

#[test]
fn test_extract_full_scoped_type_reference() {
    // Regression: a type-position reference like
    // `std::collections::HashMap` must keep the full scoped path instead
    // of truncating to the path segment `std::collections`.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn main() {
let map: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
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

    let refs: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::TypeReference)
        .collect();
    assert!(
        refs.iter()
            .any(|r| r.dst_name() == "std::collections::HashMap"),
        "expected full scoped type reference, got {refs:?}"
    );
}

#[test]
fn test_extract_c_stdlib_category() {
    // Newly categorized languages must flow through the same classifier:
    // `printf` resolves to Io instead of the uncategorized bucket.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "#include <stdio.h>\nint main() { printf(\"hi\"); return 0; }\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::C)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::C)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::C, &entities, Some(1))
        .expect("Failed to extract relations");

    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();
    let printf = calls
        .iter()
        .find(|r| r.dst_name() == "printf")
        .expect("printf call present");
    assert_eq!(
        printf.stdlib_category,
        Some(StdlibCategory::Io),
        "printf category should be Io, not uncategorized"
    );
}

#[test]
fn test_build_full_callee_name_trivial_receiver_falls_back() {
    // Regression: `this.method()` inside a JS method must still
    // produce `method` (the receiver is trivial) so local resolution
    // keeps working.
    let mat = QueryMatch {
        index: 0,
        pattern_index: 0,
        captures: vec![
            Capture {
                name: "call.method.object".to_string(),
                text: "this".to_string(),
                start_byte: 0,
                end_byte: 4,
                start_point: (0, 0),
                end_point: (0, 4),
            },
            Capture {
                name: "call.method.function".to_string(),
                text: "method".to_string(),
                start_byte: 5,
                end_byte: 11,
                start_point: (0, 5),
                end_point: (0, 11),
            },
        ],
    };
    // Create a minimal tree for AST-based name extraction.
    let mut ast_parser = AstParser::new();
    let source = "this.method()";
    let (tree, _) = ast_parser
        .parse_with_tree(source, &Language::JavaScript)
        .expect("Failed to parse");
    assert_eq!(
        relation_handlers::build_full_callee_name(&mat, &Language::JavaScript, &tree, source)
            .as_deref(),
        Some("method"),
        "trivial receiver must not be preserved"
    );
    // Rust `self.method()` must be preserved for type-member dispatch.
    assert_eq!(
        relation_handlers::build_full_callee_name(&mat, &Language::Rust, &tree, source).as_deref(),
        Some("this.method"),
        "rust trivial this should be preserved as qualified"
    );
    let mut rust_self_mat = QueryMatch {
        index: 0,
        pattern_index: 0,
        captures: vec![
            Capture {
                name: "call.method.object".to_string(),
                text: "self".to_string(),
                start_byte: 0,
                end_byte: 4,
                start_point: (0, 0),
                end_point: (0, 4),
            },
            Capture {
                name: "call.method.function".to_string(),
                text: "clone".to_string(),
                start_byte: 5,
                end_byte: 10,
                start_point: (0, 5),
                end_point: (0, 10),
            },
        ],
    };
    let rust_source = "self.clone()";
    let (rust_tree, _) = ast_parser
        .parse_with_tree(rust_source, &Language::Rust)
        .expect("Failed to parse");
    assert_eq!(
        relation_handlers::build_full_callee_name(
            &rust_self_mat,
            &Language::Rust,
            &rust_tree,
            rust_source
        )
        .as_deref(),
        Some("self.clone"),
        "rust self.clone must be preserved"
    );
    rust_self_mat.captures[0].text = "Self".to_string();
    assert_eq!(
        relation_handlers::build_full_callee_name(
            &rust_self_mat,
            &Language::Rust,
            &rust_tree,
            rust_source
        )
        .as_deref(),
        Some("Self.clone"),
        "rust self.clone must be preserved"
    );
}

#[test]
fn test_c_dispatch_table_function_reference() {
    // A dispatch-table row mentions the handler without calling it: the row
    // must yield a file-level FunctionReference edge for the handler. The
    // macro tag and the constant in the same row are captured raw as well;
    // non-function targets are dropped later at resolution time.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "static int handle_stts(void);\nstatic int KIND;\nstatic void *table[] = {\n    { 1, KIND, handle_stts },\n};\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::C)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::C)
        .expect("extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::C, &entities, Some(1))
        .expect("extract relations");

    let funcrefs: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::FunctionReference)
        .collect();
    let handler = funcrefs
        .iter()
        .find(|r| r.dst_name() == "handle_stts")
        .expect("table handler must be a FunctionReference edge");
    assert_eq!(
        handler.caller_level,
        cce_types::RelationLevel::File,
        "file-scope table edge must be file-level"
    );

    // A table built inside a function body attributes the edge to that function.
    let scoped = "int handle(void);\nint run(void) {\n    int (*h[])(void) = { handle };\n    return 0;\n}\n";
    let tree = ast_parser
        .parse_with_tree(scoped, &Language::C)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, scoped, &Language::C)
        .expect("extract entities");
    let relations = relation_extractor
        .extract(&tree, scoped, &Language::C, &entities, Some(1))
        .expect("extract relations");
    assert!(
        relations
            .iter()
            .any(|r| r.relation_type == RelationType::FunctionReference
                && r.dst_name() == "handle"
                && r.caller_level == cce_types::RelationLevel::Entity),
        "in-function table edge must be entity-level: {relations:?}"
    );
}

#[test]
fn test_c_assignment_function_reference() {
    // `h = foo;` and `h = &foo;` use the function value without calling
    // it: both must yield entity-level FunctionReference edges. A plain
    // variable copy carries no function and is captured raw as well;
    // the function-kind gate drops it at resolution time.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "int foo(void);\nint use(void) {\n    int (*h)(void);\n    h = foo;\n    h = &foo;\n    return 0;\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::C)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::C)
        .expect("extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::C, &entities, Some(1))
        .expect("extract relations");

    let funcrefs: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::FunctionReference && r.dst_name() == "foo")
        .collect();
    assert_eq!(
        funcrefs.len(),
        2,
        "both assignment forms must be captured: {relations:?}"
    );
    assert!(
        funcrefs
            .iter()
            .all(|r| r.caller_level == cce_types::RelationLevel::Entity),
        "in-function assignments must be entity-level: {relations:?}"
    );
}

#[test]
fn test_cpp_qualified_member_pointer_reference() {
    // `&S::bar` in an initializer names the member without calling it.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "struct S { static void bar(); };\nvoid (*h)() = &S::bar;\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Cpp)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Cpp)
        .expect("extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::Cpp, &entities, Some(1))
        .expect("extract relations");

    assert!(
        relations.iter().any(|r| r.relation_type == RelationType::FunctionReference
            && r.dst_name() == "S::bar"),
        "qualified member pointer must be a FunctionReference: {relations:?}"
    );
}
