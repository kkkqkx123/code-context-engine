use super::associator::{gap_is_adjacent, merge_plain_comment_blocks};
use super::classifier::{CommentClass, classify_comment, merge_top_level_line_comments};
use super::*;
use crate::parser::ast_parser::AstParser;
use cce_types::EntityId;

fn make_comment(text: &str, row: usize, col: usize, name: &str) -> Comment {
    Comment {
        text: text.to_string(),
        span: Span::new(0, text.len(), row, col, row, col + text.len()),
        capture_name: name.to_string(),
    }
}

fn make_entity(
    id: u64,
    start_byte: usize,
    end_byte: usize,
    start_row: usize,
    end_row: usize,
    name: &str,
    kind: cce_types::EntityKind,
) -> Entity {
    Entity {
        id: EntityId(id),
        kind,
        name: name.to_string(),
        signature: String::new(),
        parameters: vec![],
        return_type: None,
        span: Span::new(start_byte, end_byte, start_row, 0, end_row, 0),
        depth: 0,
        parent: None,
        children: vec![],
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: std::collections::HashMap::new(),
        metadata: std::collections::HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

#[test]
fn test_classify_comment_by_marker() {
    assert_eq!(
        classify_comment(&make_comment("/// doc", 0, 0, "comment.line")),
        CommentClass::OuterDoc
    );
    assert_eq!(
        classify_comment(&make_comment("//! inner", 0, 0, "comment.doc")),
        CommentClass::InnerDoc
    );
    assert_eq!(
        classify_comment(&make_comment("/*! inner block */", 0, 0, "comment.block")),
        CommentClass::InnerDoc
    );
    assert_eq!(
        classify_comment(&make_comment("#! shebang", 0, 0, "comment.line")),
        CommentClass::InnerDoc
    );
    assert_eq!(
        classify_comment(&make_comment("\"\"\"doc\"\"\"", 0, 0, "comment.docstring")),
        CommentClass::Docstring
    );
    assert_eq!(
        classify_comment(&make_comment("/* block */", 0, 0, "comment.block")),
        CommentClass::DocBlock
    );
    assert_eq!(
        classify_comment(&make_comment("<!-- html -->", 0, 0, "comment.block")),
        CommentClass::DocBlock
    );
    assert_eq!(
        classify_comment(&make_comment("// plain", 0, 0, "comment.line")),
        CommentClass::Plain
    );
    assert_eq!(
        classify_comment(&make_comment("# plain", 0, 0, "comment.line")),
        CommentClass::Plain
    );

    // Marker-less captures fall back to capture name
    assert_eq!(
        classify_comment(&make_comment("str content", 0, 0, "comment.docstring")),
        CommentClass::Docstring
    );
    assert_eq!(
        classify_comment(&make_comment("content", 0, 0, "comment.doc")),
        CommentClass::OuterDoc
    );
    assert_eq!(
        classify_comment(&make_comment("generic", 0, 0, "comment")),
        CommentClass::Plain
    );
}

#[test]
fn test_merge_top_level_line_comments_only_keeps_doc_markers() {
    let comments = vec![
        Comment {
            text: "//! crate docs".to_string(),
            span: Span::new(0, 14, 0, 0, 0, 14),
            capture_name: "doc_comment".to_string(),
        },
        Comment {
            text: "//! more docs".to_string(),
            span: Span::new(15, 28, 1, 0, 1, 13),
            capture_name: "doc_comment".to_string(),
        },
    ];

    let merged = merge_top_level_line_comments(comments);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].text, "//! crate docs\n//! more docs");
}

#[test]
fn test_merge_top_level_line_comments_preserves_indented_doc() {
    let comments = vec![
        Comment {
            text: "/// Compile the given pattern".to_string(),
            span: Span::new(0, 28, 10, 4, 10, 32),
            capture_name: "comment.line".to_string(),
        },
        Comment {
            text: "/// Returns a mutable reference".to_string(),
            span: Span::new(29, 58, 15, 4, 15, 33),
            capture_name: "comment.line".to_string(),
        },
    ];

    let merged = merge_top_level_line_comments(comments);
    assert_eq!(
        merged.len(),
        2,
        "indented /// doc comments should be preserved"
    );
    assert!(merged[0].text.contains("Compile"));
    assert!(merged[1].text.contains("Returns"));
}

#[test]
fn test_merge_top_level_line_comments_merges_consecutive_only() {
    let comments = vec![
        Comment {
            text: "/// Doc for method A".to_string(),
            span: Span::new(0, 20, 10, 4, 10, 24),
            capture_name: "comment.line".to_string(),
        },
        Comment {
            text: "/// More doc for A".to_string(),
            span: Span::new(21, 38, 11, 4, 11, 22),
            capture_name: "comment.line".to_string(),
        },
        Comment {
            text: "/// Doc for method B".to_string(),
            span: Span::new(100, 120, 15, 4, 15, 24),
            capture_name: "comment.line".to_string(),
        },
    ];

    let merged = merge_top_level_line_comments(comments);
    assert_eq!(merged.len(), 2, "non-consecutive docs should be separate");
    assert!(merged[0].text.contains("Doc for method A"));
    assert!(merged[0].text.contains("More doc for A"));
    assert!(merged[1].text.contains("Doc for method B"));
}

