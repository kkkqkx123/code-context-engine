use super::*;

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
        .join("cce-e2e-tests")
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
