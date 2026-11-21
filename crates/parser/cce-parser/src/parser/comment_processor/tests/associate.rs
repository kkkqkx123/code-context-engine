use super::*;

#[test]
fn test_associate_comments_forward() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "/// Test documentation\nfn test() {}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("fn test").unwrap(),
        code.len(),
        1,
        1,
        "test",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert_eq!(
        entities[0].doc_comment,
        Some("Test documentation".to_string())
    );
}

#[test]
fn test_outer_doc_preserved_across_inline_attribute() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "/// Doc\n#[inline]\nfn get() {}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("fn get").unwrap(),
        code.len(),
        2,
        2,
        "get",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert_eq!(entities[0].doc_comment.as_deref(), Some("Doc"));
}

#[test]
fn test_outer_doc_preserved_across_multi_line_derive() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "/// Doc\n#[derive(\n    Debug,\n    Clone,\n)]\nstruct S {}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("struct S").unwrap(),
        code.len(),
        5,
        5,
        "S",
        cce_types::EntityKind::Struct,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert_eq!(entities[0].doc_comment.as_deref(), Some("Doc"));
}

#[test]
fn test_body_plain_comment_goes_to_containing_function() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "fn demo() {\n    // body note\n    let x = 1;\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        0,
        code.len(),
        0,
        3,
        "demo",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "plain comment must not become doc"
    );
    let facts = behavior.get(EntityId(1)).expect("comment fact expected");
    assert!(facts.facts.iter().any(|f| f.text.contains("body note")));
    assert!(
        facts.facts.iter().all(|f| !f.text.contains("//")),
        "fragment text must be cleaned"
    );
}

#[test]
fn test_file_header_plain_comment_goes_to_sentinel() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code =
        "// There's a lot of scary concurrent code\n// in this module\nuse std::sync::Once;\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("use std::sync::Once").unwrap(),
        code.len(),
        2,
        2,
        "X",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let facts = behavior
        .get(FILE_DOC_SENTINEL_ID)
        .expect("sentinel facts expected");
    assert!(
        facts
            .facts
            .iter()
            .any(|f| f.text.contains("scary concurrent code"))
    );
    assert!(entities[0].doc_comment.is_none());
}

#[test]
fn test_body_outer_doc_not_attached_and_not_behavior() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "fn demo() {\n    /// not a real doc\n    let x = 1;\n}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        0,
        code.len(),
        0,
        3,
        "demo",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "body /// must not become doc"
    );
    let entry = behavior.get(EntityId(1));
    if let Some(entry) = entry {
        assert!(
            entry
                .facts
                .iter()
                .all(|f| !f.text.contains("not a real doc")),
            "doc channel must not leak into behavior"
        );
    }
}

#[test]
fn test_orphan_comment_discarded() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    // Comment after the only entity, with a code line in the gap
    let code = "fn demo() {}\n// orphan\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        0,
        code.find('\n').unwrap(),
        0,
        0,
        "demo",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        behavior.get(EntityId(1)).is_none_or(|e| e.facts.is_empty()),
        "orphan comment must be dropped"
    );
    assert!(behavior.get(FILE_DOC_SENTINEL_ID).is_none());
}

#[test]
fn test_python_body_hash_comment_goes_to_owning_function() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "def helper():\n    # local note\n    return 1\n";
    let tree = parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("def helper").unwrap(),
        code.len(),
        0,
        2,
        "helper",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Python, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(
        entities[0].doc_comment.is_none(),
        "# comment must not become doc"
    );
    let facts = behavior.get(EntityId(1)).expect("comment fact expected");
    assert!(facts.facts.iter().any(|f| f.text.contains("local note")));
}

#[test]
fn test_python_file_header_hash_comment_goes_to_sentinel() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "# Flask app entry\n# More header text\nimport flask\n";
    let tree = parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("import flask").unwrap(),
        code.len(),
        2,
        2,
        "x",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Python, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let facts = behavior
        .get(FILE_DOC_SENTINEL_ID)
        .expect("sentinel facts expected");
    assert!(
        facts
            .facts
            .iter()
            .any(|f| f.text.contains("Flask app entry"))
    );
}

#[test]
fn test_comment_above_constant_attaches_to_constant() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code =
        "struct S {}\n// Three states that a OnceCell can be in\nconst INCOMPLETE: usize = 0x0;\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![
        make_entity(
            1,
            0,
            code.find("// Three").unwrap(),
            0,
            0,
            "S",
            cce_types::EntityKind::Struct,
        ),
        make_entity(
            2,
            code.find("const INCOMPLETE").unwrap(),
            code.len(),
            2,
            2,
            "INCOMPLETE",
            cce_types::EntityKind::Constant,
        ),
    ];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(entities[1].doc_comment.is_none());
    let facts = behavior.get(EntityId(2)).expect("comment fact expected");
    assert!(facts.facts.iter().any(|f| f.text.contains("Three states")));
    assert!(
        behavior.get(EntityId(1)).is_none_or(|e| e.facts.is_empty()),
        "comment before first entity must not attach to it"
    );
}
