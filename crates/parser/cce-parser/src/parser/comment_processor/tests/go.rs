use super::*;

#[test]
fn test_is_go_doc_line_markers() {
    assert!(is_go_doc_line("// Returns the handle."));
    assert!(is_go_doc_line("   // indented remark"));
    assert!(!is_go_doc_line("/// Rust style doc"));
    assert!(!is_go_doc_line("//! inner doc"));
    assert!(!is_go_doc_line("//go:noinline"));
    assert!(!is_go_doc_line("//go:build linux"));
    assert!(!is_go_doc_line("/* block comment */"));
}

#[test]
fn test_go_doc_run_attaches_without_blank_line() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "package gin\n\n// Returns the handle registered with the given path.\n// Wildcard values are saved to a map.\nfunc getValue(path string) int {\n\treturn 0\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("func getValue").unwrap(),
        code.len(),
        4,
        6,
        "getValue",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Go, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let doc = entities[0]
        .doc_comment
        .as_deref()
        .expect("Go doc comment must attach");
    assert!(
        doc.contains("Returns the handle registered with the given path."),
        "doc text missing, got: {}",
        doc
    );
    assert!(
        doc.contains("Wildcard values are saved to a map."),
        "second doc line missing, got: {}",
        doc
    );
    assert!(!doc.contains("//"), "comment markers must be stripped");
}

#[test]
fn test_go_doc_run_rejected_across_blank_line() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "package gin\n\n// Returns the handle registered with the given path.\n\nfunc getValue(path string) int {\n\treturn 0\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("func getValue").unwrap(),
        code.len(),
        4,
        6,
        "getValue",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Go, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "comment across a blank line is not documentation"
    );
}

#[test]
fn test_go_directive_is_not_documentation() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "package gin\n\n//go:noinline\nfunc getValue(path string) int {\n\treturn 0\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("func getValue").unwrap(),
        code.len(),
        3,
        5,
        "getValue",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Go, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "compiler directives must not become documentation"
    );
}

#[test]
fn test_go_doc_consumed_not_duplicated_as_behavior() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "package gin\n\n// First does one thing.\nfunc first() {}\n\n// Returns the handle registered with the given path.\n// Wildcard values are saved to a map.\nfunc second(path string) int {\n\treturn 0\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![
        make_entity(
            1,
            code.find("func first").unwrap(),
            code.find("func second").unwrap(),
            3,
            3,
            "first",
            cce_types::EntityKind::Function,
        ),
        make_entity(
            2,
            code.find("func second").unwrap(),
            code.len(),
            7,
            9,
            "second",
            cce_types::EntityKind::Function,
        ),
    ];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Go, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert_eq!(
        entities[0].doc_comment.as_deref(),
        Some("First does one thing.")
    );
    assert!(
        entities[1]
            .doc_comment
            .as_deref()
            .is_some_and(|doc| doc.contains("Returns the handle registered with the given path.")),
        "second function doc missing"
    );
    assert!(
        behavior.is_empty(),
        "consumed doc runs must not leak into behavior facts"
    );
}

#[test]
fn test_go_inner_remark_is_not_documentation() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "package gin\n\nfunc first() {\n\t// walk the tree\n\t_ = 1\n}\n\n// Documents second.\nfunc second() {}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Go)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![
        make_entity(
            1,
            code.find("func first").unwrap(),
            code.find("func second").unwrap(),
            2,
            5,
            "first",
            cce_types::EntityKind::Function,
        ),
        make_entity(
            2,
            code.find("func second").unwrap(),
            code.len(),
            8,
            8,
            "second",
            cce_types::EntityKind::Function,
        ),
    ];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Go, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "inner remarks must not document the enclosing function"
    );
    assert_eq!(
        entities[1].doc_comment.as_deref(),
        Some("Documents second.")
    );
}
