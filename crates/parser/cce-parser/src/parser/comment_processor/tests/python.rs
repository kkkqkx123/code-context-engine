use super::*;

#[test]
fn test_python_module_docstring_goes_to_file() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = "\"\"\"Module overview.\nMore details here.\"\"\"\n\ndef helper():\n    pass\n";
    let tree = parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.find("def helper").unwrap(),
        code.len(),
        3,
        3,
        "helper",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    let file_doc = processor
        .process_with_span(&tree, code, &Language::Python, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let file_doc = file_doc.expect("module docstring should be file-level");
    assert!(file_doc.text.contains("Module overview"));
    assert!(
        entities[0].doc_comment.is_none(),
        "module docstring must not attach to first entity"
    );
}

#[test]
fn test_python_nested_function_docstring_goes_to_innermost() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code =
        "def outer():\n    def inner():\n        \"\"\"Inner doc.\"\"\"\n        pass\n    pass\n";
    let tree = parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![
        make_entity(
            1,
            0,
            code.len(),
            0,
            5,
            "outer",
            cce_types::EntityKind::Function,
        ),
        make_entity(
            2,
            code.find("def inner").unwrap(),
            code.len(),
            1,
            3,
            "inner",
            cce_types::EntityKind::Function,
        ),
    ];
    let mut behavior = BehaviorStore::default();

    processor
        .process_with_span(&tree, code, &Language::Python, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    assert!(entities[0].doc_comment.is_none());
    assert_eq!(entities[1].doc_comment.as_deref(), Some("Inner doc."));
}
