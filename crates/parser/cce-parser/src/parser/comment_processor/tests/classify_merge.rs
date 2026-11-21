use super::*;

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