#[test]
fn test_merge_plain_comment_blocks() {
    let comments = vec![
        Comment {
            text: "// first".to_string(),
            span: Span::new(0, 8, 0, 0, 0, 8),
            capture_name: "comment.line".to_string(),
        },
        Comment {
            text: "// second".to_string(),
            span: Span::new(9, 18, 1, 0, 1, 9),
            capture_name: "comment.line".to_string(),
        },
        // Separated by one row: not merged
        Comment {
            text: "// far".to_string(),
            span: Span::new(19, 26, 3, 0, 3, 7),
            capture_name: "comment.line".to_string(),
        },
        // Doc marker comments are not merged here
        Comment {
            text: "/// doc".to_string(),
            span: Span::new(27, 34, 4, 0, 4, 7),
            capture_name: "comment.line".to_string(),
        },
    ];

    let merged = merge_plain_comment_blocks(&comments);
    assert_eq!(
        merged.len(),
        2,
        "plain comments merge, doc markers excluded"
    );
    assert!(merged[0].text.contains("first"));
    assert!(merged[0].text.contains("second"));
    assert!(merged[1].text.contains("far"));
}

#[test]
fn test_gap_is_adjacent() {
    // Blank lines only
    assert!(gap_is_adjacent("/// doc\n\nfn f() {}", 7, 9));
    // Attribute lines
    assert!(gap_is_adjacent("/// doc\n#[inline]\nfn f() {}", 7, 18));
    // Multi-line derive
    assert!(gap_is_adjacent(
        "/// doc\n#[derive(\n Debug,\n Clone,\n)]\nfn f() {}",
        7,
        42
    ));
    // Java override
    assert!(gap_is_adjacent(
        "/** doc */\n@Override\nvoid f() {}",
        10,
        21
    ));
    // Empty gap (comment ends exactly where the entity starts)
    assert!(gap_is_adjacent("/// doc", 7, 7));
    // Code in the gap
    assert!(!gap_is_adjacent("/// doc\nfn other() {}\nfn f() {}", 7, 9));
    // Another comment in the gap
    assert!(!gap_is_adjacent("/// doc\n// plain\nfn f() {}", 7, 9));
}

#[test]
fn test_clean_doc_comment_rust() {
    let text = "/// This is a doc comment\n/// with multiple lines";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a doc comment with multiple lines");
}

#[test]
fn test_clean_doc_comment_javadoc() {
    let text = "/**\n * This is a javadoc\n * comment\n */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a javadoc comment");
}

#[test]
fn test_clean_doc_comment_python_docstring() {
    // Single line docstring with double quotes
    let text = "\"\"\"This is a Python docstring\"\"\"";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a Python docstring");

    // Multi-line docstring with double quotes
    let text = "\"\"\"This is a multi-line\ndocstring for Python\nwith multiple lines\"\"\"";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(
        cleaned,
        "This is a multi-line\ndocstring for Python\nwith multiple lines"
    );

    // Single quotes docstring
    let text = "'''Single quote docstring'''";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Single quote docstring");
}

#[test]
fn test_clean_doc_comment_block() {
    // C-style block comment
    let text = "/* This is a block comment */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a block comment");

    // Multi-line block comment
    let text = "/*\n * Line 1\n * Line 2\n */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Line 1 Line 2");
}

#[test]
fn test_clean_rust_doc_comment_multiline_preserving() {
    // Test the actual case from OnceCell
    let text = "/// A cell which can be written to only once. It is not thread safe.\n\
                    ///\n\
                    /// Unlike [`std::cell::RefCell`], a `OnceCell` provides simple `&`\n\
                    /// references to the contents.\n\
                    ///\n\
                    /// # Example\n\
                    /// Some example code here";

    let cleaned = clean_doc_comment_impl(text, true);

    // Should preserve multiple lines with content
    assert!(
        cleaned.contains("A cell which"),
        "First line should be present"
    );
    assert!(
        cleaned.contains("Unlike"),
        "Second paragraph should be present"
    );
    assert!(
        cleaned.contains("Example"),
        "Section header should be present"
    );
}

#[test]
fn test_clean_doc_comment_generic_comment() {
    // Generic comment (from common query)
    let text = "/* Generic comment without specific markers */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Generic comment without specific markers");
}

#[test]
fn test_extract_comments_c_keeps_all_and_classifies() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = r#"
// This is a single line comment
/**
 * This is a block comment (kept)
 */
int global = 0;

// Another single line
int test() {
    return 0;
}
"#;

    let tree = parser
        .parse_with_tree(code, &Language::C)
        .expect("Failed to parse")
        .0;

    let comments = processor
        .extract_comments(&tree, code, &Language::C)
        .expect("Failed to extract comments");

    // All comments are collected, both line and block
    assert!(
        comments
            .iter()
            .any(|c| c.text.contains("single line comment"))
    );
    assert!(comments.iter().any(|c| c.text.contains("block comment")));
    assert!(
        comments
            .iter()
            .any(|c| c.text.contains("Another single line"))
    );
}

#[test]
fn test_extract_comments_rust() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = r#"
// Single line comment
/// Doc comment
fn documented() {}

//! Module doc
mod test {}
"#;

    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let comments = processor
        .extract_comments(&tree, code, &Language::Rust)
        .expect("Failed to extract comments");

    assert!(
        comments
            .iter()
            .any(|c| c.text.contains("Single line comment"))
    );
    assert!(comments.iter().any(|c| c.text.contains("Doc comment")));
    assert!(comments.iter().any(|c| c.text.contains("Module doc")));
}

#[test]
fn test_extract_comments_rust_merges_top_level_doc_block() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = r#"//! Title
//!
//! ```rust
//! fn main() {}
//! ```
fn documented() {}
"#;

    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let comments = processor
        .extract_comments(&tree, code, &Language::Rust)
        .expect("Failed to extract comments");

    assert_eq!(comments.len(), 1, "Expected a merged doc comment block");
    assert!(comments[0].text.contains("Title"));
    assert!(comments[0].text.contains("```rust"));
}

#[test]
fn test_extract_comments_java_merges_top_level_doc_block() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = r#"/* Title
 *
 * ```java
 * class Demo {}
 * ```
 */
class Demo {}
"#;

    let tree = parser
        .parse_with_tree(code, &Language::Java)
        .expect("Failed to parse")
        .0;

    let comments = processor
        .extract_comments(&tree, code, &Language::Java)
        .expect("Failed to extract comments");

    assert_eq!(comments.len(), 1, "Expected a merged doc comment block");
    assert!(comments[0].text.contains("Title"));
    assert!(comments[0].text.contains("```java"));
}

#[test]
fn test_process_rust_file_doc_comment_preserves_markdown() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let code = r#"//! # Overview
//!
//! ```rust
//! fn example() {}
//! ```
fn documented() {}
"#;

    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let fn_start = code.find("fn documented").unwrap();
    let mut entities = vec![make_entity(
        1,
        fn_start,
        code.len(),
        5,
        5,
        "documented",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    let file_doc = processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let file_doc = file_doc.expect("Expected file doc comment");
    assert!(file_doc.text.contains("# Overview"));
    assert!(file_doc.text.contains("```rust"));
    assert!(file_doc.text.contains("fn example() {}"));
    assert!(
        entities[0].doc_comment.is_none(),
        "file doc must not leak into entity"
    );
}

#[test]
fn test_process_rust_block_inner_doc_before_imports_becomes_file_doc() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();
    let code =
        "/*!\nCrate level documentation.\n*/\nuse std::collections::HashMap;\n\npub fn foo() {}\n";
    let tree = parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let foo_start = code.find("pub fn foo").expect("foo not found");
    let mut entities = vec![make_entity(
        1,
        foo_start,
        code.len(),
        4,
        5,
        "foo",
        cce_types::EntityKind::Function,
    )];
    let mut behavior = BehaviorStore::default();

    let file_doc = processor
        .process_with_span(&tree, code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let file_doc = file_doc.expect("Expected file-level doc for /*! block");
    assert!(file_doc.text.contains("Crate level documentation"));
    assert!(
        entities[0].doc_comment.is_none(),
        "/*! must not attach to the first entity"
    );
}

#[test]
fn test_process_once_cell_fixture_file_doc_comment() {
    let processor = CommentProcessor::new();
    let mut parser = AstParser::new();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_path = std::path::Path::new(manifest_dir)
        .join("..")
        .join("..")
        .join("app")
        .join("cce_e2e_tests")
        .join("fixtures")
        .join("rust")
        .join("review")
        .join("once_cell")
        .join("src")
        .join("lib.rs");

    let code = std::fs::read_to_string(&fixture_path).expect("Failed to read once_cell fixture");
    let tree = parser
        .parse_with_tree(&code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let mut entities = vec![make_entity(
        1,
        code.len().saturating_sub(1),
        code.len(),
        200,
        200,
        "once_cell",
        cce_types::EntityKind::Module,
    )];
    entities[0].span = Span::new(code.len().saturating_sub(1), code.len(), 200, 0, 200, 1);
    let mut behavior = BehaviorStore::default();

    let file_doc = processor
        .process_with_span(&tree, &code, &Language::Rust, &mut entities, &mut behavior)
        .expect("Failed to process comments");

    let file_doc = file_doc.expect("Expected once_cell file doc comment");
    assert!(file_doc.text.contains("# Overview"));
    assert!(file_doc.text.contains("```rust"));
    assert!(file_doc.text.contains("OnceCell"));
    // Tree-sitter end position points to start of line 374, which is not occupied by the span
    assert_eq!(file_doc.span.line_range_opt(), Some((1, 373)));
}

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
